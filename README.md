<div align="center">
  <img src="data/io.github.magnotec.Pilot.svg" width="120" height="120" alt="Pilot Logo">

  # Pilot

  A GTK4 and Libadwaita hardware controller and voice dictation suite for the Linux desktop.

  <p>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-2021-ea6344?style=flat&logo=rust&logoColor=white" alt="Rust"></a>
    <a href="https://gtk.org/"><img src="https://img.shields.io/badge/GTK-4-3584e4?style=flat&logo=gnome&logoColor=white" alt="GTK4"></a>
    <a href="https://gnome.pages.gitlab.gnome.org/libadwaita/"><img src="https://img.shields.io/badge/Libadwaita-1.x-3584e4?style=flat" alt="Libadwaita"></a>
    <a href="https://www.python.org/"><img src="https://img.shields.io/badge/Python-3.11+-3776ab?style=flat&logo=python&logoColor=white" alt="Python"></a>
  </p>

  <p>
    <a href="docs/INSTALL.md">Install</a> &bull;
    <a href="docs/DEVELOPMENT.md">Development</a> &bull;
    <a href="docs/SERVICES.md">Services Architecture</a> &bull;
    <a href="https://github.com/irelandqlan/pilot/issues">Issues</a>
  </p>
</div>

<br>

> [!NOTE]
> Designed specifically for the **Chromecast with Google TV Voice Remote** (over Bluetooth) on GNOME / Linux.

---

## About

**Pilot** turns your Chromecast with Google TV Bluetooth remote into a full PC media controller, smooth virtual mouse, and push-to-talk voice typing device.

It pairs low-latency background input and speech daemons with a native Libadwaita settings application for button remapping, mouse acceleration tuning, and live visual testing.

## Features

- **Customizable Multi-Layer Profiles**:
  - Fully customizable button mapping layers (shipped with Layer 1: Media and Layer 2: Mouse as the default configuration).
  - Add, duplicate, and delete custom layers directly in the Settings app or config file.
  - Cycle through layers or jump directly to specific profiles using any mapped button.
  - Support for tap, hold (long-press), and double-tap actions across all buttons for keys, key combinations, mouse clicks, shell commands, or system actions.
- **Virtual Mouse & Smooth Acceleration**:
  - Emulates an accelerated virtual mouse pointer with configurable base speed, maximum speed, exponential curves, and scroll steps.
- **Whisper Voice Dictation**:
  - Hold the Assistant button to stream speech directly into local faster-whisper inference (with CUDA GPU acceleration or CPU) and auto-type into the active window.
- **Sleep & Wake Resilient**:
  - Automatically reconnects and re-grabs the remote whenever Bluetooth connects, disconnects, or wakes from suspend.
- **Interactive Libadwaita GUI**:
  - Visual remote simulator providing live feedback as you press physical buttons, with direct per-layer configuration.

## Quick Start

Install or update the system:
```bash
just install
```

Launch the Settings GUI:
```bash
just run
# or
pilot
```

Check system status (services, Bluetooth connectivity, permissions):
```bash
just status
```

For full setup instructions, see the [Installation Guide](docs/INSTALL.md).
