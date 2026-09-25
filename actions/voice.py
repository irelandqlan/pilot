"""
Voice Typing Service for Chromecast Remote.
Listens to ATVVoice BLE microphone signals, records audio during hold-to-talk,
transcribes using faster-whisper, and types/pastes text into the active window.
"""

import os
import time
import shutil
import logging
import tempfile
import threading
import subprocess
import ctypes
from pathlib import Path

try:
    from gi.repository import Gio, GLib
    HAS_GI = True
except Exception:
    HAS_GI = False

# Preload NVIDIA CUDA runtime libraries if present in venv
try:
    site_pkg = Path(__file__).resolve().parent.parent / "venv" / "lib64" / "python3.14" / "site-packages"
    if not site_pkg.exists():
        site_pkg = Path(__file__).resolve().parent.parent / "venv" / "lib" / "python3.14" / "site-packages"
    for _p in site_pkg.glob("nvidia/*/lib"):
        for _so in _p.glob("*.so*"):
            try:
                ctypes.CDLL(str(_so))
            except Exception:
                pass
except Exception:
    pass

logger = logging.getLogger(__name__)

class VoiceTypingService:
    def __init__(self, emit_key_combination_fn, config: dict, type_string_fn=None):
        self.emit_key_combination = emit_key_combination_fn
        self.type_string_fn = type_string_fn
        self.config = config.get("voice", {})
        self.enabled = self.config.get("enabled", True)
        self.model_name = self.config.get("model", "tiny.en")
        self.paste_method = self.config.get("paste_method", "keystrokes")
        self.min_duration = self.config.get("min_duration_sec", 0.35)
        self.device = self.config.get("device", "cpu")
        self.compute_type = self.config.get("compute_type", "int8")

        self.model = None
        self.model_loading = False
        self.model_load_error = None
        self.record_process = None
        self.record_start_time = 0.0
        self.temp_wav = os.path.join(tempfile.gettempdir(), "chromecast_voice_input.wav")
        self.lock = threading.Lock()

        if self.enabled:
            # Preload Whisper in background thread
            threading.Thread(target=self._load_model, daemon=True).start()
            # Start D-Bus signal listener
            threading.Thread(target=self._start_dbus_listener, daemon=True).start()

    def _load_model(self):
        """Preload the WhisperModel into memory so transcription has zero startup delay."""
        try:
            self.model_loading = True
            self.model_load_error = None
            logger.info(f"Loading Whisper model '{self.model_name}' on {self.device} ({self.compute_type})...")
            from faster_whisper import WhisperModel
            t0 = time.time()
            if self.device == "cuda":
                self.model = WhisperModel(self.model_name, device="cuda", compute_type=self.compute_type)
            else:
                self.model = WhisperModel(self.model_name, device="cpu", compute_type=self.compute_type, cpu_threads=6)
            logger.info(f"Whisper model loaded in {time.time() - t0:.2f}s.")
        except Exception as e:
            self.model_load_error = str(e)
            logger.error(f"Failed to load Whisper model: {e}")
        finally:
            self.model_loading = False

    def _start_dbus_listener(self):
        """Subscribes to ATVVoice D-Bus signals for mic state changes."""
        try:
            import asyncio
            from dbus_fast.aio import MessageBus
            from dbus_fast.constants import BusType

            async def _run_dbus_fast():
                bus = await MessageBus(bus_type=BusType.SESSION).connect()
                while True:
                    try:
                        intro = await bus.introspect("org.atvvoice.chromecast-remote", "/org/atvvoice/Daemon")
                        proxy = bus.get_proxy_object("org.atvvoice.chromecast-remote", "/org/atvvoice/Daemon", intro)
                        iface = proxy.get_interface("org.atvvoice.Daemon")
                        iface.on_mic_state_changed(self._handle_mic_state_str)
                        logger.info("Subscribed to ATVVoice MicStateChanged signals via dbus-fast.")
                        await bus.wait_for_disconnect()
                    except Exception as err:
                        logger.debug(f"Waiting for ATVVoice D-Bus service: {err}")
                        await asyncio.sleep(1.0)

            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            loop.run_until_complete(_run_dbus_fast())
            return
        except ImportError:
            pass
        except Exception as e:
            logger.error(f"Error in dbus-fast listener: {e}")

        if HAS_GI:
            try:
                bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
                bus.signal_subscribe(
                    "org.atvvoice.chromecast-remote",
                    "org.atvvoice.Daemon",
                    "MicStateChanged",
                    "/org/atvvoice/Daemon",
                    None,
                    Gio.DBusSignalFlags.NONE,
                    self._on_mic_state_changed,
                    None
                )
                logger.info("Subscribed to ATVVoice MicStateChanged signals via Gio.")
                loop = GLib.MainLoop()
                loop.run()
                return
            except Exception as e:
                logger.error(f"Error in ATVVoice Gio D-Bus listener: {e}")

        logger.error("Neither dbus-fast nor PyGObject (gi) is available for D-Bus listener.")

    def _handle_mic_state_str(self, state: str):
        """Called when ATVVoice signals mic open/streaming or closed."""
        logger.debug(f"ATVVoice MicStateChanged: {state}")
        if state in ("streaming", "opening"):
            self.start_recording()
        elif state in ("connected", "idle", "closed"):
            self.stop_and_transcribe()

    def _on_mic_state_changed(self, conn, sender, path, iface, signal, params, user_data):
        """Called when ATVVoice signals mic open/streaming or closed via Gio."""
        try:
            state = params.unpack()[0]
            self._handle_mic_state_str(state)
        except Exception as e:
            logger.error(f"Error handling MicStateChanged: {e}")

    def start_recording(self):
        with self.lock:
            if self.record_process is not None:
                return  # Already recording

            if os.path.exists(self.temp_wav):
                try:
                    os.remove(self.temp_wav)
                except OSError:
                    pass

            logger.info("Voice button pressed. Recording audio from remote mic...")
            self.record_start_time = time.time()
            try:
                self.record_process = subprocess.Popen(
                    [
                        "pw-record",
                        "--target", "atvvoice-chromecast-remote",
                        self.temp_wav
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL
                )
            except Exception as e:
                logger.error(f"Failed to start pw-record: {e}")
                self.record_process = None

    def stop_and_transcribe(self):
        proc = None
        duration = 0.0
        with self.lock:
            if self.record_process is None:
                return
            proc = self.record_process
            self.record_process = None
            duration = time.time() - self.record_start_time

        logger.info(f"Voice button released. Recorded {duration:.2f}s of audio.")
        if proc:
            try:
                proc.terminate()
                proc.wait(timeout=1.0)
            except Exception:
                try:
                    proc.kill()
                except Exception:
                    pass

        if duration < self.min_duration:
            logger.info(f"Audio duration ({duration:.2f}s) shorter than threshold ({self.min_duration}s), ignoring.")
            return

        # Run transcription in a background worker thread to keep daemon responsive
        threading.Thread(target=self._transcribe_worker, daemon=True).start()

    def _transcribe_worker(self):
        """Transcribes the recorded audio file and outputs the text."""
        if not os.path.exists(self.temp_wav):
            return

        if self.model is None:
            if self.model_load_error:
                logger.error(f"Cannot transcribe: Whisper model failed to load ({self.model_load_error}).")
            elif self.model_loading:
                logger.warning("Whisper model is still loading, please wait a moment...")
            else:
                logger.error("Whisper model is not loaded.")
            return

        try:
            # Clean and normalize audio: strip DC bias and normalize peak volume
            try:
                import wave
                import numpy as np
                with wave.open(self.temp_wav, "rb") as wf:
                    params = wf.getparams()
                    frames = wf.readframes(wf.getnframes())
                if frames:
                    samples = np.frombuffer(frames, dtype=np.int16).astype(np.float32)
                    # Strip DC bias
                    samples = samples - np.mean(samples)
                    # Peak normalize to -1 dBFS (28000 / 32767)
                    max_abs = np.max(np.abs(samples))
                    if max_abs > 0:
                        samples = (samples / max_abs) * 28000.0
                    with wave.open(self.temp_wav, "wb") as wf:
                        wf.setparams(params)
                        wf.writeframes(samples.astype(np.int16).tobytes())
            except Exception as e:
                logger.warning(f"Audio cleanup warning: {e}")

            debug_path = str(Path(__file__).resolve().parent.parent / "debug_last_voice.wav")
            try:
                shutil.copyfile(self.temp_wav, debug_path)
            except Exception:
                pass

            t0 = time.time()
            segments, info = self.model.transcribe(
                self.temp_wav,
                beam_size=1,
                temperature=0.0,
                condition_on_previous_text=False,
                vad_filter=True,
                vad_parameters=dict(min_silence_duration_ms=250)
            )
            text = " ".join(seg.text for seg in segments).strip()
            elapsed = time.time() - t0
            logger.info(f"Voice transcribed in {elapsed:.2f}s: '{text}'")

            if text:
                # Auto-spacing engine: append trailing space for continuous dictation
                if self.config.get("auto_spacing", True) and not text.endswith(" "):
                    text = text + " "
                self.type_text(text)
        except Exception as e:
            logger.error(f"Error during transcription: {e}")
        finally:
            if os.path.exists(self.temp_wav):
                try:
                    os.remove(self.temp_wav)
                except OSError:
                    pass

    def type_text(self, text: str):
        """Pastes or types the transcribed text into the active window."""
        if self.paste_method == "keystrokes" and self.type_string_fn:
            logger.info(f"Direct typing via keystrokes: '{text}'")
            self.type_string_fn(text)
        elif self.paste_method == "clipboard" and shutil.which("wl-copy"):
            try:
                # Use wl-copy to put text on clipboard
                p = subprocess.Popen(["wl-copy", "--", text], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                p.wait(timeout=1.0)
                time.sleep(0.04)
                # Paste with Ctrl+V
                self.emit_key_combination(["KEY_LEFTCTRL", "KEY_V"])
            except Exception as e:
                logger.error(f"Failed to paste text via wl-copy: {e}")
        else:
            logger.info(f"Typing text fallback: '{text}'")
            if self.type_string_fn:
                self.type_string_fn(text)
            else:
                subprocess.Popen(["wl-copy", "--", text], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                time.sleep(0.04)
                self.emit_key_combination(["KEY_LEFTCTRL", "KEY_V"])
