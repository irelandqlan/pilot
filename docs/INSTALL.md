# Installation Guide

This guide walks through setting up **Pilot** on Linux (Fedora, Arch, Ubuntu/Debian).

## Prerequisites

### 1. Bluetooth Remote Pairing
- Ensure `bluez` is running.
- Pair your Chromecast with Google TV voice remote using GNOME Settings or `bluetoothctl`:
  ```bash
  bluetoothctl scan on
  # Hold Back + Home on the remote until the LED pulses
  bluetoothctl pair <REMOTE_MAC>
  bluetoothctl connect <REMOTE_MAC>
  bluetoothctl trust <REMOTE_MAC>
  ```

### 2. User Permissions for Virtual Input (`/dev/uinput`)
To synthesize virtual mouse movement and keystrokes without running as root, install the udev rule included with Pilot:
```bash
sudo cp data/70-pilot-uinput.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```
*(Note: Installing via the RPM package automatically installs and activates this udev rule).*

### 3. Audio & Voice Dictation
- PipeWire and WirePlumber for capturing the remote's microphone stream.
- Python 3 with `faster-whisper` (for speech-to-text inference).

---

## Installation Methods

### Option A: Flatpak Installation (Recommended)

Pilot integrates seamlessly with GNOME Quick Settings "Background Apps" when running as a Flatpak:

```bash
# Build and install directly into your user Flatpak environment
just flatpak

# Run Pilot
just flatpak-run
# or
flatpak run io.github.magnotec.Pilot
```

To generate a standalone `.flatpak` bundle for offline distribution:
```bash
just flatpak-bundle
```
The resulting bundle is written to `dist/pilot-v<version>.flatpak`.

### Option B: Fedora / RHEL RPM Package

Build and install as a native system RPM:

```bash
# Build and install in one step
just rpm-install

# Or manually:
just rpm
sudo dnf install dist/pilot-*.rpm
```

---

## Verifying Status

Inspect Pilot's running state, background workers, Bluetooth connection, and `/dev/uinput` permissions:

```bash
just status
# or
./bin/pilot-ctl status
```

---

## Uninstallation

### Flatpak
```bash
flatpak uninstall io.github.magnotec.Pilot
```

### RPM
```bash
sudo dnf remove pilot
```
