# Chromecast Remote & Voice Control

Unified Linux / GNOME desktop controller and voice dictation system for the **Chromecast with Google TV Remote** (over Bluetooth).

Includes:
- **Python Daemons**: Remote input handler (`remote_daemon.py`), Whisper voice dictation service (`voice_daemon.py`), and PipeWire BLE microphone support (`atvvoice`).
- **Rust / Libadwaita GUI**: Native desktop settings application (`chromecast-settings`) with interactive remote mapping, mouse tuning, and live event testing.
- **Unified CLI Manager**: `manage.sh` (linked as `chromecast-ctl`) for one-command installation, status checks, restarts, and log monitoring.

---

## Architecture & Layout

```
chromecast-remote/
├── Cargo.toml / Cargo.lock   # Rust settings app build definition
├── src/                      # Native Libadwaita / GTK4 Settings application
│   └── main.rs
├── data/                     # Icons & .desktop launcher
├── remote_daemon.py          # Dual-mode (Media / Mouse) input daemon
├── voice_daemon.py           # Whisper STT voice typing daemon
├── actions/                  # Modular input action handlers (mouse, media, system)
├── bin/                      # BLE microphone daemon (atvvoice)
├── config.toml               # Master configuration file (XDG ~/.config/chromecast-remote/)
├── manage.sh                 # Unified installer, service manager, and CLI utility
├── install.sh / uninstall.sh # Quick setup wrappers
└── venv/                     # Python virtual environment with dependencies
```

---

## Features

- **Dual-Mode Control**:
  - **Media Mode (Default)**: Volume, media scrub (seek forward/backward), play/pause, fullscreen, YouTube & streaming app launcher.
  - **Mouse Mode**: D-Pad becomes a smooth virtual mouse pointer with configurable exponential acceleration, tap Center for Left Click, Back for Right Click, side volume buttons for Scroll Wheel.
- **Instant Mode Switching**: Press the **Input / TV** button to toggle modes anytime.
- **Whisper Voice Dictation**: Hold the **Assistant (Mic)** button to speak; release to instantly transcribe and type via faster-whisper (CUDA accelerated).
- **Bedtime Power Controls**:
  - **Short Tap Power Button**: Blanks / puts monitors into power-save mode so you can sleep without bright screens.
  - **Hold Power Button (> 0.7s)**: Suspends PC (`systemctl suspend`).
- **Sleep & Wake Resilient**: Continuously detects Bluetooth disconnects/reconnects and re-grabs the remote automatically.
- **Native Settings App**: Modern GNOME Libadwaita UI to configure button bindings, mouse curves, and voice dictation settings.

---

## Quick Start & Management

### Using the Management CLI (`chromecast-ctl` / `manage.sh`)

Install or update the entire system:
```bash
./manage.sh install
```

Check system status (services, Bluetooth connectivity, uinput permissions):
```bash
./manage.sh status
```

Service controls:
```bash
./manage.sh restart    # Restart all services
./manage.sh stop       # Stop all services
./manage.sh start      # Start all services
./manage.sh logs       # Stream combined journalctl logs
```

Launch the Settings GUI:
```bash
chromecast-settings
```
Or run directly from source:
```bash
cargo run --release
```

---

## Configuration

Settings are saved in `~/.config/chromecast-remote/config.toml` (with a local `config.toml` dev fallback).

Key configurations:
- **`[device]`**: Remote name pattern, exclusive grab, poll interval.
- **`[mouse]`**: Base speed, max speed, acceleration curve, poll rate, scroll steps.
- **`[voice]`**: Whisper model size (`tiny`, `base`, `small`, `medium.en`), compute type (`float16`, `int8`), auto-spacing.
- **`[mode.media]` / `[mode.mouse]`**: Per-button bindings supporting single-key, multi-key combinations, shell commands, or built-in actions.
