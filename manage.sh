#!/usr/bin/env bash
# ==============================================================================
# Chromecast Remote & Voice Control - Unified Manager & Installer
# ==============================================================================
set -euo pipefail

# Colors & Formatting
BOLD="\033[1m"
GREEN="\033[1;32m"
BLUE="\033[1;34m"
YELLOW="\033[1;33m"
RED="\033[1;31m"
CYAN="\033[1;36m"
DIM="\033[2m"
RESET="\033[0m"

# Project Source Directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
PYTHON_SRC="$SCRIPT_DIR"
RUST_SRC="$SCRIPT_DIR"

# Standard XDG Installation Paths
INSTALL_BIN="$HOME/.local/bin"
INSTALL_SHARE="$HOME/.local/share/chromecast-remote"
INSTALL_CONFIG="$HOME/.config/chromecast-remote"
INSTALL_SYSTEMD="$HOME/.config/systemd/user"
INSTALL_APPS="$HOME/.local/share/applications"
INSTALL_ICONS="$HOME/.local/share/icons/hicolor/scalable/apps"

SERVICES=("chromecast-remote.service" "chromecast-voice.service" "atvvoice.service")

print_banner() {
    echo -e "${BLUE}${BOLD}=== Pilot - Chromecast PC Remote Management ===${RESET}"
}

resolve_unit_name() {
    case "${1:-all}" in
        remote|chromecast-remote) echo "chromecast-remote.service" ;;
        voice|chromecast-voice)   echo "chromecast-voice.service" ;;
        atv|atvvoice)             echo "atvvoice.service" ;;
        all)                      echo "all" ;;
        *)                        echo "$1" ;;
    esac
}

cmd_install() {
    print_banner
    echo -e "${CYAN}Installing Pilot system...${RESET}\n"

    mkdir -p "$INSTALL_BIN" "$INSTALL_SHARE" "$INSTALL_CONFIG" "$INSTALL_SYSTEMD" "$INSTALL_APPS" "$INSTALL_ICONS"

    # 1. Build Rust Settings GUI
    echo -e "${BOLD}[1/6] Compiling Pilot Settings Application (Rust)...${RESET}"
    if [ -d "$RUST_SRC" ]; then
        (cd "$RUST_SRC" && cargo build --release)
        # Avoid ETXTBSY if binary is running by removing destination first
        rm -f "$INSTALL_BIN/pilot" "$INSTALL_BIN/chromecast-settings"
        if [ -f "$RUST_SRC/target/release/pilot" ]; then
            install -m 755 "$RUST_SRC/target/release/pilot" "$INSTALL_BIN/pilot"
        elif [ -f "$RUST_SRC/target/release/chromecast-settings" ]; then
            install -m 755 "$RUST_SRC/target/release/chromecast-settings" "$INSTALL_BIN/pilot"
        fi
        ln -sfn "$INSTALL_BIN/pilot" "$INSTALL_BIN/chromecast-settings"
        echo -e "  ${GREEN}✓ Installed ${INSTALL_BIN}/pilot (and legacy link chromecast-settings)${RESET}"
    else
        echo -e "  ${YELLOW}! Rust source directory not found, skipping GUI compilation${RESET}"
    fi

    # 2. Setup Python Backend in ~/.local/share/chromecast-remote
    echo -e "\n${BOLD}[2/6] Installing Backend Daemons...${RESET}"
    mkdir -p "$INSTALL_SHARE/bin"
    cp -r "$PYTHON_SRC/actions" "$INSTALL_SHARE/"
    cp "$PYTHON_SRC/remote_daemon.py" "$INSTALL_SHARE/"
    cp "$PYTHON_SRC/voice_daemon.py" "$INSTALL_SHARE/"
    if [ -f "$PYTHON_SRC/bin/atvvoice" ]; then
        rm -f "$INSTALL_SHARE/bin/atvvoice"
        install -m 755 "$PYTHON_SRC/bin/atvvoice" "$INSTALL_SHARE/bin/atvvoice"
    fi

    # Virtual Environment link / creation
    if [ -d "$PYTHON_SRC/venv" ]; then
        ln -sfn "$PYTHON_SRC/venv" "$INSTALL_SHARE/venv"
        echo -e "  ${GREEN}✓ Linked Python virtual environment (with faster-whisper/CUDA & evdev)${RESET}"
    fi

    # 3. Default Configuration
    echo -e "\n${BOLD}[3/6] Configuring Default Settings...${RESET}"
    if [ ! -f "$INSTALL_CONFIG/config.toml" ]; then
        if [ -f "$PYTHON_SRC/config.toml" ]; then
            cp "$PYTHON_SRC/config.toml" "$INSTALL_CONFIG/config.toml"
        else
            touch "$INSTALL_CONFIG/config.toml"
        fi
        echo -e "  ${GREEN}✓ Initialized config at ${INSTALL_CONFIG}/config.toml${RESET}"
    else
        echo -e "  ${DIM}ℹ Existing config preserved at ${INSTALL_CONFIG}/config.toml${RESET}"
    fi

    # 4. Desktop Entry & Icon
    echo -e "\n${BOLD}[4/6] Installing Desktop Launcher & Icon...${RESET}"
    if [ -f "$RUST_SRC/data/io.github.magnotec.Pilot.svg" ]; then
        cp "$RUST_SRC/data/io.github.magnotec.Pilot.svg" "$INSTALL_ICONS/"
        ln -sfn "$INSTALL_ICONS/io.github.magnotec.Pilot.svg" "$INSTALL_ICONS/com.chromecast.Settings.svg"
        echo -e "  ${GREEN}✓ Installed icon to ${INSTALL_ICONS}/io.github.magnotec.Pilot.svg${RESET}"
    elif [ -f "$RUST_SRC/data/com.chromecast.Settings.svg" ]; then
        cp "$RUST_SRC/data/com.chromecast.Settings.svg" "$INSTALL_ICONS/io.github.magnotec.Pilot.svg"
        echo -e "  ${GREEN}✓ Installed icon to ${INSTALL_ICONS}/io.github.magnotec.Pilot.svg${RESET}"
    fi
    if [ -d "$RUST_SRC/data/icons" ]; then
        mkdir -p "$INSTALL_SHARE/icons" "$HOME/.local/share/icons/hicolor/scalable/actions"
        cp -r "$RUST_SRC/data/icons/." "$INSTALL_SHARE/icons/"
        cp -r "$RUST_SRC/data/icons/." "$HOME/.local/share/icons/hicolor/scalable/actions/" 2>/dev/null || true
        echo -e "  ${GREEN}✓ Installed symbolic action icons${RESET}"
    fi
    if [ -f "$RUST_SRC/data/io.github.magnotec.Pilot.desktop" ]; then
        cp "$RUST_SRC/data/io.github.magnotec.Pilot.desktop" "$INSTALL_APPS/"
        command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$INSTALL_APPS" 2>/dev/null || true
        echo -e "  ${GREEN}✓ Registered GNOME desktop entry (io.github.magnotec.Pilot.desktop)${RESET}"
    fi

    # 5. Systemd User Services
    echo -e "\n${BOLD}[5/6] Generating Systemd User Services...${RESET}"
    
    # chromecast-remote.service
    cat <<EOF > "$INSTALL_SYSTEMD/chromecast-remote.service"
[Unit]
Description=Chromecast Remote PC Controller Daemon
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=$INSTALL_SHARE/venv/bin/python3 $INSTALL_SHARE/remote_daemon.py
Restart=always
RestartSec=3
Environment=PYTHONUNBUFFERED=1
Environment=CHROMECAST_REMOTE_CONFIG=$INSTALL_CONFIG/config.toml

[Install]
WantedBy=graphical-session.target
EOF

    # chromecast-voice.service
    cat <<EOF > "$INSTALL_SYSTEMD/chromecast-voice.service"
[Unit]
Description=Chromecast Voice Typing & Dictation Daemon
After=graphical-session.target pipewire.service atvvoice.service
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=$INSTALL_SHARE/venv/bin/python3 $INSTALL_SHARE/voice_daemon.py
Restart=always
RestartSec=3
Environment=PYTHONUNBUFFERED=1
Environment=CHROMECAST_REMOTE_CONFIG=$INSTALL_CONFIG/config.toml

[Install]
WantedBy=graphical-session.target
EOF

    # atvvoice.service
    cat <<EOF > "$INSTALL_SYSTEMD/atvvoice.service"
[Unit]
Description=ATVVoice - Chromecast Remote BLE Microphone Daemon
After=pipewire.service wireplumber.service bluetooth.target
Wants=pipewire.service

[Service]
Type=simple
ExecStart=$INSTALL_SHARE/bin/atvvoice -d 6C:B2:FD:DA:8B:4C --frame-timeout 0 -g 10
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload
    echo -e "  ${GREEN}✓ Systemd units installed and daemon reloaded${RESET}"

    # 6. Global CLI Wrapper (pilot-ctl)
    echo -e "\n${BOLD}[6/6] Installing CLI management utility (pilot-ctl)...${RESET}"
    ln -sfn "$SCRIPT_DIR/manage.sh" "$INSTALL_BIN/pilot-ctl"
    ln -sfn "$INSTALL_BIN/pilot-ctl" "$INSTALL_BIN/chromecast-ctl"
    echo -e "  ${GREEN}✓ Linked ${INSTALL_BIN}/pilot-ctl (and legacy link chromecast-ctl)${RESET}"

    echo -e "\n${GREEN}${BOLD}Installation Complete!${RESET}"
    echo -e "Start services now with: ${CYAN}pilot-ctl start${RESET} or ${CYAN}./manage.sh start${RESET}"
    echo -e "Open Settings app with:   ${CYAN}pilot${RESET}"
}

cmd_uninstall() {
    print_banner
    echo -e "${RED}Uninstalling Pilot...${RESET}\n"

    echo "Stopping and disabling systemd services..."
    systemctl --user stop "${SERVICES[@]}" 2>/dev/null || true
    systemctl --user disable "${SERVICES[@]}" 2>/dev/null || true

    echo "Removing systemd units..."
    for s in "${SERVICES[@]}"; do
        rm -f "$INSTALL_SYSTEMD/$s"
    done
    systemctl --user daemon-reload

    echo "Removing binaries and desktop entries..."
    rm -f "$INSTALL_BIN/pilot" "$INSTALL_BIN/chromecast-settings"
    rm -f "$INSTALL_BIN/pilot-ctl" "$INSTALL_BIN/chromecast-ctl"
    rm -f "$INSTALL_APPS/io.github.magnotec.Pilot.desktop" "$INSTALL_APPS/com.chromecast.Settings.desktop"
    rm -f "$INSTALL_ICONS/io.github.magnotec.Pilot.svg" "$INSTALL_ICONS/com.chromecast.Settings.svg"
    rm -rf "$INSTALL_SHARE"

    if [ -d "$INSTALL_CONFIG" ]; then
        read -r -p "Delete user configuration directory (${INSTALL_CONFIG})? [y/N] " confirm
        if [[ "$confirm" =~ ^[Yy]$ ]]; then
            rm -rf "$INSTALL_CONFIG"
            echo -e "${YELLOW}User configuration deleted.${RESET}"
        else
            echo -e "${DIM}Configuration preserved at ${INSTALL_CONFIG}.${RESET}"
        fi
    fi

    echo -e "\n${GREEN}Uninstallation complete.${RESET}"
}

cmd_status() {
    print_banner
    echo -e "${BOLD}Service Status:${RESET}"
    for s in "${SERVICES[@]}"; do
        if systemctl --user is-active --quiet "$s"; then
            pid=$(systemctl --user show --property MainPID --value "$s" 2>/dev/null || echo "?")
            echo -e "  ${GREEN}●${RESET} ${BOLD}$s${RESET}: ${GREEN}Active${RESET} (PID: $pid)"
        else
            echo -e "  ${RED}○${RESET} ${BOLD}$s${RESET}: ${RED}Inactive${RESET}"
        fi
    done

    echo -e "\n${BOLD}Hardware & Bluetooth:${RESET}"
    if command -v bluetoothctl >/dev/null 2>&1; then
        if bluetoothctl info 6C:B2:FD:DA:8B:4C 2>/dev/null | grep -q "Connected: yes"; then
            echo -e "  ${GREEN}●${RESET} Chromecast Remote (6C:B2:FD:DA:8B:4C): ${GREEN}Connected${RESET}"
        else
            echo -e "  ${YELLOW}○${RESET} Chromecast Remote (6C:B2:FD:DA:8B:4C): ${YELLOW}Disconnected / Sleeping${RESET}"
        fi
    fi

    echo -e "\n${BOLD}Permissions & Environment:${RESET}"
    if [ -w /dev/uinput ]; then
        echo -e "  ${GREEN}✓${RESET} /dev/uinput: Read/Write Access Available"
    else
        echo -e "  ${RED}✗${RESET} /dev/uinput: Permission Denied (Ensure user is in 'input' group)"
    fi

    cfg_file="$INSTALL_CONFIG/config.toml"
    if [ -f "$cfg_file" ]; then
        echo -e "  ${GREEN}✓${RESET} Configuration: $cfg_file"
    else
        echo -e "  ${YELLOW}!${RESET} Configuration: $PYTHON_SRC/config.toml (dev fallback)"
    fi
}

cmd_start() {
    target="$(resolve_unit_name "${1:-all}")"
    if [ "$target" = "all" ]; then
        echo -e "${CYAN}Starting all Chromecast services...${RESET}"
        systemctl --user start "${SERVICES[@]}"
    else
        echo -e "${CYAN}Starting $target...${RESET}"
        systemctl --user start "$target"
    fi
    cmd_status
}

cmd_stop() {
    target="$(resolve_unit_name "${1:-all}")"
    if [ "$target" = "all" ]; then
        echo -e "${YELLOW}Stopping all Chromecast services...${RESET}"
        systemctl --user stop "${SERVICES[@]}"
    else
        echo -e "${YELLOW}Stopping $target...${RESET}"
        systemctl --user stop "$target"
    fi
    cmd_status
}

cmd_restart() {
    target="$(resolve_unit_name "${1:-all}")"
    if [ "$target" = "all" ]; then
        echo -e "${CYAN}Restarting all Chromecast services...${RESET}"
        systemctl --user restart "${SERVICES[@]}"
    else
        echo -e "${CYAN}Restarting $target...${RESET}"
        systemctl --user restart "$target"
    fi
    cmd_status
}

cmd_logs() {
    target="$(resolve_unit_name "${1:-all}")"
    if [ "$target" = "all" ]; then
        echo -e "${CYAN}Streaming logs for all services (Ctrl+C to stop)...${RESET}"
        journalctl --user -f -u chromecast-remote.service -u chromecast-voice.service -u atvvoice.service
    else
        echo -e "${CYAN}Streaming logs for $target (Ctrl+C to stop)...${RESET}"
        journalctl --user -f -u "$target"
    fi
}

cmd_doctor() {
    print_banner
    echo -e "${BOLD}System Diagnostics:${RESET}\n"
    
    # 1. /dev/uinput
    echo -n "Checking /dev/uinput: "
    if [ -w /dev/uinput ]; then
        echo -e "${GREEN}OK${RESET}"
    else
        echo -e "${RED}FAILED${RESET} (Cannot write to /dev/uinput; add user to 'input' group or check udev rules)"
    fi

    # 2. PipeWire
    echo -n "Checking PipeWire session: "
    if systemctl --user is-active --quiet pipewire.service; then
        echo -e "${GREEN}OK (Active)${RESET}"
    else
        echo -e "${YELLOW}WARNING (pipewire not running)${RESET}"
    fi

    # 3. D-Bus Voice Dictation service
    echo -n "Checking D-Bus Voice Service: "
    if busctl --user list 2>/dev/null | grep -q "org.local.Dictation"; then
        echo -e "${GREEN}OK (org.local.Dictation exposed)${RESET}"
    else
        echo -e "${YELLOW}INACTIVE (chromecast-voice service is not running)${RESET}"
    fi

    # 4. NVIDIA CUDA for Whisper
    echo -n "Checking NVIDIA GPU / CUDA for Whisper: "
    if command -v nvidia-smi >/dev/null 2>&1; then
        gpu_name=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -n1 || echo "NVIDIA GPU")
        echo -e "${GREEN}OK ($gpu_name)${RESET}"
    else
        echo -e "${YELLOW}CPU Mode (nvidia-smi not detected)${RESET}"
    fi
}

case "${1:-status}" in
    install)
        cmd_install
        ;;
    uninstall)
        cmd_uninstall
        ;;
    status)
        cmd_status
        ;;
    start)
        shift
        cmd_start "${1:-all}"
        ;;
    stop)
        shift
        cmd_stop "${1:-all}"
        ;;
    restart)
        shift
        cmd_restart "${1:-all}"
        ;;
    logs)
        shift
        cmd_logs "${1:-all}"
        ;;
    doctor|check)
        cmd_doctor
        ;;
    help|--help|-h)
        print_banner
        echo "Usage: ./manage.sh [command]"
        echo ""
        echo "Commands:"
        echo "  install             Compile GUI, configure backend, desktop entry & systemd units"
        echo "  uninstall           Cleanly stop and remove all services and files"
        echo "  status              Show live status of all services and hardware connection"
        echo "  start [unit]        Start all services or specific unit (remote|voice|atv)"
        echo "  stop [unit]         Stop all services or specific unit (remote|voice|atv)"
        echo "  restart [unit]      Restart all services or specific unit (remote|voice|atv)"
        echo "  logs [unit]         Stream live journal logs (remote|voice|atv|all)"
        echo "  doctor / check      Run system diagnostic checks"
        echo "  help                Show this message"
        ;;
    *)
        echo -e "${RED}Unknown command: $1${RESET}"
        echo "Run './manage.sh help' for usage instructions."
        exit 1
        ;;
esac
