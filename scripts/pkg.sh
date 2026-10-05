#!/usr/bin/env bash
# Pilot Packaging Utility
# Handles Native (RPM, Install, Run) and Flatpak (Bundle, Install, Run) targets.

set -euo pipefail

usage() {
    local code="${1:-0}"
    cat << 'EOF'
Usage: just pkg <native | flatpak> <subcommand>

Native targets:
  just pkg native rpm       Build Fedora RPM package (.rpm) in dist/
  just pkg native install   Install built RPM package onto the host via dnf

Flatpak targets:
  just pkg flatpak install  Build and install Flatpak package locally into user environment
  just pkg flatpak bundle   Build standalone .flatpak ostree bundle in dist/

Run targets:
  just run native           Launch native development GUI with cargo
  just run flatpak          Launch installed Flatpak application
EOF
    exit "$code"
}

# --- Flatpak Subcommands ---

cmd_flatpak_install() {
    echo "==> Building and installing Flatpak package locally..."
    flatpak-builder --disable-rofiles-fuse --force-clean --user --install build/flatpak io.github.irelandqlan.Pilot.yml
}

cmd_flatpak_bundle() {
    local version
    version=$(grep '^version =' Cargo.toml | head -n1 | cut -d'"' -f2)
    mkdir -p dist build/flatpak-repo
    echo "==> Building Flatpak ostree repository..."
    flatpak-builder --disable-rofiles-fuse --force-clean --repo=build/flatpak-repo build/flatpak io.github.irelandqlan.Pilot.yml
    echo "==> Creating standalone bundle dist/pilot-v${version}.flatpak..."
    flatpak build-bundle build/flatpak-repo "dist/pilot-v${version}.flatpak" io.github.irelandqlan.Pilot master
    echo "Flatpak bundle ready at dist/pilot-v${version}.flatpak"
}

cmd_flatpak_run() {
    echo "==> Running Flatpak application io.github.irelandqlan.Pilot..."
    flatpak run io.github.irelandqlan.Pilot "$@"
}

# --- Native Subcommands ---

cmd_native_rpm() {
    local version
    version=$(grep '^version =' Cargo.toml | head -n1 | cut -d'"' -f2)
    echo "==> Packaging Pilot v${version} RPM..."
    local build_dir="$(pwd)/build/rpm"
    rm -rf "${build_dir}"
    mkdir -p "${build_dir}/"{BUILD,RPMS,SOURCES,SPECS,SRPMS} dist
    local tarball="${build_dir}/SOURCES/pilot-${version}.tar.gz"
    tar --exclude-vcs --exclude="./target" --exclude="./venv" --exclude="./build" --exclude="./dist" --exclude="./models" \
        --transform "s,^\.,pilot-${version}," -czf "${tarball}" .
    rpmbuild --define "_topdir ${build_dir}" -ba pilot.spec
    cp "${build_dir}/RPMS/"*/*.rpm dist/
    echo ""
    echo "Pilot RPM built successfully in dist/:"
    ls -lh dist/*.rpm
}

cmd_native_install() {
    cmd_native_rpm
    local rpm_file
    rpm_file=$(ls -t dist/pilot-*.rpm 2>/dev/null | head -n1)
    if [ -z "$rpm_file" ]; then
        echo "Error: No RPM file found in dist/"
        exit 1
    fi
    echo "==> Installing ${rpm_file} via dnf..."
    sudo dnf install -y --nogpgcheck "${rpm_file}"
    echo "Pilot installed! Launch with 'pilot' or manage with 'just service status'."
}

cmd_native_run() {
    echo "==> Running native release build..."
    cargo run --release -- "$@"
}

# --- Dispatcher ---

TARGET="${1:-help}"
shift || true
ACTION="${1:-}"
if [ -n "$ACTION" ]; then
    shift || true
fi

case "$TARGET" in
    native)
        case "$ACTION" in
            rpm|"")        cmd_native_rpm ;;
            install)       cmd_native_install ;;
            run)           cmd_native_run "$@" ;;
            help|--help|-h) usage ;;
            *)
                echo "Error: Unknown native action: $ACTION"
                usage 1
                ;;
        esac
        ;;
    flatpak)
        case "$ACTION" in
            bundle)        cmd_flatpak_bundle ;;
            install|"")    cmd_flatpak_install ;;
            run)           cmd_flatpak_run "$@" ;;
            help|--help|-h) usage ;;
            *)
                echo "Error: Unknown flatpak action: $ACTION"
                usage 1
                ;;
        esac
        ;;
    # Shortcuts for convenience
    rpm)      cmd_native_rpm ;;
    bundle)   cmd_flatpak_bundle ;;
    install)  cmd_native_install ;;
    run)      cmd_flatpak_run "$@" ;;
    help|--help|-h)
        usage
        ;;
    *)
        echo "Error: Unknown packaging target: $TARGET"
        usage 1
        ;;
esac
