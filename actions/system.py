import os
import time
import subprocess
import shutil
import logging
import queue
import threading

logger = logging.getLogger(__name__)

def find_mode_sound(sound_name: str) -> str | None:
    """Finds an appropriate, pleasing GNOME sound for mode switching or system actions."""
    if sound_name == "mouse":
        candidates = [
            "/usr/share/sounds/gnome/default/alerts/click.ogg",
            "/usr/share/sounds/freedesktop/stereo/device-added.oga",
            "/usr/share/sounds/freedesktop/stereo/audio-volume-change.oga",
        ]
    elif sound_name == "media":
        candidates = [
            "/usr/share/sounds/gnome/default/alerts/swing.ogg",
            "/usr/share/sounds/gnome/default/alerts/string.ogg",
            "/usr/share/sounds/freedesktop/stereo/device-removed.oga",
        ]
    else:
        candidates = [
            f"/usr/share/sounds/freedesktop/stereo/{sound_name}.oga",
            f"/usr/share/sounds/gnome/default/alerts/{sound_name}.ogg",
        ]
    for c in candidates:
        if os.path.exists(c):
            return c
    return None

_last_sound_proc = None

def play_sound(sound_name: str = "dialog-information", volume: float = 0.28):
    """Plays a system notification sound asynchronously at an attenuated, pleasant volume."""
    global _last_sound_proc
    if _last_sound_proc and _last_sound_proc.poll() is None:
        try:
            _last_sound_proc.terminate()
        except Exception:
            pass

    sound_path = None
    if os.path.isabs(sound_name) and os.path.exists(sound_name):
        sound_path = sound_name
    else:
        sound_path = find_mode_sound(sound_name)

    if sound_path and shutil.which("pw-play"):
        try:
            _last_sound_proc = subprocess.Popen(
                ["pw-play", "--volume", str(volume), sound_path],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL
            )
            return
        except Exception:
            pass

    if shutil.which("canberra-gtk-play"):
        try:
            canberra_id = "device-added" if sound_name == "mouse" else ("device-removed" if sound_name == "media" else sound_name)
            _last_sound_proc = subprocess.Popen(["canberra-gtk-play", "-i", canberra_id], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return
        except Exception:
            pass

_notif_queue = queue.Queue()
_active_notif_data = {}  # tag -> (id, timestamp)

def _notif_worker():
    """Background worker that serializes notifications and tracks active IDs for in-place replacement while visible."""
    while True:
        try:
            item = _notif_queue.get()
            if item is None:
                break
            title, message, icon, transient, tag, timeout_ms = item
            if not shutil.which("notify-send"):
                _notif_queue.task_done()
                continue

            now = time.time()
            cmd = ["notify-send", "-p", "-a", "Chromecast Remote", "-i", icon]
            if timeout_ms:
                cmd.extend(["-t", str(timeout_ms)])
            if transient:
                cmd.extend([
                    "-e",
                    "-h", "int:transient:1",
                    "-h", f"string:x-canonical-private-synchronous:{tag}",
                ])

            # Only reuse previous ID if the previous notification was sent recently and is still visible on screen.
            # If it has already timed out/faded away, do NOT pass -r, so GNOME creates a fresh on-screen banner!
            prev_data = _active_notif_data.get(tag)
            if prev_data:
                prev_id, prev_time = prev_data
                active_window = (timeout_ms / 1000.0) + 0.3 if timeout_ms else 2.0
                if (now - prev_time) < active_window:
                    cmd.extend(["-r", str(prev_id)])

            cmd.append(title)
            if message:
                cmd.append(message)

            res = subprocess.run(cmd, capture_output=True, text=True, timeout=1.0)
            out = res.stdout.strip()
            if out.isdigit():
                _active_notif_data[tag] = (int(out), time.time())
        except Exception as e:
            logger.error(f"Failed to send notification: {e}")
        finally:
            _notif_queue.task_done()

_worker_thread = threading.Thread(target=_notif_worker, daemon=True)
_worker_thread.start()

def send_notification(
    title: str,
    message: str = "",
    icon: str = "preferences-desktop-remote-symbolic",
    transient: bool = True,
    tag: str = "mode",
    timeout_ms: int = 1500,
    replace_id: int | None = None,
):
    """
    Sends a GNOME-native desktop notification asynchronously.
    Enqueues to a serialized worker thread that queries and tracks the server-assigned
    notification ID, ensuring in-place replacement and zero duplicate popups.
    """
    _notif_queue.put((title, message, icon, transient, tag, timeout_ms))

def toggle_displays(state: str = "off"):
    """
    Turns displays off or on in GNOME Wayland via Mutter DisplayConfig.
    state: 'off' (PowerSaveMode 3) or 'on' (PowerSaveMode 0)
    """
    mode = 3 if state == "off" else 0
    try:
        subprocess.run(
            [
                "busctl", "--user", "set-property",
                "org.gnome.Mutter.DisplayConfig",
                "/org/gnome/Mutter/DisplayConfig",
                "org.gnome.Mutter.DisplayConfig",
                "PowerSaveMode", "i", str(mode)
            ],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
    except Exception as e:
        logger.error(f"Failed to toggle display power: {e}")

def lock_screen():
    """Locks the GNOME screen."""
    try:
        subprocess.run(
            ["busctl", "--user", "call", "org.gnome.ScreenSaver", "/org/gnome/ScreenSaver", "org.gnome.ScreenSaver", "Lock"],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
    except Exception as e:
        logger.error(f"Failed to lock screen: {e}")

def suspend_system():
    """Suspends the PC."""
    send_notification("Bedtime", "Suspending PC...", icon="system-shutdown-symbolic")
    play_sound("service-logout")
    try:
        subprocess.Popen(["systemctl", "suspend"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception as e:
        logger.error(f"Failed to suspend system: {e}")

def execute_command(cmd: str):
    """Runs an arbitrary shell command asynchronously."""
    try:
        subprocess.Popen(cmd, shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception as e:
        logger.error(f"Failed to execute command '{cmd}': {e}")

def is_overview_active() -> bool:
    """Checks if GNOME Activities Overview is currently active on screen."""
    try:
        res = subprocess.check_output(
            ["busctl", "--user", "get-property", "org.gnome.Shell", "/org/gnome/Shell", "org.gnome.Shell", "OverviewActive"],
            text=True,
            timeout=0.15
        ).strip()
        return "true" in res.lower()
    except Exception:
        return False


