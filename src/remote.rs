use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, EventType, InputEvent, Key, RelativeAxisType};
use std::collections::{HashMap, HashSet};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseDirection {
    Up,
    Down,
    Left,
    Right,
}

pub fn parse_mouse_dir(a: &str) -> Option<MouseDirection> {
    match a {
        "mouse:move_up" | "action:mouse_move_up" => Some(MouseDirection::Up),
        "mouse:move_down" | "action:mouse_move_down" => Some(MouseDirection::Down),
        "mouse:move_left" | "action:mouse_move_left" => Some(MouseDirection::Left),
        "mouse:move_right" | "action:mouse_move_right" => Some(MouseDirection::Right),
        _ => None,
    }
}

pub struct MouseEngine {
    uinput: Arc<Mutex<VirtualDevice>>,
    config_arc: Arc<RwLock<crate::AppConfig>>,
    active_directions: Arc<Mutex<HashSet<MouseDirection>>>,
    press_start: Arc<Mutex<Option<Instant>>>,
    running: Arc<AtomicBool>,
}

impl MouseEngine {
    pub fn new(
        uinput: Arc<Mutex<VirtualDevice>>,
        config_arc: Arc<RwLock<crate::AppConfig>>,
    ) -> Self {
        Self {
            uinput,
            config_arc,
            active_directions: Arc::new(Mutex::new(HashSet::new())),
            press_start: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_motion(&self, dir: MouseDirection) {
        let mut dirs = self.active_directions.lock().unwrap();
        if dirs.is_empty() {
            let mut start = self.press_start.lock().unwrap();
            *start = Some(Instant::now());
        }
        dirs.insert(dir);

        if !self.running.load(Ordering::SeqCst) {
            self.running.store(true, Ordering::SeqCst);

            let uinput_c = self.uinput.clone();
            let dirs_c = self.active_directions.clone();
            let start_c = self.press_start.clone();
            let running_c = self.running.clone();
            let cfg_c = self.config_arc.clone();

            std::thread::spawn(move || {
                while running_c.load(Ordering::SeqCst) {
                    let tick_start = Instant::now();
                    let (base_speed, max_speed, accel_rate, poll_rate) = {
                        let cfg = cfg_c.read().unwrap();
                        (
                            cfg.mouse.base_speed,
                            cfg.mouse.max_speed,
                            cfg.mouse.acceleration,
                            Duration::from_millis(cfg.mouse.poll_rate_ms.max(5)),
                        )
                    };

                    let (dx, dy) = {
                        let active = dirs_c.lock().unwrap();
                        if active.is_empty() {
                            running_c.store(false, Ordering::SeqCst);
                            break;
                        }
                        let elapsed = start_c
                            .lock()
                            .unwrap()
                            .map(|s| s.elapsed().as_secs_f64())
                            .unwrap_or(0.0);

                        let speed = (base_speed * accel_rate.powf(elapsed * 7.0)).min(max_speed);
                        let mut x = 0.0;
                        let mut y = 0.0;
                        if active.contains(&MouseDirection::Left) {
                            x -= speed;
                        }
                        if active.contains(&MouseDirection::Right) {
                            x += speed;
                        }
                        if active.contains(&MouseDirection::Up) {
                            y -= speed;
                        }
                        if active.contains(&MouseDirection::Down) {
                            y += speed;
                        }
                        (x as i32, y as i32)
                    };

                    if dx != 0 || dy != 0 {
                        let mut events = Vec::with_capacity(2);
                        if dx != 0 {
                            events.push(InputEvent::new(
                                EventType::RELATIVE,
                                RelativeAxisType::REL_X.0,
                                dx,
                            ));
                        }
                        if dy != 0 {
                            events.push(InputEvent::new(
                                EventType::RELATIVE,
                                RelativeAxisType::REL_Y.0,
                                dy,
                            ));
                        }
                        if let Ok(mut u) = uinput_c.lock() {
                            let _ = u.emit(&events);
                        }
                    }

                    let spent = tick_start.elapsed();
                    if spent < poll_rate {
                        std::thread::sleep(poll_rate - spent);
                    }
                }
            });
        }
    }

    pub fn stop_motion(&self, dir: MouseDirection) {
        let mut dirs = self.active_directions.lock().unwrap();
        dirs.remove(&dir);
        if dirs.is_empty() {
            self.running.store(false, Ordering::SeqCst);
            let mut start = self.press_start.lock().unwrap();
            *start = None;
        }
    }

    pub fn stop_all(&self) {
        let mut dirs = self.active_directions.lock().unwrap();
        dirs.clear();
        self.running.store(false, Ordering::SeqCst);
        let mut start = self.press_start.lock().unwrap();
        *start = None;
    }

    pub fn click(&self, btn: Key) {
        if let Ok(mut u) = self.uinput.lock() {
            let _ = u.emit(&[InputEvent::new(EventType::KEY, btn.code(), 1)]);
        }
        std::thread::sleep(Duration::from_millis(45));
        if let Ok(mut u) = self.uinput.lock() {
            let _ = u.emit(&[InputEvent::new(EventType::KEY, btn.code(), 0)]);
        }
    }

    pub fn drag_toggle(&self) {
        static DRAGGING: AtomicBool = AtomicBool::new(false);
        let was_dragging = DRAGGING.load(Ordering::SeqCst);
        let now_dragging = !was_dragging;
        DRAGGING.store(now_dragging, Ordering::SeqCst);
        let val = if now_dragging { 1 } else { 0 };
        if let Ok(mut u) = self.uinput.lock() {
            let _ = u.emit(&[InputEvent::new(EventType::KEY, Key::BTN_LEFT.code(), val)]);
        }
    }

    pub fn scroll(&self, delta: i32) {
        if let Ok(mut u) = self.uinput.lock() {
            let _ = u.emit(&[InputEvent::new(
                EventType::RELATIVE,
                RelativeAxisType::REL_WHEEL.0,
                delta,
            )]);
        }
    }
}

pub fn create_virtual_input_device() -> Result<VirtualDevice, std::io::Error> {
    let mut keys = AttributeSet::<Key>::new();

    let key_codes = [
        Key::KEY_ESC, Key::KEY_1, Key::KEY_2, Key::KEY_3, Key::KEY_4, Key::KEY_5,
        Key::KEY_6, Key::KEY_7, Key::KEY_8, Key::KEY_9, Key::KEY_0, Key::KEY_MINUS,
        Key::KEY_EQUAL, Key::KEY_BACKSPACE, Key::KEY_TAB, Key::KEY_Q, Key::KEY_W,
        Key::KEY_E, Key::KEY_R, Key::KEY_T, Key::KEY_Y, Key::KEY_U, Key::KEY_I,
        Key::KEY_O, Key::KEY_P, Key::KEY_LEFTBRACE, Key::KEY_RIGHTBRACE, Key::KEY_ENTER,
        Key::KEY_LEFTCTRL, Key::KEY_A, Key::KEY_S, Key::KEY_D, Key::KEY_F, Key::KEY_G,
        Key::KEY_H, Key::KEY_J, Key::KEY_K, Key::KEY_L, Key::KEY_SEMICOLON, Key::KEY_APOSTROPHE,
        Key::KEY_GRAVE, Key::KEY_LEFTSHIFT, Key::KEY_BACKSLASH, Key::KEY_Z, Key::KEY_X,
        Key::KEY_C, Key::KEY_V, Key::KEY_B, Key::KEY_N, Key::KEY_M, Key::KEY_COMMA,
        Key::KEY_DOT, Key::KEY_SLASH, Key::KEY_RIGHTSHIFT, Key::KEY_KPASTERISK, Key::KEY_LEFTALT,
        Key::KEY_RIGHTALT, Key::KEY_RIGHTCTRL, Key::KEY_SPACE, Key::KEY_CAPSLOCK,
        Key::KEY_F1, Key::KEY_F2, Key::KEY_F3, Key::KEY_F4, Key::KEY_F5, Key::KEY_F6,
        Key::KEY_F7, Key::KEY_F8, Key::KEY_F9, Key::KEY_F10, Key::KEY_F11, Key::KEY_F12,
        Key::KEY_UP, Key::KEY_PAGEUP, Key::KEY_LEFT, Key::KEY_RIGHT, Key::KEY_END,
        Key::KEY_DOWN, Key::KEY_PAGEDOWN, Key::KEY_INSERT, Key::KEY_DELETE, Key::KEY_HOME,
        Key::KEY_MUTE, Key::KEY_VOLUMEDOWN, Key::KEY_VOLUMEUP, Key::KEY_POWER,
        Key::KEY_HOMEPAGE, Key::KEY_BACK, Key::KEY_PLAYPAUSE, Key::KEY_STOPCD,
        Key::KEY_PREVIOUSSONG, Key::KEY_NEXTSONG, Key::KEY_SEARCH, Key::KEY_LEFTMETA,
        Key::KEY_RIGHTMETA, Key::KEY_SELECT, Key::KEY_OK,
        // Application and vendor buttons (588 = YouTube, 589 = Netflix on Chromecast)
        Key::new(587), Key::new(588), Key::new(589), Key::new(590), Key::new(591),
        // Mouse Buttons
        Key::BTN_LEFT, Key::BTN_RIGHT, Key::BTN_MIDDLE, Key::BTN_SIDE, Key::BTN_EXTRA,
    ];

    for k in key_codes {
        keys.insert(k);
    }

    let mut rels = AttributeSet::<RelativeAxisType>::new();
    rels.insert(RelativeAxisType::REL_X);
    rels.insert(RelativeAxisType::REL_Y);
    rels.insert(RelativeAxisType::REL_WHEEL);
    rels.insert(RelativeAxisType::REL_HWHEEL);

    VirtualDeviceBuilder::new()?
        .name("Pilot Virtual Input")
        .with_keys(&keys)?
        .with_relative_axes(&rels)?
        .build()
}

pub fn evdev_code_to_key_str(code: u16) -> String {
    match code {
        587 => "KEY_CAMERA_ACCESS_ENABLE".to_string(),
        588 => "KEY_CAMERA_ACCESS_DISABLE".to_string(), // YouTube button on Chromecast
        589 => "KEY_CAMERA_ACCESS_TOGGLE".to_string(),  // Netflix button on Chromecast
        590 => "KEY_ACCESSIBILITY".to_string(),
        591 => "KEY_DO_NOT_DISTURB".to_string(),
        584 => "KEY_DICTATE".to_string(),
        585 => "KEY_EMOJI_PICKER".to_string(),
        586 => "KEY_KBD_LAYOUT_NEXT".to_string(),
        _ => {
            let key = Key::new(code);
            let s = format!("{:?}", key);
            if s.starts_with("unknown key:") {
                format!("KEY_{}", code)
            } else {
                s
            }
        }
    }
}

pub fn parse_key_str(name: &str) -> Option<Key> {
    match name.to_uppercase().as_str() {
        // Navigation & D-Pad
        "KEY_UP" => Some(Key::KEY_UP),
        "KEY_DOWN" => Some(Key::KEY_DOWN),
        "KEY_LEFT" => Some(Key::KEY_LEFT),
        "KEY_RIGHT" => Some(Key::KEY_RIGHT),
        "KEY_PAGEUP" => Some(Key::KEY_PAGEUP),
        "KEY_PAGEDOWN" => Some(Key::KEY_PAGEDOWN),
        "KEY_HOME" => Some(Key::KEY_HOME),
        "KEY_END" => Some(Key::KEY_END),
        "KEY_INSERT" => Some(Key::KEY_INSERT),
        "KEY_DELETE" => Some(Key::KEY_DELETE),

        // Remote buttons
        "KEY_SELECT" | "KEY_OK" => Some(Key::KEY_SELECT),
        "KEY_BACK" => Some(Key::KEY_BACK),
        "KEY_HOMEPAGE" => Some(Key::KEY_HOMEPAGE),
        "KEY_MUTE" => Some(Key::KEY_MUTE),
        "KEY_VOLUMEUP" | "KEY_VOL_UP" => Some(Key::KEY_VOLUMEUP),
        "KEY_VOLUMEDOWN" | "KEY_VOL_DOWN" => Some(Key::KEY_VOLUMEDOWN),
        "KEY_POWER" | "KEY_SCREENLOCK" => Some(Key::KEY_POWER),
        "KEY_PLAYPAUSE" => Some(Key::KEY_PLAYPAUSE),
        "KEY_NEXTSONG" => Some(Key::KEY_NEXTSONG),
        "KEY_PREVIOUSSONG" => Some(Key::KEY_PREVIOUSSONG),

        // YouTube & Netflix buttons on Chromecast
        "KEY_CAMERA_ACCESS_DISABLE" | "KEY_KBDILLUMUP" | "KEY_YOUTUBE" | "YOUTUBE" | "KEY_588" => {
            Some(Key::new(588))
        }
        "KEY_CAMERA_ACCESS_TOGGLE" | "KEY_NETFLIX" | "NETFLIX" | "KEY_589" => {
            Some(Key::new(589))
        }

        // Modifiers
        "KEY_LEFTMETA" | "KEY_RIGHTMETA" | "KEY_META" | "KEY_SUPER" | "META" | "SUPER" => Some(Key::KEY_LEFTMETA),
        "KEY_LEFTCTRL" | "KEY_CTRL" | "CTRL" => Some(Key::KEY_LEFTCTRL),
        "KEY_RIGHTCTRL" => Some(Key::KEY_RIGHTCTRL),
        "KEY_LEFTSHIFT" | "KEY_SHIFT" | "SHIFT" => Some(Key::KEY_LEFTSHIFT),
        "KEY_RIGHTSHIFT" => Some(Key::KEY_RIGHTSHIFT),
        "KEY_LEFTALT" | "KEY_ALT" | "ALT" => Some(Key::KEY_LEFTALT),
        "KEY_RIGHTALT" => Some(Key::KEY_RIGHTALT),
        "KEY_CAPSLOCK" | "CAPSLOCK" => Some(Key::KEY_CAPSLOCK),

        // Core editing
        "KEY_ENTER" | "ENTER" | "RETURN" => Some(Key::KEY_ENTER),
        "KEY_SPACE" | "SPACE" => Some(Key::KEY_SPACE),
        "KEY_BACKSPACE" | "BACKSPACE" => Some(Key::KEY_BACKSPACE),
        "KEY_TAB" | "TAB" => Some(Key::KEY_TAB),
        "KEY_ESC" | "ESC" | "ESCAPE" => Some(Key::KEY_ESC),

        // Function keys
        "KEY_F1" => Some(Key::KEY_F1),
        "KEY_F2" => Some(Key::KEY_F2),
        "KEY_F3" => Some(Key::KEY_F3),
        "KEY_F4" => Some(Key::KEY_F4),
        "KEY_F5" => Some(Key::KEY_F5),
        "KEY_F6" => Some(Key::KEY_F6),
        "KEY_F7" => Some(Key::KEY_F7),
        "KEY_F8" => Some(Key::KEY_F8),
        "KEY_F9" => Some(Key::KEY_F9),
        "KEY_F10" => Some(Key::KEY_F10),
        "KEY_F11" => Some(Key::KEY_F11),
        "KEY_F12" => Some(Key::KEY_F12),

        // Letters
        "KEY_A" => Some(Key::KEY_A),
        "KEY_B" => Some(Key::KEY_B),
        "KEY_C" => Some(Key::KEY_C),
        "KEY_D" => Some(Key::KEY_D),
        "KEY_E" => Some(Key::KEY_E),
        "KEY_F" => Some(Key::KEY_F),
        "KEY_G" => Some(Key::KEY_G),
        "KEY_H" => Some(Key::KEY_H),
        "KEY_I" => Some(Key::KEY_I),
        "KEY_J" => Some(Key::KEY_J),
        "KEY_K" => Some(Key::KEY_K),
        "KEY_L" => Some(Key::KEY_L),
        "KEY_M" => Some(Key::KEY_M),
        "KEY_N" => Some(Key::KEY_N),
        "KEY_O" => Some(Key::KEY_O),
        "KEY_P" => Some(Key::KEY_P),
        "KEY_Q" => Some(Key::KEY_Q),
        "KEY_R" => Some(Key::KEY_R),
        "KEY_S" => Some(Key::KEY_S),
        "KEY_T" => Some(Key::KEY_T),
        "KEY_U" => Some(Key::KEY_U),
        "KEY_V" => Some(Key::KEY_V),
        "KEY_W" => Some(Key::KEY_W),
        "KEY_X" => Some(Key::KEY_X),
        "KEY_Y" => Some(Key::KEY_Y),
        "KEY_Z" => Some(Key::KEY_Z),

        // Numbers
        "KEY_0" => Some(Key::KEY_0),
        "KEY_1" => Some(Key::KEY_1),
        "KEY_2" => Some(Key::KEY_2),
        "KEY_3" => Some(Key::KEY_3),
        "KEY_4" => Some(Key::KEY_4),
        "KEY_5" => Some(Key::KEY_5),
        "KEY_6" => Some(Key::KEY_6),
        "KEY_7" => Some(Key::KEY_7),
        "KEY_8" => Some(Key::KEY_8),
        "KEY_9" => Some(Key::KEY_9),

        // Symbols
        "KEY_MINUS" => Some(Key::KEY_MINUS),
        "KEY_EQUAL" => Some(Key::KEY_EQUAL),
        "KEY_LEFTBRACE" => Some(Key::KEY_LEFTBRACE),
        "KEY_RIGHTBRACE" => Some(Key::KEY_RIGHTBRACE),
        "KEY_SEMICOLON" => Some(Key::KEY_SEMICOLON),
        "KEY_APOSTROPHE" => Some(Key::KEY_APOSTROPHE),
        "KEY_GRAVE" => Some(Key::KEY_GRAVE),
        "KEY_BACKSLASH" => Some(Key::KEY_BACKSLASH),
        "KEY_COMMA" => Some(Key::KEY_COMMA),
        "KEY_DOT" => Some(Key::KEY_DOT),
        "KEY_SLASH" => Some(Key::KEY_SLASH),

        // Mouse buttons
        "BTN_LEFT" => Some(Key::BTN_LEFT),
        "BTN_RIGHT" => Some(Key::BTN_RIGHT),
        "BTN_MIDDLE" => Some(Key::BTN_MIDDLE),
        _ => None,
    }
}

pub fn find_remote_device(pattern: &str) -> Option<(PathBuf, evdev::Device)> {
    let lower_pattern = pattern.to_lowercase();
    for (path, dev) in evdev::enumerate() {
        if let Some(name) = dev.name() {
            if name.to_lowercase().contains(&lower_pattern) {
                return Some((path, dev));
            }
        }
    }
    None
}

pub fn discover_chromecast_mac() -> Option<String> {
    if let Ok(output) = Command::new("bluetoothctl").arg("devices").output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.to_lowercase().contains("chromecast remote") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return Some(parts[1].to_uppercase());
                }
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir("/sys/class/bluetooth") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("hci") && name.contains(':') {
                    let mac = name.trim_start_matches(|c: char| c.is_alphabetic() || c.is_numeric() && c != ':');
                    if mac.split(':').count() == 6 {
                        return Some(mac.to_uppercase());
                    }
                }
            }
        }
    }

    None
}

pub fn play_sound(mode_name: &str) {
    let candidates = match mode_name {
        "mouse" => vec![
            "/usr/share/sounds/gnome/default/alerts/click.ogg",
            "/usr/share/sounds/freedesktop/stereo/device-added.oga",
        ],
        "media" => vec![
            "/usr/share/sounds/gnome/default/alerts/swing.ogg",
            "/usr/share/sounds/freedesktop/stereo/device-removed.oga",
        ],
        _ => vec!["/usr/share/sounds/freedesktop/stereo/dialog-information.oga"],
    };

    for path in candidates {
        if Path::new(path).exists() {
            let _ = Command::new("pw-play")
                .args(["--volume", "0.28", path])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            return;
        }
    }

    let canberra_id = if mode_name == "mouse" {
        "device-added"
    } else if mode_name == "media" {
        "device-removed"
    } else {
        "dialog-information"
    };

    let _ = Command::new("canberra-gtk-play")
        .args(["-i", canberra_id])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

static LAST_NOTIF_INFO: Mutex<Option<(u32, Instant)>> = Mutex::new(None);

pub fn show_mode_notification(target_mode: &str, config_arc: &Arc<RwLock<crate::AppConfig>>) {
    let mode_title = crate::get_mode_title(target_mode);
    let title = "Mode";
    let body = format!("Active Layer: {}", mode_title);

    let icon = {
        let cfg = config_arc.read().unwrap();
        crate::get_mode_icon(&cfg, target_mode)
    };

    let mut lock = LAST_NOTIF_INFO.lock().unwrap();
    let replaces_id = if let Some((prev_id, prev_time)) = *lock {
        if prev_time.elapsed() < Duration::from_millis(2500) {
            prev_id
        } else {
            0
        }
    } else {
        0
    };

    let now = Instant::now();

    // 1. Try gdbus first: calls org.freedesktop.Notifications.Notify directly on session bus,
    // which bypasses portal icon stripping inside Flatpak and supports custom symbolic icons smoothly.
    let gdbus_res = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest", "org.freedesktop.Notifications",
            "--object-path", "/org/freedesktop/Notifications",
            "--method", "org.freedesktop.Notifications.Notify",
            "Pilot",
            &replaces_id.to_string(),
            &icon,
            title,
            &body,
            "[]",
            "{'transient': <true>}",
            "1500",
        ])
        .output();

    if let Ok(output) = gdbus_res {
        if output.status.success() {
            if let Ok(s) = std::str::from_utf8(&output.stdout) {
                if let Some(start) = s.find("uint32 ") {
                    let rest = &s[start + 7..];
                    if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                        if let Ok(id) = rest[..end].parse::<u32>() {
                            *lock = Some((id, now));
                            return;
                        }
                    }
                }
            }
            return;
        }
    }

    // 2. Fallback to notify-send
    let mut cmd = Command::new("notify-send");
    cmd.args([
        "-p",
        "-a", "Pilot",
        "-i", &icon,
        "-t", "1500",
        "-e",
        "-h", "int:transient:1",
        "-h", "string:x-canonical-private-synchronous:mode",
    ]);

    if replaces_id != 0 {
        cmd.args(["-r", &replaces_id.to_string()]);
    }

    cmd.arg(title);
    cmd.arg(&body);

    if let Ok(output) = cmd.output() {
        if let Ok(s) = std::str::from_utf8(&output.stdout) {
            if let Ok(id) = s.trim().parse::<u32>() {
                *lock = Some((id, now));
            }
        }
    }
}

fn emit_key(uinput: &Arc<Mutex<VirtualDevice>>, key: Key) {
    if let Ok(mut u) = uinput.lock() {
        let _ = u.emit(&[InputEvent::new(EventType::KEY, key.code(), 1)]);
    }
    std::thread::sleep(Duration::from_millis(30));
    if let Ok(mut u) = uinput.lock() {
        let _ = u.emit(&[InputEvent::new(EventType::KEY, key.code(), 0)]);
    }
}

fn emit_key_combo(uinput: &Arc<Mutex<VirtualDevice>>, combo: &str) {
    let parts: Vec<&str> = combo.split('+').map(|s| s.trim()).collect();
    let mut resolved = Vec::new();
    for p in parts {
        if let Some(k) = parse_key_str(p) {
            resolved.push(k);
        }
    }
    if resolved.is_empty() {
        return;
    }
    if let Ok(mut u) = uinput.lock() {
        let events: Vec<InputEvent> = resolved
            .iter()
            .map(|k| InputEvent::new(EventType::KEY, k.code(), 1))
            .collect();
        let _ = u.emit(&events);
    }
    std::thread::sleep(Duration::from_millis(30));
    if let Ok(mut u) = uinput.lock() {
        let events: Vec<InputEvent> = resolved
            .iter()
            .rev()
            .map(|k| InputEvent::new(EventType::KEY, k.code(), 0))
            .collect();
        let _ = u.emit(&events);
    }
}

pub fn execute_action(
    action: &str,
    uinput: &Arc<Mutex<VirtualDevice>>,
    mouse: &MouseEngine,
    active_mode: &Arc<Mutex<String>>,
    config_arc: &Arc<RwLock<crate::AppConfig>>,
) {
    let trimmed = action.trim();
    if trimmed.is_empty() {
        return;
    }

    if let Some(cmd) = trimmed.strip_prefix("exec:") {
        let _ = Command::new("sh")
            .args(["-c", cmd])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        return;
    }

    if let Some(combo_str) = trimmed.strip_prefix("keys:") {
        emit_key_combo(uinput, combo_str);
        return;
    }

    if let Some(k_str) = trimmed.strip_prefix("key:") {
        if let Some(key) = parse_key_str(k_str) {
            emit_key(uinput, key);
        }
        return;
    }

    let act = if let Some(a) = trimmed.strip_prefix("action:") {
        a
    } else if let Some(m) = trimmed.strip_prefix("mouse:") {
        m
    } else {
        trimmed
    };

    let (sound_enabled, notif_enabled, available_modes, scroll_step) = {
        let cfg = config_arc.read().unwrap();
        (
            cfg.general.sound_feedback,
            cfg.general.notifications,
            crate::get_available_modes(&cfg),
            cfg.mouse.scroll_step,
        )
    };

    match act {
        "toggle_mode" => {
            let mut cur = active_mode.lock().unwrap();
            if let Some(idx) = available_modes.iter().position(|m| m == &*cur) {
                let next = &available_modes[(idx + 1) % available_modes.len()];
                *cur = next.clone();
                if sound_enabled {
                    play_sound(next);
                }
                if notif_enabled {
                    show_mode_notification(next, config_arc);
                }
            } else if let Some(first) = available_modes.first() {
                *cur = first.clone();
            }
        }
        s if s.starts_with("switch_mode:") => {
            let target = s.strip_prefix("switch_mode:").unwrap();
            let mut cur = active_mode.lock().unwrap();
            *cur = target.to_string();
            if sound_enabled {
                play_sound(target);
            }
            if notif_enabled {
                show_mode_notification(target, config_arc);
            }
        }
        "play_pause" => emit_key(uinput, Key::KEY_PLAYPAUSE),
        "vol_up" => emit_key(uinput, Key::KEY_VOLUMEUP),
        "vol_down" => emit_key(uinput, Key::KEY_VOLUMEDOWN),
        "mute" => emit_key(uinput, Key::KEY_MUTE),
        "next_track" => emit_key(uinput, Key::KEY_NEXTSONG),
        "prev_track" => emit_key(uinput, Key::KEY_PREVIOUSSONG),

        // Mouse actions
        "left_click" | "mouse_left_click" | "mouse_click" | "click" => mouse.click(Key::BTN_LEFT),
        "right_click" | "mouse_right_click" => mouse.click(Key::BTN_RIGHT),
        "middle_click" | "mouse_middle_click" => mouse.click(Key::BTN_MIDDLE),
        "double_click" | "mouse_double_click" => {
            mouse.click(Key::BTN_LEFT);
            std::thread::sleep(Duration::from_millis(70));
            mouse.click(Key::BTN_LEFT);
        }
        "drag" | "mouse_drag" => mouse.drag_toggle(),
        "scroll_up" | "mouse_scroll_up" => mouse.scroll(scroll_step as i32),
        "scroll_down" | "mouse_scroll_down" => mouse.scroll(-(scroll_step as i32)),

        // System actions
        "close_window" => {
            emit_key_combo(uinput, "KEY_LEFTALT+KEY_F4");
        }
        "quick_settings" => {
            emit_key_combo(uinput, "KEY_LEFTMETA+KEY_A");
        }
        "lock_screen" => {
            let _ = Command::new("busctl")
                .args(["--user", "call", "org.gnome.ScreenSaver", "/org/gnome/ScreenSaver", "org.gnome.ScreenSaver", "Lock"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
        "screen_off" => {
            let _ = Command::new("busctl")
                .args([
                    "--user", "set-property",
                    "org.gnome.Mutter.DisplayConfig",
                    "/org/gnome/Mutter/DisplayConfig",
                    "org.gnome.Mutter.DisplayConfig",
                    "PowerSaveMode", "i", "3"
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
        "suspend" => {
            let _ = Command::new("systemctl")
                .arg("suspend")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
        _ => {}
    }
}

fn broadcast_ui_event(key_str: &str, value: i32) {
    let socket_path = crate::get_ui_socket_path();
    if Path::new(&socket_path).exists() {
        if let Ok(sock) = UnixDatagram::unbound() {
            let msg = format!("{}:{}", key_str, value);
            let _ = sock.send_to(msg.as_bytes(), &socket_path);
        }
    }
}

fn is_test_mode_active() -> bool {
    crate::get_test_mode_path().exists()
}

pub fn run_remote_controller(
    stop_flag: Arc<AtomicBool>,
    config_arc: Arc<RwLock<crate::AppConfig>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    eprintln!("[RemoteController] Initializing native Rust remote engine...");

    let uinput = Arc::new(Mutex::new(create_virtual_input_device()?));
    let mouse = MouseEngine::new(uinput.clone(), config_arc.clone());

    let initial_mode = { config_arc.read().unwrap().general.initial_mode.clone() };
    let active_mode = Arc::new(Mutex::new(initial_mode));

    while !stop_flag.load(Ordering::SeqCst) {
        let (name_pattern, poll_interval, grab_device) = {
            let cfg = config_arc.read().unwrap();
            (
                cfg.device.name_pattern.clone(),
                Duration::from_secs_f64(cfg.device.reconnect_poll_interval.max(0.5)),
                cfg.device.grab_device,
            )
        };

        eprintln!(
            "[RemoteController] Searching for input device matching '{}'...",
            name_pattern
        );
        let found = find_remote_device(&name_pattern);
        let (path, mut device) = match found {
            Some(pair) => pair,
            None => {
                crate::set_portal_background_status("Searching for remote...");
                std::thread::sleep(poll_interval);
                continue;
            }
        };

        eprintln!(
            "[RemoteController] Found device {:?} ({}). Grabbing device...",
            path,
            device.name().unwrap_or("Unknown")
        );

        if grab_device {
            if let Err(e) = device.grab() {
                eprintln!("[RemoteController] Warning: EVIOCGRAB failed: {}", e);
            }
        }

        crate::set_portal_background_status("Connected");

        let fd = device.as_raw_fd();
        let mut poll_fd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };

        // Button state tracking: key_code -> press_start_instant
        let mut key_presses: HashMap<u16, Instant> = HashMap::new();
        let mut last_released: Option<(u16, Instant)> = None;

        while !stop_flag.load(Ordering::SeqCst) {
            // Wait up to 150ms for events so we can check stop_flag and config updates
            let ret = unsafe { libc::poll(&mut poll_fd, 1, 150) };
            if ret <= 0 {
                // Timeout or interrupted, loop to check stop_flag
                continue;
            }

            match device.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.event_type() != EventType::KEY {
                            continue;
                        }

                        let code = ev.code();
                        let value = ev.value();
                        let key_str = evdev_code_to_key_str(code);

                        // Broadcast to UI simulator socket for live visual testing
                        broadcast_ui_event(&key_str, value);

                        // If testing switch is ON in the UI, do not execute real actions
                        if is_test_mode_active() {
                            continue;
                        }

                        // Ensure active layer is still valid if layers were deleted
                        {
                            let mut cur = active_mode.lock().unwrap();
                            let cfg = config_arc.read().unwrap();
                            let modes = crate::get_available_modes(&cfg);
                            if !modes.contains(&*cur) {
                                if let Some(first) = modes.first() {
                                    *cur = first.clone();
                                }
                            }
                        }

                        let cur_mode = active_mode.lock().unwrap().clone();
                        let (tap_act, lp_act, dt_act, long_press_threshold) = {
                            let cfg = config_arc.read().unwrap();
                            let (tap, lp, dt) = crate::get_button_actions(&cfg, &cur_mode, &key_str);
                            let lp_thresh = Duration::from_secs_f64(cfg.general.long_press_threshold_sec.max(0.1));
                            (tap, lp, dt, lp_thresh)
                        };

                        // Check if key corresponds to continuous mouse motion in current mode
                        let mouse_dir = tap_act.as_deref().and_then(parse_mouse_dir);

                        if value == 1 {
                            // Key down
                            key_presses.insert(code, Instant::now());
                            if let Some(dir) = mouse_dir {
                                mouse.start_motion(dir);
                            }
                        } else if value == 0 {
                            // Key up
                            if let Some(dir) = mouse_dir {
                                mouse.stop_motion(dir);
                            }

                            if let Some(press_time) = key_presses.remove(&code) {
                                let duration = press_time.elapsed();
                                if duration >= long_press_threshold {
                                    // Long press
                                    if let Some(ref act) = lp_act {
                                        execute_action(
                                            act,
                                            &uinput,
                                            &mouse,
                                            &active_mode,
                                            &config_arc,
                                        );
                                    }
                                    last_released = None;
                                } else {
                                    // Check double-tap
                                    let is_dt = if let Some((prev_code, prev_time)) = last_released {
                                        prev_code == code
                                            && prev_time.elapsed() < Duration::from_millis(280)
                                    } else {
                                        false
                                    };

                                    if is_dt && dt_act.is_some() {
                                        execute_action(
                                            dt_act.as_ref().unwrap(),
                                            &uinput,
                                            &mouse,
                                            &active_mode,
                                            &config_arc,
                                        );
                                        last_released = None;
                                    } else if let Some(ref act) = tap_act {
                                        if mouse_dir.is_none() {
                                            execute_action(
                                                act,
                                                &uinput,
                                                &mouse,
                                                &active_mode,
                                                &config_arc,
                                            );
                                        }
                                        last_released = Some((code, Instant::now()));
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[RemoteController] Device read error/disconnected: {}", e);
                    mouse.stop_all();
                    crate::set_portal_background_status("Searching for remote...");
                    break;
                }
            }
        }

        if grab_device {
            let _ = device.ungrab();
        }

        if stop_flag.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(poll_interval);
    }

    mouse.stop_all();
    eprintln!("[RemoteController] Remote controller stopped.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_str() {
        assert_eq!(parse_key_str("KEY_LEFTMETA"), Some(Key::KEY_LEFTMETA));
        assert_eq!(parse_key_str("super"), Some(Key::KEY_LEFTMETA));
        assert_eq!(parse_key_str("meta"), Some(Key::KEY_LEFTMETA));
        assert_eq!(parse_key_str("KEY_HOME"), Some(Key::KEY_HOME));
        assert_eq!(parse_key_str("KEY_HOMEPAGE"), Some(Key::KEY_HOMEPAGE));
        assert_eq!(parse_key_str("KEY_ENTER"), Some(Key::KEY_ENTER));
        assert_eq!(parse_key_str("enter"), Some(Key::KEY_ENTER));
        assert_eq!(parse_key_str("KEY_BACK"), Some(Key::KEY_BACK));
        assert_eq!(parse_key_str("KEY_PLAYPAUSE"), Some(Key::KEY_PLAYPAUSE));
        assert_eq!(parse_key_str("space"), Some(Key::KEY_SPACE));
        assert_eq!(parse_key_str("invalid_key_xyz"), None);
    }

    #[test]
    fn test_parse_mouse_dir() {
        assert_eq!(parse_mouse_dir("mouse:move_up"), Some(MouseDirection::Up));
        assert_eq!(parse_mouse_dir("action:mouse_move_down"), Some(MouseDirection::Down));
        assert_eq!(parse_mouse_dir("mouse:move_left"), Some(MouseDirection::Left));
        assert_eq!(parse_mouse_dir("action:mouse_move_right"), Some(MouseDirection::Right));
        assert_eq!(parse_mouse_dir("key:KEY_ENTER"), None);
    }

    #[test]
    fn test_key_codes() {
        assert_eq!(evdev_code_to_key_str(588), "KEY_CAMERA_ACCESS_DISABLE");
        assert_eq!(evdev_code_to_key_str(589), "KEY_CAMERA_ACCESS_TOGGLE");
        assert_eq!(parse_key_str("KEY_CAMERA_ACCESS_DISABLE"), Some(Key::new(588)));
        assert_eq!(parse_key_str("KEY_CAMERA_ACCESS_TOGGLE"), Some(Key::new(589)));
        assert_eq!(parse_key_str("youtube"), Some(Key::new(588)));
        assert_eq!(parse_key_str("netflix"), Some(Key::new(589)));
    }
}
