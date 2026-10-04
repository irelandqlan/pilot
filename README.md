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

- **Dual-Mode Control**:
  - **Media Mode (Default)**: Volume, media scrub (seek forward/backward), play/pause, fullscreen, YouTube & streaming app launcher.
  - **Mouse Mode**: D-Pad becomes a smooth virtual mouse pointer with configurable exponential acceleration, tap Center for Left Click, Back for Right Click, side volume buttons for Scroll Wheel.
- **Instant Mode Switching**: Press the **Input / TV** button to toggle modes anytime.
- **Whisper Voice Dictation**: Hold the **Assistant (Mic)** button to speak; release to instantly transcribe and type via faster-whisper (CUDA accelerated or CPU).
- **Bedtime Power Controls**:
  - **Short Tap Power Button**: Blanks / puts monitors into power-save mode so you can sleep without bright screens.
  - **Hold Power Button (> 0.7s)**: Suspends PC (`systemctl suspend`).
- **Sleep & Wake Resilient**: Continuously detects Bluetooth disconnects/reconnects and re-grabs the remote automatically.
- **Native Settings App**: Modern GNOME Libadwaita UI to configure button bindings, mouse curves, and voice dictation settings.
- **Live Event Simulation**: Interactive visual remote in the settings app that lights up buttons as you press them on the physical remote.

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
