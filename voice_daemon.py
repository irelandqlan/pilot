#!/usr/bin/env python3
"""
Chromecast Voice Dictation Daemon
Standalone speech-to-text service listening to ATVVoice BLE microphone signals
and providing a session D-Bus interface for desktop-wide voice typing.
"""

import os
import sys
import time
import signal
import shutil
import logging
import tempfile
import threading
import subprocess
import ctypes
from pathlib import Path

# Compatibility fix for faster-whisper with PyAV >= 19.0.0 (metadata_errors argument removed)
try:
    import av
    _orig_av_open = av.open
    def _compat_av_open(*args, **kwargs):
        kwargs.pop("metadata_errors", None)
        return _orig_av_open(*args, **kwargs)
    av.open = _compat_av_open
except Exception:
    pass

# Load config from config.toml (with XDG user config support)
def get_config_path() -> Path:
    if "PILOT_CONFIG" in os.environ:
        return Path(os.environ["PILOT_CONFIG"])
    if "CHROMECAST_REMOTE_CONFIG" in os.environ:
        return Path(os.environ["CHROMECAST_REMOTE_CONFIG"])
    pilot_cfg = Path.home() / ".config" / "pilot" / "config.toml"
    legacy_cfg = Path.home() / ".config" / "chromecast-remote" / "config.toml"

    # Automatically migrate legacy config to pilot
    if not pilot_cfg.exists() and legacy_cfg.exists():
        try:
            pilot_cfg.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(legacy_cfg, pilot_cfg)
            return pilot_cfg
        except Exception:
            pass

    if pilot_cfg.exists():
        return pilot_cfg
    if legacy_cfg.exists():
        return legacy_cfg
    dev_cfg = Path(__file__).parent / "config.toml"
    if dev_cfg.exists():
        return dev_cfg
    sys_cfg = Path("/usr/share/pilot/config.toml")
    if sys_cfg.exists():
        try:
            pilot_cfg.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(sys_cfg, pilot_cfg)
            return pilot_cfg
        except Exception:
            return sys_cfg
    return pilot_cfg

CONFIG_FILE = get_config_path()

def load_voice_config():
    if sys.version_info >= (3, 11):
        import tomllib
    else:
        import tomli as tomllib
    cfg_path = get_config_path()
    if cfg_path.exists():
        with open(cfg_path, "rb") as f:
            cfg = tomllib.load(f)
            return cfg.get("voice", {})
    return {}

# Preload NVIDIA CUDA runtime libraries if present in venv
try:
    site_pkg = Path(__file__).resolve().parent / "venv" / "lib64" / "python3.14" / "site-packages"
    if not site_pkg.exists():
        site_pkg = Path(__file__).resolve().parent / "venv" / "lib" / "python3.14" / "site-packages"
    for _p in site_pkg.glob("nvidia/*/lib"):
        for _so in _p.glob("*.so*"):
            try:
                ctypes.CDLL(str(_so))
            except Exception:
                pass
except Exception:
    pass

import evdev
from evdev import ecodes, UInput

logging.basicConfig(
    level=logging.INFO,
    format="[%(asctime)s] [Voice %(levelname)s] %(message)s",
    datefmt="%H:%M:%S"
)
logger = logging.getLogger("VoiceDaemon")

CHAR_MAP = {
    'a': (ecodes.KEY_A, False), 'b': (ecodes.KEY_B, False), 'c': (ecodes.KEY_C, False),
    'd': (ecodes.KEY_D, False), 'e': (ecodes.KEY_E, False), 'f': (ecodes.KEY_F, False),
    'g': (ecodes.KEY_G, False), 'h': (ecodes.KEY_H, False), 'i': (ecodes.KEY_I, False),
    'j': (ecodes.KEY_J, False), 'k': (ecodes.KEY_K, False), 'l': (ecodes.KEY_L, False),
    'm': (ecodes.KEY_M, False), 'n': (ecodes.KEY_N, False), 'o': (ecodes.KEY_O, False),
    'p': (ecodes.KEY_P, False), 'q': (ecodes.KEY_Q, False), 'r': (ecodes.KEY_R, False),
    's': (ecodes.KEY_S, False), 't': (ecodes.KEY_T, False), 'u': (ecodes.KEY_U, False),
    'v': (ecodes.KEY_V, False), 'w': (ecodes.KEY_W, False), 'x': (ecodes.KEY_X, False),
    'y': (ecodes.KEY_Y, False), 'z': (ecodes.KEY_Z, False),
    'A': (ecodes.KEY_A, True),  'B': (ecodes.KEY_B, True),  'C': (ecodes.KEY_C, True),
    'D': (ecodes.KEY_D, True),  'E': (ecodes.KEY_E, True),  'F': (ecodes.KEY_F, True),
    'G': (ecodes.KEY_G, True),  'H': (ecodes.KEY_H, True),  'I': (ecodes.KEY_I, True),
    'J': (ecodes.KEY_J, True),  'K': (ecodes.KEY_K, True),  'L': (ecodes.KEY_L, True),
    'M': (ecodes.KEY_M, True),  'N': (ecodes.KEY_N, True),  'O': (ecodes.KEY_O, True),
    'P': (ecodes.KEY_P, True),  'Q': (ecodes.KEY_Q, True),  'R': (ecodes.KEY_R, True),
    'S': (ecodes.KEY_S, True),  'T': (ecodes.KEY_T, True),  'U': (ecodes.KEY_U, True),
    'V': (ecodes.KEY_V, True),  'W': (ecodes.KEY_W, True),  'X': (ecodes.KEY_X, True),
    'Y': (ecodes.KEY_Y, True),  'Z': (ecodes.KEY_Z, True),
    '0': (ecodes.KEY_0, False), '1': (ecodes.KEY_1, False), '2': (ecodes.KEY_2, False),
    '3': (ecodes.KEY_3, False), '4': (ecodes.KEY_4, False), '5': (ecodes.KEY_5, False),
    '6': (ecodes.KEY_6, False), '7': (ecodes.KEY_7, False), '8': (ecodes.KEY_8, False),
    '9': (ecodes.KEY_9, False), ' ': (ecodes.KEY_SPACE, False),
    '!': (ecodes.KEY_1, True),  '@': (ecodes.KEY_2, True),  '#': (ecodes.KEY_3, True),
    '$': (ecodes.KEY_4, True),  '%': (ecodes.KEY_5, True),  '^': (ecodes.KEY_6, True),
    '&': (ecodes.KEY_7, True),  '*': (ecodes.KEY_8, True),  '(': (ecodes.KEY_9, True),
    ')': (ecodes.KEY_0, True),  '-': (ecodes.KEY_MINUS, False), '_': (ecodes.KEY_MINUS, True),
    '=': (ecodes.KEY_EQUAL, False), '+': (ecodes.KEY_EQUAL, True),
    '[': (ecodes.KEY_LEFTBRACE, False), '{': (ecodes.KEY_LEFTBRACE, True),
    ']': (ecodes.KEY_RIGHTBRACE, False), '}': (ecodes.KEY_RIGHTBRACE, True),
    ';': (ecodes.KEY_SEMICOLON, False), ':': (ecodes.KEY_SEMICOLON, True),
    "'": (ecodes.KEY_APOSTROPHE, False), '"': (ecodes.KEY_APOSTROPHE, True),
    ',': (ecodes.KEY_COMMA, False), '<': (ecodes.KEY_COMMA, True),
    '.': (ecodes.KEY_DOT, False), '>': (ecodes.KEY_DOT, True),
    '/': (ecodes.KEY_SLASH, False), '?': (ecodes.KEY_SLASH, True),
    '`': (ecodes.KEY_GRAVE, False), '~': (ecodes.KEY_GRAVE, True),
    '\\': (ecodes.KEY_BACKSLASH, False), '|': (ecodes.KEY_BACKSLASH, True),
    '\n': (ecodes.KEY_ENTER, False), '\t': (ecodes.KEY_TAB, False),
}

def create_uinput_typist():
    typing_keys = [mapping[0] for mapping in CHAR_MAP.values()]
    keys = list(set(typing_keys + [
        ecodes.KEY_LEFTSHIFT, ecodes.KEY_LEFTCTRL, ecodes.KEY_V, ecodes.KEY_ENTER, ecodes.KEY_SPACE
    ]))
    capabilities = {ecodes.EV_KEY: keys}
    for attempt in range(10):
        try:
            ui = UInput(capabilities, name="Voice Dictation Typist")
            logger.info("Initialized virtual UInput typist device.")
            return ui
        except Exception as e:
            logger.warning(f"UInput creation attempt {attempt+1} failed: {e}. Retrying...")
            time.sleep(0.5)
    logger.error("Could not create UInput device. Fallback to wl-copy only.")
    return None

class VoiceDaemon:
    def __init__(self):
        self.config = load_voice_config()
        self.enabled = self.config.get("enabled", True)
        self.model_name = self.config.get("model", "medium.en")
        self.paste_method = self.config.get("paste_method", "keystrokes")
        self.min_duration = self.config.get("min_duration_sec", 0.35)
        self.device = self.config.get("device", "cuda")
        self.compute_type = self.config.get("compute_type", "float16")
        self.auto_spacing = self.config.get("auto_spacing", True)

        self.uinput = create_uinput_typist()
        self.model = None
        self.model_loading = False
        self.model_load_error = None
        self.record_process = None
        self.record_start_time = 0.0
        self.temp_wav = os.path.join(tempfile.gettempdir(), "chromecast_voice_input.wav")
        self.lock = threading.Lock()
        self.is_running = True

        signal.signal(signal.SIGINT, self._handle_signal)
        signal.signal(signal.SIGTERM, self._handle_signal)

        # Automatically terminate if parent process exits (prevents VRAM accumulation)
        try:
            libc = ctypes.CDLL("libc.so.6")
            libc.prctl(1, signal.SIGTERM)
        except Exception:
            pass

        if self.enabled:
            threading.Thread(target=self._load_model, daemon=True).start()
            threading.Thread(target=self._run_dbus_service, daemon=True).start()

    def _handle_signal(self, signum, frame):
        logger.info(f"Signal {signum} received. Stopping Voice Daemon...")
        self.is_running = False
        with self.lock:
            if self.record_process:
                try:
                    self.record_process.kill()
                except Exception:
                    pass
        sys.exit(0)

    def _load_model(self):
        try:
            self.model_loading = True
            self.model_load_error = None
            logger.info(f"Loading Whisper model '{self.model_name}' on {self.device} ({self.compute_type})...")
            from faster_whisper import WhisperModel
            t0 = time.time()

            def _init(local_only: bool):
                kwargs = {
                    "device": self.device,
                    "compute_type": self.compute_type,
                    "local_files_only": local_only,
                }
                if self.device != "cuda":
                    kwargs["cpu_threads"] = 6
                return WhisperModel(self.model_name, **kwargs)

            try:
                self.model = _init(local_only=True)
                logger.info(f"Loaded cached Whisper model '{self.model_name}' in {time.time() - t0:.2f}s.")
            except Exception:
                logger.info(f"Model '{self.model_name}' not found in local cache; downloading...")
                self.model = _init(local_only=False)
                logger.info(f"Downloaded and loaded Whisper model in {time.time() - t0:.2f}s.")
        except Exception as e:
            self.model_load_error = str(e)
            logger.error(f"Failed to load Whisper model: {e}")
        finally:
            self.model_loading = False

    def start_recording(self, target_source=None):
        with self.lock:
            if self.record_process is not None:
                return

            if os.path.exists(self.temp_wav):
                try:
                    os.remove(self.temp_wav)
                except OSError:
                    pass

            cmd = ["pw-record"]
            if target_source:
                cmd.extend(["--target", target_source])
            else:
                # Default to atvvoice remote mic if available
                cmd.extend(["--target", "atvvoice-chromecast-remote"])
            cmd.append(self.temp_wav)

            logger.info("Recording voice input...")
            self.record_start_time = time.time()
            try:
                self.record_process = subprocess.Popen(
                    cmd,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL
                )
            except Exception as e:
                logger.error(f"Failed to run pw-record: {e}")
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

        logger.info(f"Recording stopped. Duration: {duration:.2f}s")
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
            logger.info(f"Ignored: duration ({duration:.2f}s) < threshold ({self.min_duration}s).")
            return

        threading.Thread(target=self._transcribe_worker, daemon=True).start()

    def _transcribe_worker(self):
        if not os.path.exists(self.temp_wav):
            return

        if self.model is None:
            if self.model_load_error:
                logger.error(f"Cannot transcribe: Whisper failed to load ({self.model_load_error}).")
            elif self.model_loading:
                logger.warning("Whisper model is still loading, please wait...")
            else:
                logger.error("Whisper model is not available.")
            return

        try:
            # Audio normalization
            try:
                import wave
                import numpy as np
                with wave.open(self.temp_wav, "rb") as wf:
                    params = wf.getparams()
                    frames = wf.readframes(wf.getnframes())
                if frames:
                    samples = np.frombuffer(frames, dtype=np.int16).astype(np.float32)
                    samples = samples - np.mean(samples)
                    max_abs = np.max(np.abs(samples))
                    if max_abs > 0:
                        samples = (samples / max_abs) * 28000.0
                    with wave.open(self.temp_wav, "wb") as wf:
                        wf.setparams(params)
                        wf.writeframes(samples.astype(np.int16).tobytes())
            except Exception as e:
                logger.warning(f"Audio cleanup warning: {e}")

            debug_path = str(Path(__file__).resolve().parent / "debug_last_voice.wav")
            try:
                shutil.copyfile(self.temp_wav, debug_path)
            except Exception:
                pass

            t0 = time.time()
            segments, _ = self.model.transcribe(
                self.temp_wav,
                beam_size=1,
                temperature=0.0,
                condition_on_previous_text=False,
                vad_filter=True,
                vad_parameters=dict(min_silence_duration_ms=250)
            )
            text = " ".join(seg.text for seg in segments).strip()
            elapsed = time.time() - t0
            logger.info(f"Transcribed in {elapsed:.2f}s: '{text}'")

            if text:
                if self.auto_spacing and not text.endswith(" "):
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
        if self.paste_method == "keystrokes" and self.uinput:
            logger.info(f"Typing text via keystrokes: '{text}'")
            for char in text:
                if char in CHAR_MAP:
                    code, shift = CHAR_MAP[char]
                    if shift:
                        self.uinput.write(ecodes.EV_KEY, ecodes.KEY_LEFTSHIFT, 1)
                        self.uinput.syn()
                    self.uinput.write(ecodes.EV_KEY, code, 1)
                    self.uinput.syn()
                    time.sleep(0.005)
                    self.uinput.write(ecodes.EV_KEY, code, 0)
                    self.uinput.syn()
                    if shift:
                        self.uinput.write(ecodes.EV_KEY, ecodes.KEY_LEFTSHIFT, 0)
                        self.uinput.syn()
                    time.sleep(0.005)
        else:
            logger.info(f"Pasting text via clipboard: '{text}'")
            if shutil.which("wl-copy"):
                try:
                    p = subprocess.Popen(["wl-copy", "--", text], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    p.wait(timeout=1.0)
                    time.sleep(0.04)
                    if self.uinput:
                        self.uinput.write(ecodes.EV_KEY, ecodes.KEY_LEFTCTRL, 1)
                        self.uinput.write(ecodes.EV_KEY, ecodes.KEY_V, 1)
                        self.uinput.syn()
                        time.sleep(0.05)
                        self.uinput.write(ecodes.EV_KEY, ecodes.KEY_V, 0)
                        self.uinput.write(ecodes.EV_KEY, ecodes.KEY_LEFTCTRL, 0)
                        self.uinput.syn()
                except Exception as e:
                    logger.error(f"Failed to paste text: {e}")

    def _broadcast_event(self, key_name: str, state: int):
        try:
            sock_path = f"/run/user/{os.getuid()}/chromecast_remote_ui.sock"
            if os.path.exists(sock_path):
                import socket
                with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as s:
                    s.sendto(f"{key_name}:{state}".encode("utf-8"), sock_path)
        except Exception:
            pass

    def _handle_mic_state_str(self, state: str):
        logger.debug(f"ATVVoice MicStateChanged: {state}")
        test_mode_file = f"/run/user/{os.getuid()}/chromecast_remote_test_mode"
        if state in ("streaming", "opening"):
            self._broadcast_event("KEY_ASSISTANT", 1)
            if not os.path.exists(test_mode_file):
                self.start_recording("atvvoice-chromecast-remote")
            else:
                logger.info("Test mode active: ignored voice recording trigger")
        elif state in ("connected", "idle", "closed"):
            self._broadcast_event("KEY_ASSISTANT", 0)
            if not os.path.exists(test_mode_file):
                self.stop_and_transcribe()

    def _run_dbus_service(self):
        """Runs the D-Bus loop to listen for ATVVoice mic signals and handle external triggers."""
        try:
            import asyncio
            from dbus_fast.aio import MessageBus
            from dbus_fast.service import ServiceInterface, method
            from dbus_fast.constants import BusType

            daemon = self

            class DictationInterface(ServiceInterface):
                def __init__(self):
                    super().__init__("org.local.Dictation")

                @method()
                def Start(self):
                    daemon.start_recording()

                @method()
                def Stop(self):
                    daemon.stop_and_transcribe()

                @method()
                def Toggle(self):
                    if daemon.record_process is not None:
                        daemon.stop_and_transcribe()
                    else:
                        daemon.start_recording()

                @method()
                def GetStatus(self) -> 's':
                    if daemon.model_loading:
                        return "loading_model"
                    if daemon.record_process is not None:
                        return "recording"
                    return "idle"

            async def main_loop():
                bus = await MessageBus(bus_type=BusType.SESSION).connect()
                # Export Dictation service
                dictation_iface = DictationInterface()
                bus.export("/org/local/Dictation", dictation_iface)
                await bus.request_name("org.local.Dictation")
                logger.info("Exported org.local.Dictation on D-Bus session bus.")

                # Subscribe to ATVVoice signals
                while daemon.is_running:
                    try:
                        intro = await bus.introspect("org.atvvoice.chromecast-remote", "/org/atvvoice/Daemon")
                        proxy = bus.get_proxy_object("org.atvvoice.chromecast-remote", "/org/atvvoice/Daemon", intro)
                        iface = proxy.get_interface("org.atvvoice.Daemon")
                        iface.on_mic_state_changed(daemon._handle_mic_state_str)
                        logger.info("Subscribed to ATVVoice MicStateChanged signals.")
                        await bus.wait_for_disconnect()
                        break
                    except Exception as err:
                        logger.debug(f"Waiting for ATVVoice service on D-Bus: {err}")
                        await asyncio.sleep(2.0)

            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            loop.run_until_complete(main_loop())
        except Exception as e:
            logger.error(f"D-Bus service error: {e}")

if __name__ == "__main__":
    daemon = VoiceDaemon()
    logger.info("Voice Daemon running. Waiting for voice commands...")
    while daemon.is_running:
        time.sleep(1)
