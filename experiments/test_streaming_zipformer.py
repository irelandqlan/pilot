#!/usr/bin/env python3
"""
Streaming Zipformer RNN-T Sandbox Experiment.
Tests sherpa-onnx online (streaming) recognition against simulated chunks
and live microphone streaming from ATVVoice via PipeWire.
"""

import os
import sys
import time
import subprocess
from pathlib import Path
import numpy as np

# Try importing sherpa_onnx
try:
    import sherpa_onnx
except ImportError:
    print("Error: sherpa_onnx not found. Run in the project venv.")
    sys.exit(1)

PROJECT_DIR = Path(__file__).resolve().parent.parent
MODEL_DIR = PROJECT_DIR / "models" / "sherpa-onnx-streaming-zipformer-en-2023-06-26"

def create_recognizer(use_gpu=False):
    """Initialize the Sherpa-ONNX streaming Zipformer recognizer."""
    encoder = str(MODEL_DIR / "encoder-epoch-99-avg-1-chunk-16-left-128.onnx")
    decoder = str(MODEL_DIR / "decoder-epoch-99-avg-1-chunk-16-left-128.onnx")
    joiner = str(MODEL_DIR / "joiner-epoch-99-avg-1-chunk-16-left-128.onnx")
    tokens = str(MODEL_DIR / "tokens.txt")

    for f in [encoder, decoder, joiner, tokens]:
        if not os.path.exists(f):
            raise FileNotFoundError(f"Missing model file: {f}")

    print(f"Loading Zipformer model from {MODEL_DIR.name}...")
    t0 = time.time()
    recognizer = sherpa_onnx.OnlineRecognizer.from_transducer(
        tokens=tokens,
        encoder=encoder,
        decoder=decoder,
        joiner=joiner,
        num_threads=4,
        sample_rate=16000,
        feature_dim=80,
        provider="cuda" if use_gpu else "cpu",
    )
    print(f"Recognizer loaded in {time.time() - t0:.3f}s (provider: {'cuda' if use_gpu else 'cpu'}).")
    return recognizer

def test_file_stream(recognizer, wav_path, chunk_duration_sec=0.1):
    """
    Simulate streaming audio in 100ms chunks to evaluate real-time responsiveness.
    """
    import soundfile as sf
    if not os.path.exists(wav_path):
        print(f"File not found: {wav_path}")
        return

    audio, sr = sf.read(wav_path, dtype="float32")
    if audio.ndim > 1:
        audio = audio.mean(axis=1) # Mono

    if sr != 16000:
        print(f"Warning: Audio sample rate is {sr}Hz, expected 16000Hz. Resampling might be needed.")

    # Remove DC bias
    audio = audio - np.mean(audio)
    peak = np.max(np.abs(audio))
    if peak > 0:
        audio = audio / peak * 0.9

    print(f"\n--- Streaming Simulation: {Path(wav_path).name} ({len(audio)/sr:.2f}s) ---")
    print(f"Feeding chunks every {chunk_duration_sec*1000:.0f}ms:")

    stream = recognizer.create_stream()
    chunk_samples = int(sr * chunk_duration_sec)
    last_text = ""

    start_time = time.time()
    for i in range(0, len(audio), chunk_samples):
        chunk = audio[i : i + chunk_samples]
        stream.accept_waveform(sr, chunk)
        
        while recognizer.is_ready(stream):
            recognizer.decode_stream(stream)
        
        text = recognizer.get_result(stream)
        if text != last_text:
            new_words = text[len(last_text):]
            print(f"[{time.time() - start_time:.2f}s] + '{new_words}'  (Current: '{text}')")
            last_text = text

        # Simulate real-time audio cadence
        time.sleep(chunk_duration_sec * 0.5)

    # Input finished
    stream.input_finished()
    while recognizer.is_ready(stream):
        recognizer.decode_stream(stream)
    
    final_text = recognizer.get_result(stream)
    print(f"\n[Final Result]: '{final_text}'")
    print(f"Total time: {time.time() - start_time:.2f}s")

import evdev
from evdev import ecodes, UInput

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
    '.': (ecodes.KEY_DOT, False), ',': (ecodes.KEY_COMMA, False),
    '!': (ecodes.KEY_1, True),    '?': (ecodes.KEY_SLASH, True),
    "'": (ecodes.KEY_APOSTROPHE, False), '-': (ecodes.KEY_MINUS, False)
}

def create_uinput_typer():
    keys = list(set([m[0] for m in CHAR_MAP.values()] + [ecodes.KEY_BACKSPACE, ecodes.KEY_LEFTSHIFT]))
    return UInput({ecodes.EV_KEY: keys}, name="Zipformer Live Typer")

def type_str(ui, s: str):
    for ch in s:
        if ch in CHAR_MAP:
            code, shift = CHAR_MAP[ch]
            if shift:
                ui.write(ecodes.EV_KEY, ecodes.KEY_LEFTSHIFT, 1)
                ui.syn()
            ui.write(ecodes.EV_KEY, code, 1)
            ui.syn()
            time.sleep(0.005)
            ui.write(ecodes.EV_KEY, code, 0)
            ui.syn()
            if shift:
                ui.write(ecodes.EV_KEY, ecodes.KEY_LEFTSHIFT, 0)
                ui.syn()
            time.sleep(0.005)

def press_backspace(ui, count: int):
    for _ in range(count):
        ui.write(ecodes.EV_KEY, ecodes.KEY_BACKSPACE, 1)
        ui.syn()
        time.sleep(0.004)
        ui.write(ecodes.EV_KEY, ecodes.KEY_BACKSPACE, 0)
        ui.syn()
        time.sleep(0.004)

def live_microphone_stream(recognizer):
    """
    Stream live audio from PipeWire 'atvvoice-chromecast-remote' node
    synchronized with D-Bus Google Assistant push-to-talk button events,
    and type words live via uinput as you speak.
    """
    from gi.repository import Gio, GLib
    import threading

    try:
        ui = create_uinput_typer()
        print("Initialized virtual uinput live typing device.")
    except Exception as e:
        print(f"Warning: Could not initialize uinput device ({e}). Will only log to console.")
        ui = None

    print("\n" + "="*60)
    print("LIVE STREAMING + UINPUT TYPING READY")
    print("Hold the Google Assistant button on your remote and speak.")
    print("Words will type onto your screen in real time as you speak!")
    print("Press Ctrl+C to exit.")
    print("="*60 + "\n")

    state = {
        "proc": None,
        "stream": None,
        "thread": None,
        "is_recording": False,
        "last_text": "",
        "typed_text": "",
        "dc_mean": 0.0
    }
    lock = threading.Lock()

    def audio_worker(proc, stream):
        chunk_bytes = int(16000 * 2 * 0.1) # 100ms chunks
        dc_mean = 0.0
        last_text = ""
        while state["is_recording"] and proc.poll() is None:
            raw = proc.stdout.read(chunk_bytes)
            if not raw:
                break
            samples = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
            
            # Real-time DC filter
            dc_mean = 0.95 * dc_mean + 0.05 * np.mean(samples)
            samples = samples - dc_mean

            stream.accept_waveform(16000, samples)
            while recognizer.is_ready(stream):
                recognizer.decode_stream(stream)

            raw_text = recognizer.get_result(stream).strip()
            # Lowercase for natural typing
            current_text = raw_text.lower()

            if current_text != last_text:
                if current_text.startswith(last_text):
                    delta = current_text[len(last_text):]
                    sys.stdout.write(delta)
                    sys.stdout.flush()
                    if ui:
                        type_str(ui, delta)
                else:
                    # Model revised previous token: backspace and retype
                    if ui and len(last_text) > 0:
                        press_backspace(ui, len(last_text))
                        type_str(ui, current_text)
                    sys.stdout.write(f"\n[revised] {current_text}")
                    sys.stdout.flush()

                last_text = current_text

        # Finalize
        stream.input_finished()
        while recognizer.is_ready(stream):
            recognizer.decode_stream(stream)
        final_text = recognizer.get_result(stream).strip().lower()

        # Handle any final delta
        if final_text != last_text:
            if final_text.startswith(last_text):
                delta = final_text[len(last_text):]
                if ui:
                    type_str(ui, delta)
            else:
                if ui and len(last_text) > 0:
                    press_backspace(ui, len(last_text))
                    type_str(ui, final_text)

        # Append auto-spacing at the end of utterance
        if ui and final_text:
            type_str(ui, " ")

        print(f"\n[Final Sentence]: '{final_text}'\n")

    def on_mic_state_changed(conn, sender, path, iface, signal, params, user_data):
        st = params.unpack()[0]
        if st in ("streaming", "opening"):
            with lock:
                if state["is_recording"]:
                    return
                state["is_recording"] = True
                state["stream"] = recognizer.create_stream()
                cmd = [
                    "pw-record",
                    "--target", "atvvoice-chromecast-remote",
                    "--format", "s16",
                    "--rate", "16000",
                    "--channels", "1",
                    "-"
                ]
                sys.stdout.write("\n[Mic Open] > ")
                sys.stdout.flush()
                state["proc"] = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
                t = threading.Thread(target=audio_worker, args=(state["proc"], state["stream"]), daemon=True)
                t.start()
                state["thread"] = t

        elif st in ("connected", "idle", "closed"):
            with lock:
                if not state["is_recording"]:
                    return
                state["is_recording"] = False
                if state["proc"]:
                    state["proc"].terminate()
                    try:
                        state["proc"].wait(timeout=0.5)
                    except Exception:
                        state["proc"].kill()
                    state["proc"] = None

    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        # Pass sender=None so it captures ATVVoice signals regardless of sender name
        bus.signal_subscribe(
            None,
            "org.atvvoice.Daemon",
            "MicStateChanged",
            "/org/atvvoice/Daemon",
            None,
            Gio.DBusSignalFlags.NONE,
            on_mic_state_changed,
            None
        )
        loop = GLib.MainLoop()
        loop.run()
    except KeyboardInterrupt:
        print("\nExiting live test.")

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Sherpa-ONNX Streaming Zipformer Test")
    parser.add_argument("--live", action="store_true", help="Run live microphone streaming mode")
    parser.add_argument("--wav", type=str, default=None, help="Custom WAV file to test")
    parser.add_argument("--gpu", action="store_true", help="Use CUDA provider")
    args = parser.parse_args()

    recognizer = create_recognizer(use_gpu=args.gpu)

    if args.live:
        live_microphone_stream(recognizer)
    else:
        # Test built-in sample wav first
        test_wav = args.wav or str(MODEL_DIR / "test_wavs" / "0.wav")
        test_file_stream(recognizer, test_wav)

        # Also test on our saved remote recording if available
        remote_debug_wav = PROJECT_DIR / "debug_last_voice.wav"
        if os.path.exists(remote_debug_wav) and not args.wav:
            print("\n" + "="*60)
            print("Now testing on your saved Chromecast remote audio recording:")
            test_file_stream(recognizer, str(remote_debug_wav))
