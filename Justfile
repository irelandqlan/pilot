# Pilot Justfile
# Command runner recipes for development, testing, services, packaging, and releases.

# Show quick reference guide and usage examples
default: help

# Show quick reference guide and usage examples
help:
    @echo "Pilot - Chromecast Voice Remote PC Controller"
    @echo "=============================================================================="
    @echo "Usage: just <command> [subcommand] [args]"
    @echo ""
    @echo "Application & Packaging:"
    @echo "  just run [native|flatpak] [args]    Launch Pilot GUI (native cargo or Flatpak)"
    @echo "  just pkg native rpm                 Build Fedora RPM package in dist/"
    @echo "  just pkg native install             Install built RPM onto host with dnf"
    @echo "  just pkg flatpak install            Build & install Flatpak package locally"
    @echo "  just pkg flatpak bundle             Build standalone .flatpak bundle in dist/"
    @echo ""
    @echo "Release & Branch Management:"
    @echo "  just release <version>              Create, tag, and publish release (e.g. 0.2.1)"
    @echo "  just release abort [version]        Roll back an interrupted/failed release"
    @echo "  just release sync                   Safely merge develop into master and push"
    @echo "  just sync                           Pull latest commits with rebase on current branch"
    @echo ""
    @echo "Development & Testing:"
    @echo "  just test                           Run all cargo unit tests"
    @echo "  just check                          Fast cargo type & syntax check"
    @echo "  just build                          Build optimized release binary (target/release)"
    @echo "  just clean                          Clean build artifacts, temp caches, and dist/"
    @echo ""
    @echo "Hardware & Service Daemons:"
    @echo "  just service status                 Check BLE remote connection, uinput, and daemons"
    @echo "  just service start | stop | restart Manage background daemons"
    @echo "  just service logs                   Follow live systemd logs for Pilot daemons"
    @echo ""
    @echo "Run 'just --list' to inspect raw recipe signatures."

# -- Application Execution ---------------------------------------------------

# Launch Pilot GUI (native development or Flatpak)
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

# -- Development & Testing ---------------------------------------------------

# Run all cargo unit tests
test:
    cargo test

# Fast compiler syntax and type check
check:
    cargo check

# Build optimized native release binary
build:
    cargo build --release

# Clean build artifacts, temporary caches, and packaging outputs
clean:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf build dist target/release target/debug .flatpak-builder
    find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
    echo "Cleaned build directories, packaging outputs, and caches."

# -- Services & Hardware -----------------------------------------------------

# Manage background daemons (status, start, stop, restart, logs)
service action="status":
    ./bin/pilot-ctl {{action}}

# -- Packaging & Distribution ------------------------------------------------

# Package management (native rpm|install, flatpak bundle|install)
pkg target="help" action="" *args="":
    ./scripts/pkg.sh {{target}} {{action}} {{args}}

# -- Releases & Git ----------------------------------------------------------

# Safely pull latest commits with rebase on the current branch
sync:
    git pull --rebase origin $(git rev-parse --abbrev-ref HEAD)

# Release management (<version>, abort, sync)
release cmd="help" *args="":
    ./scripts/release.sh {{cmd}} {{args}}

# -- Backwards-Compatible Shortcuts ------------------------------------------

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
