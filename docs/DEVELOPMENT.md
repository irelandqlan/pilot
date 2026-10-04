# Developer Guide

Welcome to developing **Pilot**.

## Project Architecture

```
pilot/
├── Cargo.toml / Cargo.lock       # Rust build specification
├── Justfile                      # Command runner for development and release workflows
├── io.github.magnotec.Pilot.yml  # Flatpak application manifest
├── pilot.spec                    # Fedora / RHEL RPM package spec
├── config.toml                   # Master default configuration file
├── src/                          # Native Rust GUI & Remote Controller Engine
│   ├── main.rs                   # GTK4 / Libadwaita interface & app lifecycle
│   ├── remote.rs                 # Native evdev grabber, mouse physics & uinput engine
│   └── supervisor.rs             # Process supervisor for child workers
├── data/                         # GResource bundle, icons, udev rule, & .desktop launcher
│   ├── resources.gresource.xml   # GResource manifest for bundled SVGs
│   ├── 70-pilot-uinput.rules     # Udev permission rule for /dev/uinput
│   ├── io.github.magnotec.Pilot.svg
│   ├── io.github.magnotec.Pilot.desktop
│   ├── io.github.magnotec.Pilot.metainfo.xml
│   └── icons/                    # Symbolic action icons
├── bin/                          # Native binaries & CLI management
│   ├── atvvoice                  # C/C++ BLE GATT microphone capture daemon
│   └── pilot-ctl                 # Command-line diagnostics and control tool
├── voice_daemon.py               # faster-whisper speech-to-text service
└── docs/                         # Extended documentation
```

---

## Development Workflows

We use [`just`](https://github.com/casey/just) as our primary task runner.

### Running Pilot Locally

Run the GUI directly with development config:
```bash
just run
```

### Running Tests

Execute the Rust unit test suite:
```bash
just test
```

### Fast Compiler Checks

```bash
just check
```

### Compiling Optimized Release

```bash
just build-release
```

---

## Packaging Workflows

### Flatpak
```bash
# Build and install locally for testing
just flatpak

# Run the installed Flatpak
just flatpak-run

# Build standalone distribution bundle (.flatpak file in dist/)
just flatpak-bundle
```

### Fedora RPM
```bash
# Build and install RPM locally
just rpm-install

# Or build RPM package in dist/
just rpm
```

---

## Managing Processes & Services

Inspect and control Pilot and background workers during development:

```bash
just status      # Inspect active processes, Bluetooth status, and uinput permissions
just restart     # Restart Pilot and background workers
just stop        # Stop Pilot and background workers
just logs        # Follow logs (systemd / journalctl)
```

---

## Git Workflow & Releases

Pilot follows the standard branching and release model:
- **`develop`**: Default branch for daily commits and iterative feature work.
- **`master`**: Production-ready code.

### Merging to Master

When changes on `develop` are verified:
```bash
just merge-to-master
```

### Creating Releases

Automate version bumping across `Cargo.toml`, `pilot.spec`, and AppStream metadata, with Git tagging and publishing:
```bash
just release 0.2.0
```
