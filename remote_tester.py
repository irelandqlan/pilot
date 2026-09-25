#!/usr/bin/env python3
"""
Chromecast Remote - Key Inspector & Tester Tool
Run this script to see exact button presses and event codes from your Chromecast remote.
Usage:
    python3 remote_tester.py
"""

import sys
import evdev
from evdev import ecodes

def find_chromecast_remote(name_pattern="chromecast remote"):
    devices = [evdev.InputDevice(path) for path in evdev.list_devices()]
    for dev in devices:
        if name_pattern in dev.name.lower():
            return dev
    return None

def main():
    print("=" * 60)
    print("  Chromecast Remote - Button Event Inspector")
    print("=" * 60)
    
    device = find_chromecast_remote()
    if not device:
        print("\n[!] Chromecast Remote not found in connected input devices.")
        print("    Available input devices:")
        for path in evdev.list_devices():
            d = evdev.InputDevice(path)
            print(f"      - {d.path}: {d.name}")
        print("\nMake sure the remote is Bluetooth paired and wake it by pressing any button.")
        sys.exit(1)

    print(f"\n[+] Found remote: {device.name}")
    print(f"    Device path: {device.path}")
    print(f"    MAC / Uniq : {device.uniq}")
    print("\nPress buttons on your remote to test them (Press Ctrl+C to exit):\n")

    try:
        for event in device.read_loop():
            if event.type == ecodes.EV_KEY:
                key_event = evdev.categorize(event)
                state_str = "DOWN (Pressed)" if key_event.keystate == 1 else ("UP (Released)" if key_event.keystate == 0 else "HOLD (Repeating)")
                key_name = key_event.keycode if isinstance(key_event.keycode, str) else "/".join(key_event.keycode)
                print(f"-> Button: {key_name:<25} Code: {event.code:<5} State: {state_str}")
    except KeyboardInterrupt:
        print("\nExiting key inspector.")

if __name__ == "__main__":
    main()
