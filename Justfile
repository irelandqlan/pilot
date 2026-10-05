# Pilot Justfile
# Command runner recipes for development, testing, services, packaging, and releases.

default:
    @just --list

# ── Application Execution ───────────────────────────────────────────────────

# Run Pilot GUI (just run [native|flatpak] [args])
run target="native" *args="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "{{target}}" = "flatpak" ]; then
        echo "==> Running Flatpak application io.github.irelandqlan.Pilot..."
        flatpak run io.github.irelandqlan.Pilot {{args}}
    elif [ "{{target}}" = "native" ]; then
        cargo run -- {{args}}
    else
        # Target was a cargo flag/argument (e.g. just run --help)
        cargo run -- {{target}} {{args}}
    fi

# ── Development & Testing ───────────────────────────────────────────────────

# Run all unit tests
test:
    cargo test

# Fast compiler check
check:
    cargo check

# Build optimized release binary
build:
    cargo build --release

# Clean all build artifacts, temporary caches, and packaging outputs
clean:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf build dist target/release target/debug .flatpak-builder
    find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
    echo "✨ Cleaned build directories, packaging outputs, and caches."

# ── Services & Hardware ─────────────────────────────────────────────────────

# Manage background daemons (status, start, stop, restart, logs)
service action="status":
    ./bin/pilot-ctl {{action}}

# ── Packaging & Distribution ────────────────────────────────────────────────

# Package management (native rpm|install, flatpak bundle|install)
pkg target="help" action="" *args="":
    ./scripts/pkg.sh {{target}} {{action}} {{args}}

# ── Releases & Git ──────────────────────────────────────────────────────────

# Safely pull latest commits with rebase on the current branch
sync:
    git pull --rebase origin $(git rev-parse --abbrev-ref HEAD)

# Release management (<version>, abort, sync)
release cmd="help" *args="":
    ./scripts/release.sh {{cmd}} {{args}}

# ── Backwards-Compatible Shortcuts ──────────────────────────────────────────

[private]
status: (service "status")

[private]
restart: (service "restart")

[private]
stop: (service "stop")

[private]
start: (service "start")

[private]
logs: (service "logs")

[private]
flatpak: (pkg "flatpak" "install")

[private]
flatpak-bundle: (pkg "flatpak" "bundle")

[private]
flatpak-run: (run "flatpak")

[private]
rpm: (pkg "native" "rpm")

[private]
rpm-install: (pkg "native" "install")

[private]
abort-release version="": (release "abort" version)

[private]
release-abort version="": (release "abort" version)
