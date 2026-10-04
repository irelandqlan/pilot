# Installation Guide

This guide walks through setting up **Pilot** on Linux (Fedora, Arch, Ubuntu/Debian).

## Prerequisites

1. **Bluetooth Support**:
   - `bluez` and `bluez-tools`
   - Pair your Chromecast with Google TV voice remote using GNOME Settings or `bluetoothctl`:
     ```bash
     bluetoothctl scan on
     # Hold Back + Home on remote until LED pulses
     bluetoothctl pair <REMOTE_MAC>
     bluetoothctl connect <REMOTE_MAC>
     bluetoothctl trust <REMOTE_MAC>
     ```

2. **User Permissions (`uinput`)**:
   - Ensure your user has access to `/dev/uinput` to emulate virtual mouse and keyboard events:
     ```bash
     sudo usermod -aG input $USER
     ```

3. **Audio & Dictation (Optional for Voice Typing)**:
   - PipeWire and WirePlumber
   - NVIDIA CUDA (for GPU-accelerated Whisper) or standard CPU execution

---

## One-Command Installation

Pilot comes with an installer script that compiles the Rust settings application, sets up the systemd user services, and registers desktop entries and icons:

```bash
# Using Just runner
just install

# Or using the installer directly
./install.sh
```

This will:
- Compile the optimized release binary `pilot` to `~/.local/bin/pilot`.
- Install desktop launcher `io.github.magnotec.Pilot.desktop` and application icon `io.github.magnotec.Pilot.svg`.
- Set up systemd user services in `~/.config/systemd/user/`.
- Provide the CLI management utility `pilot-ctl` (and legacy link `chromecast-ctl`).

---

## Verifying Status

Check system connectivity, background daemons, and permissions:

```bash
just status
# or
pilot-ctl status
```

---

## Uninstallation

To remove Pilot binaries, systemd units, and desktop shortcuts:

```bash
just uninstall
# or
./uninstall.sh
```
