"""
Media controls: Volume, Play/Pause, Next/Previous, MPRIS DBus calls
"""

import subprocess
import shutil
import logging

logger = logging.getLogger(__name__)

def volume_up(step="5%"):
    """Increase PipeWire / system audio volume."""
    if shutil.which("wpctl"):
        subprocess.run(["wpctl", "set-volume", "-l", "1.5", "@DEFAULT_AUDIO_SINK@", f"{step}+"], check=False)
    elif shutil.which("pactl"):
        subprocess.run(["pactl", "set-sink-volume", "@DEFAULT_SINK@", f"+{step}"], check=False)

def volume_down(step="5%"):
    """Decrease PipeWire / system audio volume."""
    if shutil.which("wpctl"):
        subprocess.run(["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", f"{step}-"], check=False)
    elif shutil.which("pactl"):
        subprocess.run(["pactl", "set-sink-volume", "@DEFAULT_SINK@", f"-{step}"], check=False)

def toggle_mute():
    """Toggle PipeWire / system audio mute."""
    if shutil.which("wpctl"):
        subprocess.run(["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"], check=False)
    elif shutil.which("pactl"):
        subprocess.run(["pactl", "set-sink-mute", "@DEFAULT_SINK@", "toggle"], check=False)

def mpris_play_pause():
    """Send MPRIS PlayPause via playerctl or busctl."""
    if shutil.which("playerctl"):
        subprocess.run(["playerctl", "play-pause"], check=False)
    else:
        # Send via busctl to all active mpris players
        try:
            out = subprocess.check_output(["busctl", "--user", "list"], text=True)
            for line in out.splitlines():
                if "org.mpris.MediaPlayer2." in line:
                    service = line.split()[0]
                    subprocess.run([
                        "busctl", "--user", "call", service,
                        "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player",
                        "PlayPause"
                    ], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except Exception as e:
            logger.error(f"Failed to send MPRIS play/pause: {e}")

def mpris_next():
    """Send MPRIS Next song."""
    try:
        out = subprocess.check_output(["busctl", "--user", "list"], text=True)
        for line in out.splitlines():
            if "org.mpris.MediaPlayer2." in line:
                service = line.split()[0]
                subprocess.run([
                    "busctl", "--user", "call", service,
                    "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player",
                    "Next"
                ], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception as e:
        logger.error(f"Failed to send MPRIS next: {e}")

def mpris_previous():
    """Send MPRIS Previous song."""
    try:
        out = subprocess.check_output(["busctl", "--user", "list"], text=True)
        for line in out.splitlines():
            if "org.mpris.MediaPlayer2." in line:
                service = line.split()[0]
                subprocess.run([
                    "busctl", "--user", "call", service,
                    "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player",
                    "Previous"
                ], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except Exception as e:
        logger.error(f"Failed to send MPRIS previous: {e}")
