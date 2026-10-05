#!/usr/bin/env bash
# Pilot Packaging Utility
# Handles RPM and Flatpak local builds, bundles, and installations.

set -euo pipefail

usage() {
    local code="${1:-0}"
    echo "Usage: just pkg <command>"
    echo ""
    echo "Commands:"
    echo "  flatpak       Build and install Flatpak package locally"
    echo "  bundle        Build standalone Flatpak bundle in dist/"
    echo "  run           Run the installed Flatpak application"
    echo "  rpm           Build Fedora RPM package into dist/"
    echo "  install       Install built RPM onto the system via dnf"
    exit "$code"
}

cmd_flatpak() {
    echo "==> Building and installing Flatpak package locally..."
    flatpak-builder --disable-rofiles-fuse --force-clean --user --install build/flatpak io.github.irelandqlan.Pilot.yml
}

cmd_bundle() {
    local version
    version=$(grep '^version =' Cargo.toml | head -n1 | cut -d'"' -f2)
    mkdir -p dist build/flatpak-repo
    echo "==> Building Flatpak ostree repository..."
    flatpak-builder --disable-rofiles-fuse --force-clean --repo=build/flatpak-repo build/flatpak io.github.irelandqlan.Pilot.yml
    echo "==> Creating standalone bundle dist/pilot-v${version}.flatpak..."
    flatpak build-bundle build/flatpak-repo "dist/pilot-v${version}.flatpak" io.github.irelandqlan.Pilot master
    echo "✅ Flatpak bundle ready at dist/pilot-v${version}.flatpak"
}

cmd_run() {
    echo "==> Running Flatpak application io.github.irelandqlan.Pilot..."
    flatpak run io.github.irelandqlan.Pilot
}

cmd_rpm() {
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
    echo "✅ Pilot RPM built successfully in dist/:"
    ls -lh dist/*.rpm
}

cmd_install() {
    cmd_rpm
    local rpm_file
    rpm_file=$(ls -t dist/pilot-*.rpm 2>/dev/null | head -n1)
    if [ -z "$rpm_file" ]; then
        echo "❌ Error: No RPM file found in dist/"
        exit 1
    fi
    echo "==> Installing ${rpm_file} via dnf..."
    sudo dnf install -y --nogpgcheck "${rpm_file}"
    echo "✅ Pilot installed! Launch with 'pilot' or manage with 'just service status'."
}

ACTION="${1:-help}"
case "$ACTION" in
    flatpak)  cmd_flatpak ;;
    bundle)   cmd_bundle ;;
    run)      cmd_run ;;
    rpm)      cmd_rpm ;;
    install)  cmd_install ;;
    help|--help|-h) usage ;;
    *)
        echo "❌ Unknown packaging command: $ACTION"
        usage 1
        ;;
esac
