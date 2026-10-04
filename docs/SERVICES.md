# Services & Architecture

Pilot features an integrated, unified architecture. Rather than relying on fragile standalone user systemd units, Pilot's main application runs an internal process supervisor (`WorkerSupervisor`) that manages worker threads and helper child processes directly.

---

## 1. Native Remote Controller Engine (Rust In-Process Thread)

- **Source**: `src/remote.rs`
- **Execution**: Runs in a dedicated background worker thread within the `pilot` process.
- **Functionality**:
  - **Evdev Device Grabbing**: Continuously monitors `/dev/input/event*` for Chromecast Voice Remotes and exclusively grabs (`EVIOCGRAB`) the device so input does not bleed through to other desktop components.
  - **Accelerated Mouse Physics**: Dedicated `MouseEngine` running a high-frequency polling loop for accelerated pointer coordinates, clicks, scrolling, and drag-lock with zero Python GIL/GC latency.
  - **Multi-Action State Machine**: Handles single taps, hold (long-press), and double-tap gestures per button across user-defined layers.
  - **Virtual Input**: Emulates key presses and relative mouse motion via Linux `/dev/uinput`.
  - **Auto-Reconnection**: Resilient to Bluetooth sleep, disconnection, and resume from suspend.

---

## 2. ATVVoice BLE Audio Daemon

- **Source / Binary**: `bin/atvvoice` (C/C++ PipeWire/BlueZ binary)
- **Execution**: Spawned and supervised directly as a child process by Pilot.
- **Functionality**:
  - Automatically queries the paired remote's Bluetooth MAC address (`discover_chromecast_mac()`).
  - Connects to the Chromecast Voice Remote's BLE GATT voice profile.
  - Decodes real-time audio packets and streams them into a virtual PipeWire microphone node for dictation.

---

## 3. Voice Dictation Daemon

- **Script**: `voice_daemon.py`
- **Execution**: Spawned and supervised directly as a child process by Pilot.
- **Functionality**:
  - Monitors remote microphone button triggers.
  - Captures the audio buffer recorded by PipeWire.
  - Runs local AI speech-to-text inference using `faster-whisper` (GPU accelerated via CUDA when available, with automatic CPU fallback).
  - Types or pastes transcribed text directly into the focused application window.

---

## GNOME Shell "Background Apps" Integration

When running as a Flatpak (`io.github.irelandqlan.Pilot`), Pilot registers with the `xdg-desktop-portal` background monitor (`org.freedesktop.portal.Background`).

- Closing the Settings window hides the GUI, keeping the remote engine and audio workers active in the background.
- Pilot appears in GNOME Quick Settings under **Background Apps**.
- Clicking the **`X`** button next to Pilot in GNOME Quick Settings cleanly terminates the app and all supervised workers in a single click.
