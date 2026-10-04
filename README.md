<div align="center">
  <img src="data/io.github.irelandqlan.Pilot.svg" width="120" height="120" alt="Pilot Logo">

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
- **GNOME Background Apps Integration**:
  - Closing the settings window keeps Pilot active in the GNOME 44+ Quick Settings "Background Apps" drawer.
  - Clicking the **`X`** in Background Apps terminates Pilot and cleanly shuts down all background workers in one click.
- **Sleep & Wake Resilient**:
  - Automatically reconnects and re-grabs the remote whenever Bluetooth connects, disconnects, or wakes from suspend.
- **Interactive Libadwaita GUI**:
  - Visual remote simulator providing live feedback as you press physical buttons, with direct per-layer configuration.

## Installation

### Flatpak (Recommended)
Build and install Pilot into your user Flatpak environment:
```bash
just flatpak
# or build a standalone bundle in dist/
just flatpak-bundle
```
Run with:
```bash
flatpak run io.github.irelandqlan.Pilot
# or via command runner
just flatpak-run
```

### Fedora / RHEL RPM Package
Build and install Pilot as a native tracked RPM package:
```bash
just rpm-install
# or manually:
just rpm
sudo dnf install dist/pilot-*.rpm
```

## Quick Start

Launch the Settings GUI:
```bash
pilot
# or via command runner
just run
```

Inspect process status, Bluetooth pairing, and `/dev/uinput` permissions:
```bash
pilot-ctl status
# or via command runner
just status
```

For full setup instructions and hardware permissions, see the [Installation Guide](docs/INSTALL.md).
