"""
Mouse controller: Handles smooth D-Pad cursor movement, acceleration, clicks, and scrolling.
"""

import time
import threading
from evdev import ecodes, UInput

class MouseController:
    def __init__(self, uinput_dev: UInput, base_speed=6.0, max_speed=40.0, accel_rate=1.15, poll_rate_ms=16):
        self.uinput = uinput_dev
        self.base_speed = base_speed
        self.max_speed = max_speed
        self.accel_rate = accel_rate
        self.poll_rate_sec = poll_rate_ms / 1000.0

        # State tracking for held directions: 'up', 'down', 'left', 'right'
        self.active_directions = set()
        self._lock = threading.Lock()
        self._thread = None
        self._running = False
        self.press_start_time = 0.0

    def start_motion(self, direction: str):
        """Called when a directional key is pressed."""
        with self._lock:
            if not self.active_directions:
                self.press_start_time = time.time()
            self.active_directions.add(direction)
            if not self._running:
                self._running = True
                self._thread = threading.Thread(target=self._motion_loop, daemon=True)
                self._thread.start()

    def stop_motion(self, direction: str):
        """Called when a directional key is released."""
        with self._lock:
            self.active_directions.discard(direction)
            if not self.active_directions:
                self._running = False

    def _motion_loop(self):
        """Background thread executing smooth cursor updates while directions are held."""
        while self._running:
            start_tick = time.time()
            with self._lock:
                if not self.active_directions:
                    self._running = False
                    break
                duration = time.time() - self.press_start_time
                # Acceleration curve: base_speed * (accel_rate ^ (duration * 8))
                speed = min(self.max_speed, self.base_speed * (self.accel_rate ** (duration * 7)))

                dx, dy = 0, 0
                if 'left' in self.active_directions:
                    dx -= speed
                if 'right' in self.active_directions:
                    dx += speed
                if 'up' in self.active_directions:
                    dy -= speed
                if 'down' in self.active_directions:
                    dy += speed

            if dx != 0:
                self.uinput.write(ecodes.EV_REL, ecodes.REL_X, int(dx))
            if dy != 0:
                self.uinput.write(ecodes.EV_REL, ecodes.REL_Y, int(dy))
            if dx != 0 or dy != 0:
                self.uinput.syn()

            elapsed = time.time() - start_tick
            sleep_time = max(0.001, self.poll_rate_sec - elapsed)
            time.sleep(sleep_time)

    def click(self, button=ecodes.BTN_LEFT):
        """Emit a single mouse button click."""
        self.uinput.write(ecodes.EV_KEY, button, 1)
        self.uinput.syn()
        time.sleep(0.05)
        self.uinput.write(ecodes.EV_KEY, button, 0)
        self.uinput.syn()

    def set_button_state(self, button, is_down: bool):
        """Set mouse button down or up (useful for click and drag)."""
        self.uinput.write(ecodes.EV_KEY, button, 1 if is_down else 0)
        self.uinput.syn()

    def scroll(self, delta: int):
        """
        Emit mouse scroll.
        delta: positive for scroll up, negative for scroll down
        """
        self.uinput.write(ecodes.EV_REL, ecodes.REL_WHEEL, delta)
        self.uinput.syn()

    def start_scroll(self, delta: int):
        """Start continuous scrolling in background thread."""
        with self._lock:
            self._scroll_delta = delta
            if not getattr(self, '_scrolling', False):
                self._scrolling = True
                self._scroll_thread = threading.Thread(target=self._scroll_loop, daemon=True)
                self._scroll_thread.start()

    def stop_scroll(self):
        """Stop continuous scrolling."""
        with self._lock:
            self._scrolling = False
            self._scroll_delta = 0

    def stop_all(self):
        """Stops all active motion and scrolling."""
        with self._lock:
            self.active_directions.clear()
            self._running = False
            self._scrolling = False
            self._scroll_delta = 0

    def _scroll_loop(self):
        while getattr(self, '_scrolling', False):
            delta = getattr(self, '_scroll_delta', 0)
            if delta != 0:
                self.scroll(delta)
            time.sleep(0.08)



