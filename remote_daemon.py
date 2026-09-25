#!/usr/bin/env python3
"""
Chromecast Remote Daemon for Linux / GNOME
Allows using a Google Chromecast with Google TV remote to control PC from bed.
"""

import os
import sys
import time
import signal
import tomllib
import logging
import threading
from pathlib import Path
import evdev
from evdev import ecodes, UInput

from actions.mouse import MouseController
from actions import system, media

# Set up logging
logging.basicConfig(
    level=logging.INFO,
    format="[%(asctime)s] [%(levelname)s] %(message)s",
    datefmt="%H:%M:%S"
)
logger = logging.getLogger("ChromecastRemote")

def get_config_path() -> Path:
    if "CHROMECAST_REMOTE_CONFIG" in os.environ:
        return Path(os.environ["CHROMECAST_REMOTE_CONFIG"])
    user_cfg = Path.home() / ".config" / "chromecast-remote" / "config.toml"
    if user_cfg.exists():
        return user_cfg
    dev_cfg = Path(__file__).parent / "config.toml"
    if dev_cfg.exists():
        return dev_cfg
    return user_cfg

CONFIG_FILE = get_config_path()

def load_config():
    """Load configuration from config.toml."""
    cfg_path = get_config_path()
    if not cfg_path.exists():
        logger.error(f"Config file not found: {cfg_path}")
        sys.exit(1)
    with open(cfg_path, "rb") as f:
        return tomllib.load(f)

CHAR_MAP = {
    'a': (ecodes.KEY_A, False), 'b': (ecodes.KEY_B, False), 'c': (ecodes.KEY_C, False),
    'd': (ecodes.KEY_D, False), 'e': (ecodes.KEY_E, False), 'f': (ecodes.KEY_F, False),
    'g': (ecodes.KEY_G, False), 'h': (ecodes.KEY_H, False), 'i': (ecodes.KEY_I, False),
    'j': (ecodes.KEY_J, False), 'k': (ecodes.KEY_K, False), 'l': (ecodes.KEY_L, False),
    'm': (ecodes.KEY_M, False), 'n': (ecodes.KEY_N, False), 'o': (ecodes.KEY_O, False),
    'p': (ecodes.KEY_P, False), 'q': (ecodes.KEY_Q, False), 'r': (ecodes.KEY_R, False),
    's': (ecodes.KEY_S, False), 't': (ecodes.KEY_T, False), 'u': (ecodes.KEY_U, False),
    'v': (ecodes.KEY_V, False), 'w': (ecodes.KEY_W, False), 'x': (ecodes.KEY_X, False),
    'y': (ecodes.KEY_Y, False), 'z': (ecodes.KEY_Z, False),
    'A': (ecodes.KEY_A, True),  'B': (ecodes.KEY_B, True),  'C': (ecodes.KEY_C, True),
    'D': (ecodes.KEY_D, True),  'E': (ecodes.KEY_E, True),  'F': (ecodes.KEY_F, True),
    'G': (ecodes.KEY_G, True),  'H': (ecodes.KEY_H, True),  'I': (ecodes.KEY_I, True),
    'J': (ecodes.KEY_J, True),  'K': (ecodes.KEY_K, True),  'L': (ecodes.KEY_L, True),
    'M': (ecodes.KEY_M, True),  'N': (ecodes.KEY_N, True),  'O': (ecodes.KEY_O, True),
    'P': (ecodes.KEY_P, True),  'Q': (ecodes.KEY_Q, True),  'R': (ecodes.KEY_R, True),
    'S': (ecodes.KEY_S, True),  'T': (ecodes.KEY_T, True),  'U': (ecodes.KEY_U, True),
    'V': (ecodes.KEY_V, True),  'W': (ecodes.KEY_W, True),  'X': (ecodes.KEY_X, True),
    'Y': (ecodes.KEY_Y, True),  'Z': (ecodes.KEY_Z, True),
    '0': (ecodes.KEY_0, False), '1': (ecodes.KEY_1, False), '2': (ecodes.KEY_2, False),
    '3': (ecodes.KEY_3, False), '4': (ecodes.KEY_4, False), '5': (ecodes.KEY_5, False),
    '6': (ecodes.KEY_6, False), '7': (ecodes.KEY_7, False), '8': (ecodes.KEY_8, False),
    '9': (ecodes.KEY_9, False), ' ': (ecodes.KEY_SPACE, False),
    '!': (ecodes.KEY_1, True),  '@': (ecodes.KEY_2, True),  '#': (ecodes.KEY_3, True),
    '$': (ecodes.KEY_4, True),  '%': (ecodes.KEY_5, True),  '^': (ecodes.KEY_6, True),
    '&': (ecodes.KEY_7, True),  '*': (ecodes.KEY_8, True),  '(': (ecodes.KEY_9, True),
    ')': (ecodes.KEY_0, True),  '-': (ecodes.KEY_MINUS, False), '_': (ecodes.KEY_MINUS, True),
    '=': (ecodes.KEY_EQUAL, False), '+': (ecodes.KEY_EQUAL, True),
    '[': (ecodes.KEY_LEFTBRACE, False), '{': (ecodes.KEY_LEFTBRACE, True),
    ']': (ecodes.KEY_RIGHTBRACE, False), '}': (ecodes.KEY_RIGHTBRACE, True),
    ';': (ecodes.KEY_SEMICOLON, False), ':': (ecodes.KEY_SEMICOLON, True),
    "'": (ecodes.KEY_APOSTROPHE, False), '"': (ecodes.KEY_APOSTROPHE, True),
    ',': (ecodes.KEY_COMMA, False), '<': (ecodes.KEY_COMMA, True),
    '.': (ecodes.KEY_DOT, False), '>': (ecodes.KEY_DOT, True),
    '/': (ecodes.KEY_SLASH, False), '?': (ecodes.KEY_SLASH, True),
    '`': (ecodes.KEY_GRAVE, False), '~': (ecodes.KEY_GRAVE, True),
    '\\': (ecodes.KEY_BACKSLASH, False), '|': (ecodes.KEY_BACKSLASH, True),
    '\n': (ecodes.KEY_ENTER, False), '\t': (ecodes.KEY_TAB, False),
}

def create_virtual_uinput():
    """Create a virtual input device with startup retry loop for service resilience."""
    typing_keys = [mapping[0] for mapping in CHAR_MAP.values()]
    keys = [
        ecodes.KEY_UP, ecodes.KEY_DOWN, ecodes.KEY_LEFT, ecodes.KEY_RIGHT,
        ecodes.KEY_SPACE, ecodes.KEY_ENTER, ecodes.KEY_ESC, ecodes.KEY_BACKSPACE,
        ecodes.KEY_LEFTMETA, ecodes.KEY_LEFTALT, ecodes.KEY_LEFTCTRL, ecodes.KEY_LEFTSHIFT,
        ecodes.KEY_TAB, ecodes.KEY_V, ecodes.KEY_F, ecodes.KEY_F11, ecodes.KEY_F12, ecodes.KEY_F4,
        ecodes.KEY_PLAYPAUSE, ecodes.KEY_NEXTSONG, ecodes.KEY_PREVIOUSSONG,
        ecodes.KEY_VOLUMEUP, ecodes.KEY_VOLUMEDOWN, ecodes.KEY_MUTE,
        ecodes.BTN_LEFT, ecodes.BTN_RIGHT, ecodes.BTN_MIDDLE
    ] + typing_keys

    capabilities = {
        ecodes.EV_KEY: list(set(keys)),
        ecodes.EV_REL: [ecodes.REL_X, ecodes.REL_Y, ecodes.REL_WHEEL]
    }
    for attempt in range(15):
        try:
            ui = UInput(capabilities, name="Chromecast Virtual Controller")
            logger.info("Created virtual UInput keyboard/mouse device.")
            return ui
        except Exception as e:
            logger.warning(f"UInput device creation attempt {attempt+1} failed: {e}. Retrying in 1s...")
            time.sleep(1.0)
    raise RuntimeError("Failed to initialize UInput device after multiple retries.")

class RemoteDaemon:
    def __init__(self):
        self.config = load_config()
        self.uinput = create_virtual_uinput()
        mouse_cfg = self.config.get("mouse", {})
        self.mouse = MouseController(
            self.uinput,
            base_speed=mouse_cfg.get("base_speed", 5.0),
            max_speed=mouse_cfg.get("max_speed", 38.0),
            accel_rate=mouse_cfg.get("acceleration", 1.14),
            poll_rate_ms=mouse_cfg.get("poll_rate_ms", 16)
        )
        self.current_mode = self.config.get("general", {}).get("initial_mode", "media")
        self.long_press_threshold = self.config.get("general", {}).get("long_press_threshold_sec", 0.7)
        self.scroll_step = mouse_cfg.get("scroll_step", 2)

        self.pending_presses = {} # keycode: {'time': float, 'timer': Timer, 'fired_long': bool}
        self.recent_taps = {} # keycode: {'timer': Timer}
        self.is_dragging = False
        self.is_running = True

        # Register termination signal handlers for clean systemd shutdowns
        signal.signal(signal.SIGINT, self._handle_signal)
        signal.signal(signal.SIGTERM, self._handle_signal)

    def _handle_signal(self, signum, frame):
        logger.info(f"Received signal {signum}. Shutting down cleanly...")
        self.is_running = False
        sys.exit(0)

    def toggle_mode(self):
        """Toggle between Media mode and Mouse mode."""
        self.current_mode = "mouse" if self.current_mode == "media" else "media"
        logger.info(f"Mode switched to: {self.current_mode.upper()}")
        
        # Stop mouse movement loop if switching out of mouse mode
        if self.current_mode != "mouse":
            self.mouse.stop_all()

        if self.config.get("general", {}).get("notifications", False):
            if self.current_mode == "mouse":
                system.send_notification("Mouse Mode", "Cursor control & navigation", icon="input-mouse-symbolic")
            else:
                system.send_notification("Media Mode", "Playback & shortcuts", icon="media-playback-start-symbolic")

        if self.config.get("general", {}).get("sound_feedback", False):
            system.play_sound(self.current_mode, volume=0.28)

    def emit_key_tap(self, key_code):
        """Emit a key press followed immediately by release."""
        self.uinput.write(ecodes.EV_KEY, key_code, 1)
        self.uinput.syn()
        time.sleep(0.04)
        self.uinput.write(ecodes.EV_KEY, key_code, 0)
        self.uinput.syn()

    def emit_key_combination(self, key_names):
        """Emit key combo like ['KEY_LEFTALT', 'KEY_F4']."""
        codes = [getattr(ecodes, k.strip()) for k in key_names if hasattr(ecodes, k.strip())]
        for c in codes:
            self.uinput.write(ecodes.EV_KEY, c, 1)
        self.uinput.syn()
        time.sleep(0.1)
        for c in reversed(codes):
            self.uinput.write(ecodes.EV_KEY, c, 0)
        self.uinput.syn()

    def type_string(self, text: str):
        """Type text character by character directly into active Wayland focus via uinput."""
        for char in text:
            if char in CHAR_MAP:
                code, shift = CHAR_MAP[char]
                if shift:
                    self.uinput.write(ecodes.EV_KEY, ecodes.KEY_LEFTSHIFT, 1)
                    self.uinput.syn()
                self.uinput.write(ecodes.EV_KEY, code, 1)
                self.uinput.syn()
                time.sleep(0.005)
                self.uinput.write(ecodes.EV_KEY, code, 0)
                self.uinput.syn()
                if shift:
                    self.uinput.write(ecodes.EV_KEY, ecodes.KEY_LEFTSHIFT, 0)
                    self.uinput.syn()
                time.sleep(0.005)

    def execute_action(self, action_str: str):
        """Dispatch a single mapped action."""
        if not action_str:
            return

        logger.info(f"Executing action: {action_str}")

        if action_str == "action:toggle_mode":
            self.toggle_mode()
        elif action_str == "action:screen_off":
            system.toggle_displays("off")
        elif action_str == "action:lock_screen":
            system.lock_screen()
        elif action_str == "action:suspend":
            system.suspend_system()
        elif action_str == "action:play_pause":
            self.emit_key_tap(ecodes.KEY_PLAYPAUSE)
        elif action_str == "action:mouse_left_click":
            self.mouse.click(ecodes.BTN_LEFT)
        elif action_str == "action:mouse_right_click":
            self.mouse.click(ecodes.BTN_RIGHT)
        elif action_str == "action:mouse_middle_click":
            self.mouse.click(ecodes.BTN_MIDDLE)
        elif action_str == "action:mouse_drag":
            self.is_dragging = not self.is_dragging
            self.mouse.set_button_state(ecodes.BTN_LEFT, self.is_dragging)
            if self.config.get("general", {}).get("notifications", False):
                drag_status = "Enabled" if self.is_dragging else "Disabled"
                system.send_notification("Mouse Drag Lock", drag_status, icon="input-mouse-symbolic", replace_id=9943)
        elif action_str == "action:scroll_up":
            self.mouse.scroll(self.scroll_step)
        elif action_str == "action:scroll_down":
            self.mouse.scroll(-self.scroll_step)
        elif action_str == "action:quick_settings":
            self.emit_key_combination(["KEY_LEFTMETA", "KEY_F12"])
        elif action_str == "action:smart_select":
            if system.is_overview_active():
                self.emit_key_tap(ecodes.KEY_ENTER)
            else:
                self.emit_key_tap(ecodes.KEY_SPACE)
        elif action_str == "action:close_window":
            self.emit_key_combination(["KEY_LEFTALT", "KEY_F4"])
        elif action_str.startswith("key:"):
            key_name = action_str.split(":", 1)[1]
            if hasattr(ecodes, key_name):
                self.emit_key_tap(getattr(ecodes, key_name))
        elif action_str.startswith("keys:"):
            combo = action_str.split(":", 1)[1].split("+")
            self.emit_key_combination(combo)
        elif action_str.startswith("exec:"):
            cmd = action_str.split(":", 1)[1]
            system.execute_command(cmd)

    def handle_mouse_motion_event(self, binding: str, is_pressed: bool):
        """Handle directional mouse motions."""
        direction_map = {
            "mouse:move_up": "up",
            "mouse:move_down": "down",
            "mouse:move_left": "left",
            "mouse:move_right": "right"
        }
        direction = direction_map.get(binding)
        if direction:
            if is_pressed:
                self.mouse.start_motion(direction)
            else:
                self.mouse.stop_motion(direction)
            return True
        return False

    def broadcast_key_event(self, key_name: str, state: int):
        """Sends key event to UI live preview socket if active."""
        try:
            sock_path = f"/run/user/{os.getuid()}/chromecast_remote_ui.sock"
            if os.path.exists(sock_path):
                import socket
                with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as s:
                    s.sendto(f"{key_name}:{state}".encode("utf-8"), sock_path)
        except Exception:
            pass

    def handle_event(self, event):
        """Process an input event from the remote."""
        if event.type != ecodes.EV_KEY:
            return

        key_event = evdev.categorize(event)
        code = event.code
        state = key_event.keystate # 0 = UP, 1 = DOWN, 2 = HOLD

        # Get key name(s) - handle both string and tuple/list of aliases
        if isinstance(key_event.keycode, (list, tuple)):
            key_names = list(key_event.keycode)
        else:
            key_names = [key_event.keycode]

        if state == 1:
            logger.info(f"Remote key pressed: {key_names} (code {code})")
        
        # Broadcast to UI socket for live visual simulation
        for k in key_names:
            self.broadcast_key_event(k, state)

        # Check if testing mode is active (actions disabled for live UI testing)
        test_mode_file = f"/run/user/{os.getuid()}/chromecast_remote_test_mode"
        if os.path.exists(test_mode_file):
            if state == 1:
                logger.info(f"Test mode active: ignored action for {key_names}")
            return

        # Check current mode bindings
        mode_bindings = self.config.get("mode", {}).get(self.current_mode, {})
        binding = None
        matched_key_name = None
        for name in key_names:
            if name in mode_bindings:
                binding = mode_bindings[name]
                matched_key_name = name
                break

        if not binding:
            if state == 1:
                logger.warning(f"No binding for remote key: {key_names} (code {code})")
            return

        # Check if binding is a direct mouse motion
        if isinstance(binding, str) and binding.startswith("mouse:move_"):
            if state == 1:
                self.handle_mouse_motion_event(binding, True)
            elif state == 0:
                self.handle_mouse_motion_event(binding, False)
            return

        # If it's a simple tap string
        if isinstance(binding, str):
            if state == 1: # On initial press
                self.execute_action(binding)
            return

        # If it's a dict with tap / double_tap / long_press
        if isinstance(binding, dict):
            tap_action = binding.get("tap")
            double_tap_action = binding.get("double_tap")
            long_action = binding.get("long_press")

            if state == 1: # Key down
                is_second_tap = False
                if double_tap_action and code in self.recent_taps:
                    pending_tap = self.recent_taps.pop(code)
                    pending_tap["timer"].cancel()
                    is_second_tap = True

                press_data = {"time": time.time(), "fired_long": False, "is_second_tap": is_second_tap}

                def on_long_press():
                    press_data["fired_long"] = True
                    if long_action == "action:scroll_up":
                        self.mouse.start_scroll(self.scroll_step)
                    elif long_action == "action:scroll_down":
                        self.mouse.start_scroll(-self.scroll_step)
                    elif long_action:
                        self.execute_action(long_action)

                timer = threading.Timer(self.long_press_threshold, on_long_press)
                timer.daemon = True
                press_data["timer"] = timer
                self.pending_presses[code] = press_data
                timer.start()

            elif state == 0: # Key up
                press_data = self.pending_presses.pop(code, None)
                if press_data:
                    timer = press_data.get("timer")
                    if timer:
                        timer.cancel()
                    if press_data["fired_long"]:
                        if long_action in ("action:scroll_up", "action:scroll_down"):
                            self.mouse.stop_scroll()
                    else:
                        if press_data.get("is_second_tap") and double_tap_action:
                            self.execute_action(double_tap_action)
                        elif double_tap_action:
                            def on_single_tap():
                                self.recent_taps.pop(code, None)
                                if tap_action:
                                    self.execute_action(tap_action)

                            dt_timer = threading.Timer(0.28, on_single_tap)
                            dt_timer.daemon = True
                            self.recent_taps[code] = {"timer": dt_timer}
                            dt_timer.start()
                        else:
                            if tap_action:
                                self.execute_action(tap_action)

    def find_remote(self):
        """Find the Chromecast Remote event device."""
        name_pattern = self.config.get("device", {}).get("name_pattern", "Chromecast Remote").lower()
        for path in evdev.list_devices():
            try:
                dev = evdev.InputDevice(path)
                if name_pattern in dev.name.lower():
                    return dev
            except Exception:
                continue
        return None

    def run(self):
        """Main daemon loop with automatic reconnection and resilient exception handling."""
        logger.info("Chromecast Remote Daemon starting. Waiting for remote connection...")
        poll_interval = self.config.get("device", {}).get("reconnect_poll_interval", 1.5)
        grab_device = self.config.get("device", {}).get("grab_device", True)

        while self.is_running:
            try:
                self.config = load_config()
            except Exception:
                pass

            if not self.config.get("device", {}).get("enabled", True):
                time.sleep(poll_interval)
                continue

            device = self.find_remote()
            if not device:
                time.sleep(poll_interval)
                continue

            logger.info(f"Connected to remote: {device.name} at {device.path}")

            try:
                if grab_device:
                    try:
                        device.grab()
                        logger.info("Exclusively grabbed remote input.")
                    except Exception as e:
                        logger.warning(f"Could not grab remote device: {e}")

                for event in device.read_loop():
                    if not self.is_running:
                        break
                    self.handle_event(event)

            except Exception as e:
                logger.info(f"Remote disconnected or entering sleep: {e}")
            finally:
                try:
                    if grab_device:
                        device.ungrab()
                except Exception:
                    pass
                try:
                    device.close()
                except Exception:
                    pass

            time.sleep(poll_interval)

def main():
    daemon = RemoteDaemon()
    daemon.run()

if __name__ == "__main__":
    main()
