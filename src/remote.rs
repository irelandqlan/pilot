use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, EventType, InputEvent, Key, RelativeAxisType};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseDirection {
    Up,
    Down,
    Left,
    Right,
}

pub struct MouseEngine {
    uinput: Arc<Mutex<VirtualDevice>>,
    base_speed: f64,
    max_speed: f64,
    accel_rate: f64,
    poll_rate: Duration,
    active_directions: Arc<Mutex<HashSet<MouseDirection>>>,
    press_start: Arc<Mutex<Option<Instant>>>,
    running: Arc<AtomicBool>,
}

impl MouseEngine {
    pub fn new(
        uinput: Arc<Mutex<VirtualDevice>>,
        base_speed: f64,
        max_speed: f64,
        accel_rate: f64,
        poll_rate_ms: u64,
    ) -> Self {
        Self {
            uinput,
            base_speed,
            max_speed,
            accel_rate,
            poll_rate: Duration::from_millis(poll_rate_ms.max(5)),
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
            let base_speed = self.base_speed;
            let max_speed = self.max_speed;
            let accel_rate = self.accel_rate;
            let poll_rate = self.poll_rate;

            std::thread::spawn(move || {
                while running_c.load(Ordering::SeqCst) {
                    let tick_start = Instant::now();
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

    // Standard alphanumeric
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
        Key::KEY_SPACE, Key::KEY_CAPSLOCK, Key::KEY_F1, Key::KEY_F2, Key::KEY_F3,
        Key::KEY_F4, Key::KEY_F5, Key::KEY_F6, Key::KEY_F7, Key::KEY_F8, Key::KEY_F9,
        Key::KEY_F10, Key::KEY_F11, Key::KEY_F12, Key::KEY_UP, Key::KEY_PAGEUP,
        Key::KEY_LEFT, Key::KEY_RIGHT, Key::KEY_END, Key::KEY_DOWN, Key::KEY_PAGEDOWN,
        Key::KEY_INSERT, Key::KEY_DELETE, Key::KEY_MUTE, Key::KEY_VOLUMEDOWN, Key::KEY_VOLUMEUP,
        Key::KEY_POWER, Key::KEY_HOMEPAGE, Key::KEY_BACK, Key::KEY_PLAYPAUSE, Key::KEY_STOPCD,
        Key::KEY_PREVIOUSSONG, Key::KEY_NEXTSONG, Key::KEY_SEARCH, Key::KEY_LEFTMETA,
        Key::KEY_RIGHTMETA, Key::KEY_SELECT, Key::KEY_OK,
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

pub fn parse_key_str(name: &str) -> Option<Key> {
    match name.to_uppercase().as_str() {
        "KEY_UP" => Some(Key::KEY_UP),
        "KEY_DOWN" => Some(Key::KEY_DOWN),
        "KEY_LEFT" => Some(Key::KEY_LEFT),
        "KEY_RIGHT" => Some(Key::KEY_RIGHT),
        "KEY_SELECT" | "KEY_OK" => Some(Key::KEY_SELECT),
        "KEY_BACK" => Some(Key::KEY_BACK),
        "KEY_HOMEPAGE" | "KEY_HOME" => Some(Key::KEY_HOMEPAGE),
        "KEY_MUTE" => Some(Key::KEY_MUTE),
        "KEY_VOLUMEUP" | "KEY_VOL_UP" => Some(Key::KEY_VOLUMEUP),
        "KEY_VOLUMEDOWN" | "KEY_VOL_DOWN" => Some(Key::KEY_VOLUMEDOWN),
        "KEY_POWER" | "KEY_SCREENLOCK" => Some(Key::KEY_POWER),
        "KEY_PLAYPAUSE" => Some(Key::KEY_PLAYPAUSE),
        "KEY_NEXTSONG" => Some(Key::KEY_NEXTSONG),
        "KEY_PREVIOUSSONG" => Some(Key::KEY_PREVIOUSSONG),
        "KEY_ENTER" => Some(Key::KEY_ENTER),
        "KEY_SPACE" => Some(Key::KEY_SPACE),
        "KEY_BACKSPACE" => Some(Key::KEY_BACKSPACE),
        "KEY_TAB" => Some(Key::KEY_TAB),
        "KEY_ESC" => Some(Key::KEY_ESC),
        "KEY_DELETE" => Some(Key::KEY_DELETE),
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
                    return Some(parts[1].to_string());
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
    } else {
        "device-removed"
    };
    let _ = Command::new("canberra-gtk-play")
        .args(["-i", canberra_id])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn show_notification(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "Pilot",
            "-i",
            "io.github.magnotec.Pilot",
            "-h",
            "int:transient:1",
            "-h",
            "string:x-canonical-private-synchronous:pilot-mode",
            title,
            body,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn execute_action(
    action: &str,
    uinput: &Arc<Mutex<VirtualDevice>>,
    mouse: &MouseEngine,
    active_mode: &Arc<Mutex<String>>,
    available_modes: &[String],
    sound_enabled: bool,
    notif_enabled: bool,
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

    if let Some(k_str) = trimmed.strip_prefix("key:") {
        if let Some(key) = parse_key_str(k_str) {
            if let Ok(mut u) = uinput.lock() {
                let _ = u.emit(&[InputEvent::new(EventType::KEY, key.code(), 1)]);
            }
            std::thread::sleep(Duration::from_millis(30));
            if let Ok(mut u) = uinput.lock() {
                let _ = u.emit(&[InputEvent::new(EventType::KEY, key.code(), 0)]);
            }
        }
        return;
    }

    if let Some(act) = trimmed.strip_prefix("action:") {
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
                        show_notification("Pilot Mode Switched", &format!("Active Layer: {}", next));
                    }
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
                    show_notification("Pilot Mode Switched", &format!("Active Layer: {}", target));
                }
            }
            "play_pause" => {
                emit_key(uinput, Key::KEY_PLAYPAUSE);
            }
            "vol_up" => {
                emit_key(uinput, Key::KEY_VOLUMEUP);
            }
            "vol_down" => {
                emit_key(uinput, Key::KEY_VOLUMEDOWN);
            }
            "mute" => {
                emit_key(uinput, Key::KEY_MUTE);
            }
            "next_track" => {
                emit_key(uinput, Key::KEY_NEXTSONG);
            }
            "prev_track" => {
                emit_key(uinput, Key::KEY_PREVIOUSSONG);
            }
            "mouse_click" => {
                mouse.click(Key::BTN_LEFT);
            }
            "mouse_right_click" => {
                mouse.click(Key::BTN_RIGHT);
            }
            "mouse_double_click" => {
                mouse.click(Key::BTN_LEFT);
                std::thread::sleep(Duration::from_millis(80));
                mouse.click(Key::BTN_LEFT);
            }
            "mouse_scroll_up" => {
                mouse.scroll(1);
            }
            "mouse_scroll_down" => {
                mouse.scroll(-1);
            }
            _ => {}
        }
    }
}

fn emit_key(uinput: &Arc<Mutex<VirtualDevice>>, key: Key) {
    if let Ok(mut u) = uinput.lock() {
        let _ = u.emit(&[InputEvent::new(EventType::KEY, key.code(), 1)]);
    }
    std::thread::sleep(Duration::from_millis(35));
    if let Ok(mut u) = uinput.lock() {
        let _ = u.emit(&[InputEvent::new(EventType::KEY, key.code(), 0)]);
    }
}

pub fn run_remote_controller(
    stop_flag: Arc<AtomicBool>,
    config: crate::AppConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    eprintln!("[RemoteController] Initializing native Rust remote engine...");

    let uinput = Arc::new(Mutex::new(create_virtual_input_device()?));
    let mouse = MouseEngine::new(
        uinput.clone(),
        config.mouse.base_speed,
        config.mouse.max_speed,
        config.mouse.acceleration,
        config.mouse.poll_rate_ms,
    );

    let active_mode = Arc::new(Mutex::new(config.general.initial_mode.clone()));
    let available_modes = crate::get_available_modes(&config);
    let name_pattern = config.device.name_pattern.clone();
    let poll_interval = Duration::from_secs_f64(config.device.reconnect_poll_interval.max(0.5));
    let long_press_threshold = Duration::from_secs_f64(config.general.long_press_threshold_sec.max(0.1));

    while !stop_flag.load(Ordering::SeqCst) {
        eprintln!(
            "[RemoteController] Searching for input device matching '{}'...",
            name_pattern
        );
        let found = find_remote_device(&name_pattern);
        let (path, mut device) = match found {
            Some(pair) => pair,
            None => {
                std::thread::sleep(poll_interval);
                continue;
            }
        };

        eprintln!(
            "[RemoteController] Found device {:?} ({}). Grabbing device...",
            path,
            device.name().unwrap_or("Unknown")
        );

        if config.device.grab_device {
            if let Err(e) = device.grab() {
                eprintln!("[RemoteController] Warning: EVIOCGRAB failed: {}", e);
            }
        }

        // Button state tracking: key_code -> press_start_instant
        let mut key_presses: HashMap<u16, Instant> = HashMap::new();
        let mut last_released: Option<(u16, Instant)> = None;

        while !stop_flag.load(Ordering::SeqCst) {
            match device.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.event_type() != EventType::KEY {
                            continue;
                        }

                        let code = ev.code();
                        let value = ev.value();
                        let key_str = format!("{:?}", Key::new(code));
                        let cur_mode = active_mode.lock().unwrap().clone();

                        let (tap_act, lp_act, dt_act) =
                            crate::get_button_actions(&config, &cur_mode, &key_str);

                        // Check if key corresponds to continuous mouse motion in current mode
                        let mouse_dir = if let Some(ref a) = tap_act {
                            match a.as_str() {
                                "action:mouse_move_up" => Some(MouseDirection::Up),
                                "action:mouse_move_down" => Some(MouseDirection::Down),
                                "action:mouse_move_left" => Some(MouseDirection::Left),
                                "action:mouse_move_right" => Some(MouseDirection::Right),
                                _ => None,
                            }
                        } else {
                            None
                        };

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
                                            &available_modes,
                                            config.general.sound_feedback,
                                            config.general.notifications,
                                        );
                                    }
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
                                            &available_modes,
                                            config.general.sound_feedback,
                                            config.general.notifications,
                                        );
                                        last_released = None;
                                    } else if let Some(ref act) = tap_act {
                                        if mouse_dir.is_none() {
                                            execute_action(
                                                act,
                                                &uinput,
                                                &mouse,
                                                &active_mode,
                                                &available_modes,
                                                config.general.sound_feedback,
                                                config.general.notifications,
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
                    break;
                }
            }
        }

        if config.device.grab_device {
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
