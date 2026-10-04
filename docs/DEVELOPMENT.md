# Developer Guide

Welcome to developing **Pilot**.

## Project Architecture

```
pilot/
├── Cargo.toml / Cargo.lock       # Rust build specification
├── Justfile                      # Command runner for development and release workflows
├── src/                          # Native Libadwaita / GTK4 Settings application
│   └── main.rs
├── data/                         # GResource bundle, icons & .desktop launcher
│   ├── resources.gresource.xml   # GResource manifest for bundled SVGs
│   ├── io.github.magnotec.Pilot.svg
│   ├── io.github.magnotec.Pilot.desktop
│   ├── io.github.magnotec.Pilot.metainfo.xml
│   └── icons/                    # Symbolic action icons
├── remote_daemon.py              # Bluetooth input grabber & virtual uinput emitter
├── voice_daemon.py               # faster-whisper speech-to-text service
├── actions/                      # Modular action handlers (mouse, media, system)
├── bin/                          # BLE microphone capture daemon (atvvoice)
├── config.toml                   # Master default configuration file
├── manage.sh                     # System manager and CLI tool (symlinked as pilot-ctl)
└── docs/                         # Extended documentation
```

---

## Development Workflows

We use [`just`](https://github.com/casey/just) as our primary task runner.

### Running Pilot Locally

Run the GUI directly with live dev fallback config:
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

## Managing Background Daemons

Control the running daemons during development:

```bash
just status      # Inspect active daemons and hardware Bluetooth status
just restart     # Restart all background user services
just logs        # Follow combined journalctl output
just stop        # Stop all user services
just start       # Start all user services
```

---

## Git Workflow & Releases

Pilot follows the same branching and release model as Obelisk:
- **`develop`**: Default branch for daily commits and iterative feature work.
- **`master`**: Production-ready code.

### Merging to Master

When changes on `develop` are verified:
```bash
just merge-to-master
```

### Creating Releases

Automate version bumping, Git tagging, and release publishing:
```bash
just release 0.2.0
```
