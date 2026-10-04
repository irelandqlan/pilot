#![allow(deprecated)]

use adw::prelude::*;
use gtk::glib;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AppConfig {
    pub device: DeviceConfig,
    pub general: GeneralConfig,
    pub mouse: MouseConfig,
    pub voice: VoiceConfig,
    #[serde(default)]
    pub mode: toml::Table,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DeviceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub name_pattern: String,
    pub grab_device: bool,
    pub reconnect_poll_interval: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct GeneralConfig {
    pub initial_mode: String,
    pub long_press_threshold_sec: f64,
    pub notifications: bool,
    pub sound_feedback: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MouseConfig {
    pub base_speed: f64,
    pub max_speed: f64,
    pub acceleration: f64,
    pub poll_rate_ms: u64,
    pub scroll_step: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub model: String,
    pub paste_method: String,
    pub min_duration_sec: f64,
    pub auto_spacing: bool,
    pub device: String,
    pub compute_type: String,
}

fn get_config_path() -> PathBuf {
    if let Ok(val) = std::env::var("PILOT_CONFIG") {
        return PathBuf::from(val);
    }
    if let Ok(val) = std::env::var("CHROMECAST_REMOTE_CONFIG") {
        return PathBuf::from(val);
    }
    let pilot_cfg = glib::user_config_dir().join("pilot").join("config.toml");
    if pilot_cfg.exists() {
        return pilot_cfg;
    }
    let legacy_cfg = glib::user_config_dir().join("chromecast-remote").join("config.toml");
    if legacy_cfg.exists() {
        return legacy_cfg;
    }
    let dev_cfg = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml"));
    if dev_cfg.exists() {
        return dev_cfg;
    }
    if glib::user_config_dir().join("pilot").exists() {
        pilot_cfg
    } else if glib::user_config_dir().join("chromecast-remote").exists() {
        legacy_cfg
    } else {
        pilot_cfg
    }
}

fn load_config() -> Result<AppConfig, String> {
    let path = get_config_path();
    if !path.exists() {
        return Err(format!("Config file not found at {}", path.display()));
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    toml::from_str(&content).map_err(|e| e.to_string())
}

fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let toml_str = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, toml_str).map_err(|e| e.to_string())
}

fn restart_all_services(remote_enabled: bool) {
    if remote_enabled {
        let _ = Command::new("systemctl")
            .args(["--user", "restart", "chromecast-remote.service"])
            .spawn();
    } else {
        let _ = Command::new("systemctl")
            .args(["--user", "stop", "chromecast-remote.service"])
            .spawn();
    }
    let _ = Command::new("systemctl")
        .args([
            "--user",
            "restart",
            "atvvoice.service",
            "chromecast-voice.service",
        ])
        .spawn();
}

fn is_service_active(service_name: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", service_name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn set_service_state(service_name: &str, start: bool) {
    let action = if start { "start" } else { "stop" };
    let _ = Command::new("systemctl")
        .args(["--user", action, service_name])
        .status();
    let enable_action = if start { "enable" } else { "disable" };
    let _ = Command::new("systemctl")
        .args(["--user", enable_action, service_name])
        .status();
}

fn setup_reset_button(
    spin_row: &adw::SpinRow,
    default_val: f64,
    tooltip: &str,
    on_change: Rc<dyn Fn()>,
) -> gtk::Button {
    let btn = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text(tooltip)
        .build();

    let update_visibility = {
        let btn = btn.clone();
        let spin_row = spin_row.clone();
        Rc::new(move || {
            let is_diff = (spin_row.value() - default_val).abs() > 0.001;
            btn.set_visible(is_diff);
            btn.set_sensitive(is_diff);
        })
    };

    update_visibility();

    let uv = update_visibility.clone();
    let oc = on_change.clone();
    spin_row.connect_value_notify(move |_| {
        uv();
        oc();
    });

    let sr = spin_row.clone();
    btn.connect_clicked(move |_| {
        sr.set_value(default_val);
    });

    spin_row.add_suffix(&btn);
    btn
}

fn get_ui_socket_path() -> String {
    let runtime_dir = glib::user_runtime_dir();
    format!("{}/chromecast_remote_ui.sock", runtime_dir.display())
}

fn get_test_mode_path() -> PathBuf {
    glib::user_runtime_dir().join("chromecast_remote_test_mode")
}

fn set_test_mode_active(active: bool) {
    let path = get_test_mode_path();
    if active {
        let _ = fs::write(&path, "1");
    } else {
        let _ = fs::remove_file(&path);
    }
}

#[derive(Clone, Copy)]
pub struct ButtonOption {
    pub name: &'static str,
    pub key_code: &'static str,
}

pub const CONFIGURABLE_BUTTONS: &[ButtonOption] = &[
    ButtonOption { name: "Center (Select / OK)", key_code: "KEY_SELECT" },
    ButtonOption { name: "D-Pad Up", key_code: "KEY_UP" },
    ButtonOption { name: "D-Pad Down", key_code: "KEY_DOWN" },
    ButtonOption { name: "D-Pad Left", key_code: "KEY_LEFT" },
    ButtonOption { name: "D-Pad Right", key_code: "KEY_RIGHT" },
    ButtonOption { name: "Back Button", key_code: "KEY_BACK" },
    ButtonOption { name: "Home Button", key_code: "KEY_HOMEPAGE" },
    ButtonOption { name: "Mute Button", key_code: "KEY_MUTE" },
    ButtonOption { name: "YouTube Button", key_code: "KEY_KBDILLUMUP" },
    ButtonOption { name: "Netflix Button", key_code: "KEY_CAMERA_ACCESS_TOGGLE" },
    ButtonOption { name: "Power Button", key_code: "KEY_SCREENLOCK" },
    ButtonOption { name: "TV / Input Button", key_code: "KEY_TV" },
    ButtonOption { name: "Volume Up", key_code: "KEY_VOLUMEUP" },
    ButtonOption { name: "Volume Down", key_code: "KEY_VOLUMEDOWN" },
];

pub struct ActionPreset {
    pub name: &'static str,
    pub action_val: &'static str,
}

pub const ACTION_PRESETS: &[ActionPreset] = &[
    ActionPreset { name: "Toggle Mode (Media / Mouse)", action_val: "action:toggle_mode" },
    ActionPreset { name: "Play / Pause Toggle", action_val: "action:play_pause" },
    ActionPreset { name: "System Volume Up", action_val: "key:KEY_VOLUMEUP" },
    ActionPreset { name: "System Volume Down", action_val: "key:KEY_VOLUMEDOWN" },
    ActionPreset { name: "Blank / Turn Off Monitors", action_val: "action:screen_off" },
    ActionPreset { name: "Suspend / Sleep PC", action_val: "action:suspend" },
    ActionPreset { name: "Lock Screen", action_val: "action:lock_screen" },
    ActionPreset { name: "GNOME Quick Settings Menu", action_val: "action:quick_settings" },
    ActionPreset { name: "Close Active Window (Alt+F4)", action_val: "action:close_window" },
    ActionPreset { name: "Mouse Left Click", action_val: "action:mouse_left_click" },
    ActionPreset { name: "Mouse Right Click", action_val: "action:mouse_right_click" },
    ActionPreset { name: "Mouse Middle Click", action_val: "action:mouse_middle_click" },
    ActionPreset { name: "Toggle Mouse Drag Lock", action_val: "action:mouse_drag" },
    ActionPreset { name: "Scroll Up (Wheel)", action_val: "action:scroll_up" },
    ActionPreset { name: "Scroll Down (Wheel)", action_val: "action:scroll_down" },
    ActionPreset { name: "Move Pointer Up (Accelerated)", action_val: "mouse:move_up" },
    ActionPreset { name: "Move Pointer Down (Accelerated)", action_val: "mouse:move_down" },
    ActionPreset { name: "Move Pointer Left (Accelerated)", action_val: "mouse:move_left" },
    ActionPreset { name: "Move Pointer Right (Accelerated)", action_val: "mouse:move_right" },
    ActionPreset { name: "Next Media Track", action_val: "key:KEY_NEXTSONG" },
    ActionPreset { name: "Previous Media Track", action_val: "key:KEY_PREVIOUSSONG" },
    ActionPreset { name: "Escape Key", action_val: "key:KEY_ESC" },
    ActionPreset { name: "Enter Key", action_val: "key:KEY_ENTER" },
    ActionPreset { name: "Spacebar", action_val: "key:KEY_SPACE" },
    ActionPreset { name: "Tab Key (Forward)", action_val: "key:KEY_TAB" },
    ActionPreset { name: "Shift + Tab (Reverse)", action_val: "keys:KEY_LEFTSHIFT+KEY_TAB" },
    ActionPreset { name: "Super Key (GNOME Overview)", action_val: "key:KEY_LEFTMETA" },
    ActionPreset { name: "Delete Previous Word (Ctrl+Backspace)", action_val: "keys:KEY_LEFTCTRL+KEY_BACKSPACE" },
    ActionPreset { name: "Toggle Fullscreen (F11)", action_val: "key:KEY_F11" },
];

pub fn get_button_actions(
    cfg: &AppConfig,
    mode_name: &str,
    key_code: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    if let Some(mode_table) = cfg.mode.get(mode_name).and_then(|v| v.as_table()) {
        let alias = match key_code {
            "KEY_SCREENLOCK" => Some("KEY_COFFEE"),
            "KEY_KBDILLUMUP" => Some("KEY_CAMERA_ACCESS_DISABLE"),
            "KEY_ASSISTANT" => Some("KEY_VOICECOMMAND"),
            _ => None,
        };
        let val = mode_table.get(key_code).or_else(|| alias.and_then(|a| mode_table.get(a)));
        if let Some(val) = val {
            if let Some(s) = val.as_str() {
                return (Some(s.to_string()), None, None);
            } else if let Some(tab) = val.as_table() {
                let tap = tab.get("tap").and_then(|v| v.as_str()).map(|s| s.to_string());
                let lp = tab.get("long_press").and_then(|v| v.as_str()).map(|s| s.to_string());
                let dt = tab.get("double_tap").and_then(|v| v.as_str()).map(|s| s.to_string());
                return (tap, lp, dt);
            }
        }
    }
    (None, None, None)
}

pub fn format_action_label(action_str: &str) -> String {
    if let Some(preset) = ACTION_PRESETS.iter().find(|p| p.action_val == action_str) {
        return preset.name.to_string();
    }
    if let Some(cmd) = action_str.strip_prefix("exec:") {
        return format!("Run: {}", cmd);
    }
    if let Some(k) = action_str.strip_prefix("key:") {
        return format!("Key: {}", k);
    }
    if let Some(k) = action_str.strip_prefix("keys:") {
        return format!("Keys: {}", k);
    }
    if let Some(m) = action_str.strip_prefix("mouse:") {
        return format!("Mouse: {}", m);
    }
    if let Some(a) = action_str.strip_prefix("action:") {
        return format!("Action: {}", a);
    }
    action_str.to_string()
}

pub fn set_button_actions(
    cfg: &mut AppConfig,
    mode_name: &str,
    key_code: &str,
    tap: Option<String>,
    long_press: Option<String>,
    double_tap: Option<String>,
) {
    let mode_val = cfg
        .mode
        .entry(mode_name.to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let Some(mode_table) = mode_val.as_table_mut() {
        let has_complex = long_press.is_some() || double_tap.is_some();
        if !has_complex {
            if let Some(t) = tap {
                mode_table.insert(key_code.to_string(), toml::Value::String(t.clone()));
                if key_code == "KEY_SCREENLOCK" {
                    mode_table.insert("KEY_COFFEE".to_string(), toml::Value::String(t.clone()));
                } else if key_code == "KEY_KBDILLUMUP" {
                    mode_table.insert("KEY_CAMERA_ACCESS_DISABLE".to_string(), toml::Value::String(t));
                }
            } else {
                mode_table.remove(key_code);
                if key_code == "KEY_SCREENLOCK" {
                    mode_table.remove("KEY_COFFEE");
                } else if key_code == "KEY_KBDILLUMUP" {
                    mode_table.remove("KEY_CAMERA_ACCESS_DISABLE");
                }
            }
        } else {
            let mut tab = toml::Table::new();
            if let Some(t) = tap {
                tab.insert("tap".to_string(), toml::Value::String(t));
            }
            if let Some(lp) = long_press {
                tab.insert("long_press".to_string(), toml::Value::String(lp));
            }
            if let Some(dt) = double_tap {
                tab.insert("double_tap".to_string(), toml::Value::String(dt));
            }
            if tab.is_empty() {
                mode_table.remove(key_code);
                if key_code == "KEY_SCREENLOCK" {
                    mode_table.remove("KEY_COFFEE");
                } else if key_code == "KEY_KBDILLUMUP" {
                    mode_table.remove("KEY_CAMERA_ACCESS_DISABLE");
                }
            } else {
                mode_table.insert(key_code.to_string(), toml::Value::Table(tab.clone()));
                if key_code == "KEY_SCREENLOCK" {
                    mode_table.insert("KEY_COFFEE".to_string(), toml::Value::Table(tab.clone()));
                } else if key_code == "KEY_KBDILLUMUP" {
                    mode_table.insert("KEY_CAMERA_ACCESS_DISABLE".to_string(), toml::Value::Table(tab));
                }
            }
        }
    }
}

fn main() {
    let resources = gtk::gio::Resource::from_data(&gtk::glib::Bytes::from_static(include_bytes!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/data/resources.gresource")
    )))
    .expect("Failed to load GResource bundle");
    gtk::gio::resources_register(&resources);

    let app = adw::Application::builder()
        .application_id("io.github.magnotec.Pilot")
        .build();

    app.connect_startup(|_| {
        if let Some(display) = gtk::gdk::Display::default() {
            let theme = gtk::IconTheme::for_display(&display);
            theme.add_resource_path("/io/github/magnotec/Pilot/icons");
            theme.add_search_path("/home/magnotec/.local/share/chromecast-remote/icons");
            theme.add_search_path("/home/magnotec/.local/share/icons/hicolor/scalable/actions");
        }
        load_custom_css();
    });

    app.connect_activate(build_ui);
    app.run();
}

fn load_custom_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        .remote-card {
            background: @card_bg_color;
            border-radius: 16px;
            padding: 24px 28px;
        }
        .remote-body {
            background: alpha(@window_fg_color, 0.04);
            border: 1px solid alpha(@window_fg_color, 0.08);
            border-radius: 60px;
            padding: 10px 8px 60px 8px;
        }
        .remote-dpad-dish {
            background: alpha(@window_fg_color, 0.05);
            border-radius: 50%;
            padding: 2px;
        }
        .remote-btn-dpad {
            border-radius: 50%;
            min-width: 30px;
            min-height: 30px;
            padding: 0;
            transition: all 120ms ease-out;
        }
        .remote-btn-center {
            border-radius: 50%;
            min-width: 42px;
            min-height: 42px;
            padding: 0;
            background: alpha(@window_fg_color, 0.07);
            transition: all 120ms ease-out;
        }
        .remote-btn-center:hover {
            background: alpha(@window_fg_color, 0.12);
        }
        .remote-btn-circle {
            border-radius: 50%;
            min-width: 34px;
            min-height: 34px;
            padding: 0;
            transition: all 120ms ease-out;
        }
        .remote-btn-circle-small {
            border-radius: 50%;
            min-width: 26px;
            min-height: 26px;
            padding: 0;
            transition: all 120ms ease-out;
        }
        .remote-btn-assistant {
            background-color: #f7a399;
            color: #202020;
        }
        .remote-btn-assistant:hover {
            background-color: #f8b5ad;
        }
        .remote-bottom-tray {
            background: alpha(@window_fg_color, 0.035);
            border-radius: 18px;
            padding: 3px 6px;
        }
        .remote-mic-dash {
            min-width: 10px;
            min-height: 2px;
            background: alpha(@window_fg_color, 0.25);
            border-radius: 2px;
        }
        .remote-vol-rocker {
            border-radius: 12px;
            background: alpha(@window_fg_color, 0.04);
            padding: 2px;
        }
        .remote-vol-btn {
            min-width: 22px;
            min-height: 32px;
            padding: 0;
            border-radius: 10px;
        }
        .remote-btn-pressed {
            background-color: @accent_bg_color;
            color: @accent_fg_color;
            box-shadow: 0 0 12px @accent_bg_color;
            transform: scale(0.93);
        }
        ",
    );
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_ui(app: &adw::Application) {
    let config_data = match load_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            return;
        }
    };

    let saved_config = Rc::new(RefCell::new(config_data.clone()));
    let config = Rc::new(RefCell::new(config_data));

    // Main Window
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Pilot")
        .default_width(860)
        .default_height(720)
        .width_request(365)
        .build();

    let toast_overlay = adw::ToastOverlay::new();

    // ==========================================
    // NAVIGATION SPLIT VIEW (With Responsive Collapsing)
    // ==========================================
    let split_view = adw::NavigationSplitView::new();
    split_view.set_min_sidebar_width(180.0);
    split_view.set_max_sidebar_width(280.0);
    split_view.set_sidebar_width_fraction(0.25);

    // Responsive Breakpoint (collapses split view when window width <= 760sp)
    let bp_condition = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        760.0,
        adw::LengthUnit::Sp,
    );
    let bp = adw::Breakpoint::new(bp_condition);
    {
        let split = split_view.clone();
        bp.connect_apply(move |_| {
            split.set_collapsed(true);
        });
    }
    {
        let split = split_view.clone();
        bp.connect_unapply(move |_| {
            split.set_collapsed(false);
        });
    }
    window.add_breakpoint(bp);

    // Sidebar:
    let sidebar_toolbar = adw::ToolbarView::new();
    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_show_title(true);
    sidebar_header.set_show_end_title_buttons(split_view.is_collapsed());

    // Primary Menu in Sidebar Header (GNOME Standard PopoverMenu, no icons, zero CSS)
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Restart Daemons"), Some("win.restart_daemons"));
    let section = gtk::gio::Menu::new();
    section.append(Some("About Pilot"), Some("win.about"));
    menu.append_section(None, &section);

    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .primary(true)
        .tooltip_text("Options")
        .build();
    sidebar_header.pack_end(&menu_btn);
    sidebar_toolbar.add_top_bar(&sidebar_header);

    // Window Actions for Menu
    let toast_c = toast_overlay.clone();
    let cfg_restart_c = config.clone();
    let act_restart = gtk::gio::SimpleAction::new("restart_daemons", None);
    act_restart.connect_activate(move |_, _| {
        restart_all_services(cfg_restart_c.borrow().device.enabled);
        toast_c.add_toast(adw::Toast::new("Restarting background daemons..."));
    });
    window.add_action(&act_restart);

    let win_ref_about = window.clone();
    let act_about = gtk::gio::SimpleAction::new("about", None);
    act_about.connect_activate(move |_, _| {
        let about = adw::AboutDialog::builder()
            .application_name("Pilot")
            .application_icon("io.github.magnotec.Pilot")
            .developer_name("magnotec")
            .version(env!("CARGO_PKG_VERSION"))
            .comments("Configure and customize your Bluetooth Chromecast remote for PC control and voice dictation on Linux.")
            .license_type(gtk::License::MitX11)
            .website("https://github.com/irelandqlan/pilot")
            .issue_url("https://github.com/irelandqlan/pilot/issues")
            .build();
        about.present(Some(&win_ref_about));
    });
    window.add_action(&act_about);

    // Sidebar ListBox (Nautilus style from Obelisk Launcher)
    let nav_list = gtk::ListBox::new();
    nav_list.set_selection_mode(if split_view.is_collapsed() {
        gtk::SelectionMode::None
    } else {
        gtk::SelectionMode::Single
    });
    nav_list.add_css_class("navigation-sidebar");

    fn create_sidebar_row(title: &str, icon_name: &str) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::builder().activatable(true).build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(6)
            .margin_end(6)
            .margin_top(12)
            .margin_bottom(12)
            .build();

        let icon = gtk::Image::builder()
            .icon_name(icon_name)
            .pixel_size(16)
            .build();

        let label = gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .build();

        content.append(&icon);
        content.append(&label);
        row.set_child(Some(&content));

        row
    }

    let nav_items = [
        ("general", "General", "preferences-system-symbolic"),
        ("mouse", "Mouse", "input-mouse-symbolic"),
        ("voice", "Voice", "audio-input-microphone-symbolic"),
        ("buttons", "Buttons", "input-keyboard-symbolic"),
    ];

    for (_id, label, icon) in nav_items {
        let row = create_sidebar_row(label, icon);
        nav_list.append(&row);
    }
    if !split_view.is_collapsed() {
        nav_list.select_row(nav_list.row_at_index(0).as_ref());
    }

    let sidebar_scroll = gtk::ScrolledWindow::new();
    sidebar_scroll.set_child(Some(&nav_list));
    sidebar_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    sidebar_scroll.set_vexpand(true);
    sidebar_scroll.set_propagate_natural_width(true);
    sidebar_toolbar.set_content(Some(&sidebar_scroll));

    let sidebar_page = adw::NavigationPage::new(&sidebar_toolbar, "Pilot");
    split_view.set_sidebar(Some(&sidebar_page));

    // Content:
    let content_toolbar = adw::ToolbarView::new();
    let content_header = adw::HeaderBar::new();
    // Do not disable start title buttons: allows the back button to automatically show when collapsed!

    // Save Button (Not highlighted initially unless changes exist)
    let save_btn = gtk::Button::builder()
        .label("Save")
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    content_header.pack_end(&save_btn);
    content_toolbar.add_top_bar(&content_header);

    // Unsaved Changes Banner (placed beneath header bar)
    let banner = adw::Banner::new("Unsaved Changes: Save to apply updates to background services.");
    banner.set_revealed(false);
    content_toolbar.add_top_bar(&banner);

    // Change detection system
    let check_changes = {
        let saved_cfg = saved_config.clone();
        let curr_cfg = config.clone();
        let s_btn = save_btn.clone();
        let ban = banner.clone();

        Rc::new(move || {
            let changed = *saved_cfg.borrow() != *curr_cfg.borrow();
            if changed {
                s_btn.set_sensitive(true);
                s_btn.add_css_class("suggested-action");
                ban.set_revealed(true);
            } else {
                s_btn.set_sensitive(false);
                s_btn.remove_css_class("suggested-action");
                ban.set_revealed(false);
            }
        })
    };

    let content_stack = gtk::Stack::new();
    content_stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    // ==========================================
    // PAGE 1: GENERAL
    // ==========================================
    let page_general = adw::PreferencesPage::new();

    // Master Switch to Disable / Enable Remote Control
    let group_master = adw::PreferencesGroup::new();
    let switch_master = adw::SwitchRow::builder()
        .title("Enable Remote Controller")
        .subtitle("Allow the physical Chromecast remote to control mouse, media, and keys")
        .active(config.borrow().device.enabled)
        .build();
    group_master.add(&switch_master);

    // Service Daemons Group (With clean play/pause controls and spinner feedback)
    let group_services = adw::PreferencesGroup::builder()
        .title("Service Daemons")
        .description("Background daemons translating Bluetooth remote signals and speech")
        .build();

    // Helper to create daemon row with Play/Pause button
    fn create_daemon_row(
        title: &str,
        desc: &str,
        icon_name: &str,
        service_name: &'static str,
    ) -> (adw::ActionRow, Rc<dyn Fn()>) {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(desc)
            .build();
        let icon = gtk::Image::from_icon_name(icon_name);
        row.add_prefix(&icon);

        let status_label = gtk::Label::new(None);
        status_label.set_valign(gtk::Align::Center);
        status_label.add_css_class("dim-label");
        row.add_suffix(&status_label);

        let toggle_btn = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["flat", "circular"])
            .build();
        row.add_suffix(&toggle_btn);

        let s_label_c = status_label.clone();
        let t_btn_c = toggle_btn.clone();
        let update_status = Rc::new(move || {
            let active = is_service_active(service_name);
            s_label_c.set_text(if active { "Running" } else { "Stopped" });
            t_btn_c.set_icon_name(if active {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            });
            t_btn_c.set_tooltip_text(Some(if active {
                "Stop daemon"
            } else {
                "Start daemon"
            }));
        });

        update_status();

        toggle_btn.connect_clicked({
            let btn_c = toggle_btn.clone();
            let u_c = update_status.clone();
            let s_label_c2 = status_label.clone();
            move |_| {
                let active = is_service_active(service_name);
                let new_state = !active;

                let spinner = gtk::Spinner::new();
                spinner.start();
                btn_c.set_child(Some(&spinner));
                btn_c.set_sensitive(false);
                s_label_c2.set_text(if new_state { "Starting..." } else { "Stopping..." });

                set_service_state(service_name, new_state);

                let btn_restore = btn_c.clone();
                let u_restore = u_c.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                    btn_restore.set_child(None::<&gtk::Widget>);
                    btn_restore.set_sensitive(true);
                    u_restore();
                });
            }
        });

        (row, update_status)
    }

    let (row_remote, update_remote) = create_daemon_row(
        "Remote Controller Daemon",
        "Translates Bluetooth inputs into mouse moves and virtual keys",
        "input-gaming-symbolic",
        "chromecast-remote.service",
    );
    let (row_voice, update_voice) = create_daemon_row(
        "Voice Typing Daemon",
        "Runs GPU/CPU Whisper speech-to-text service on session D-Bus",
        "audio-input-microphone-symbolic",
        "chromecast-voice.service",
    );
    let (row_atv, update_atv) = create_daemon_row(
        "BLE Microphone (atvvoice)",
        "Streams remote microphone audio packets into PipeWire",
        "bluetooth-symbolic",
        "atvvoice.service",
    );

    // Switch master toggle event
    switch_master.connect_active_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        let u_remote = update_remote.clone();
        move |sw| {
            let is_on = sw.is_active();
            cfg.borrow_mut().device.enabled = is_on;
            set_service_state("chromecast-remote.service", is_on);
            chk();
            let u = u_remote.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                u();
            });
        }
    });

    group_services.add(&row_remote);
    group_services.add(&row_voice);
    group_services.add(&row_atv);

    // General Behavior Group
    let group_behavior = adw::PreferencesGroup::builder()
        .title("Remote Behavior")
        .build();

    let initial_mode_row = adw::ComboRow::builder()
        .title("Starting Mode")
        .subtitle("Select default mode when launching service")
        .model(&gtk::StringList::new(&["Media Mode", "Mouse Mode"]))
        .selected(if config.borrow().general.initial_mode == "mouse" { 1 } else { 0 })
        .build();
    initial_mode_row.connect_selected_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            cfg.borrow_mut().general.initial_mode = if r.selected() == 1 { "mouse".into() } else { "media".into() };
            chk();
        }
    });

    let long_press_row = adw::SpinRow::with_range(0.1, 2.0, 0.05);
    long_press_row.set_title("Long-Press Duration (sec)");
    long_press_row.set_subtitle("Hold duration required to trigger sleep or secondary actions");
    long_press_row.set_value(config.borrow().general.long_press_threshold_sec);
    setup_reset_button(&long_press_row, 0.4, "Reset to recommended default (0.4s)", {
        let cfg = config.clone();
        let lp = long_press_row.clone();
        let chk = check_changes.clone();
        Rc::new(move || {
            cfg.borrow_mut().general.long_press_threshold_sec = lp.value();
            chk();
        })
    });

    let notif_row = adw::SwitchRow::builder()
        .title("Desktop Notifications")
        .subtitle("Show notification banner when switching modes")
        .active(config.borrow().general.notifications)
        .build();
    notif_row.connect_active_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            cfg.borrow_mut().general.notifications = r.is_active();
            chk();
        }
    });

    let sound_row = adw::SwitchRow::builder()
        .title("Audio Feedback")
        .subtitle("Play sound chime when switching modes")
        .active(config.borrow().general.sound_feedback)
        .build();
    sound_row.connect_active_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            cfg.borrow_mut().general.sound_feedback = r.is_active();
            chk();
        }
    });

    group_behavior.add(&initial_mode_row);
    group_behavior.add(&long_press_row);
    group_behavior.add(&notif_row);
    group_behavior.add(&sound_row);

    // Expandable Advanced Hardware Group
    let group_advanced = adw::PreferencesGroup::new();
    let expander_advanced = adw::ExpanderRow::builder()
        .title("Advanced Hardware and Connection")
        .subtitle("Device name filter, exclusive device grabbing, and reconnect polling")
        .show_enable_switch(false)
        .expanded(false)
        .build();

    let pattern_row = adw::EntryRow::builder()
        .title("Remote Name Filter")
        .text(&config.borrow().device.name_pattern)
        .build();
    pattern_row.connect_text_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            cfg.borrow_mut().device.name_pattern = r.text().to_string();
            chk();
        }
    });

    let grab_row = adw::SwitchRow::builder()
        .title("Exclusively Grab Remote")
        .subtitle("Hides physical remote buttons from other apps to prevent conflicting double-actions")
        .active(config.borrow().device.grab_device)
        .build();
    grab_row.connect_active_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            cfg.borrow_mut().device.grab_device = r.is_active();
            chk();
        }
    });

    let poll_row = adw::SpinRow::with_range(0.5, 10.0, 0.5);
    poll_row.set_title("Reconnect Poll Interval (sec)");
    poll_row.set_subtitle("Delay between automatic reconnect checks when remote enters Bluetooth sleep");
    poll_row.set_value(config.borrow().device.reconnect_poll_interval);
    setup_reset_button(&poll_row, 1.5, "Reset to recommended default (1.5s)", {
        let cfg = config.clone();
        let p = poll_row.clone();
        let chk = check_changes.clone();
        Rc::new(move || {
            cfg.borrow_mut().device.reconnect_poll_interval = p.value();
            chk();
        })
    });

    expander_advanced.add_row(&pattern_row);
    expander_advanced.add_row(&grab_row);
    expander_advanced.add_row(&poll_row);
    group_advanced.add(&expander_advanced);

    page_general.add(&group_master);
    page_general.add(&group_services);
    page_general.add(&group_behavior);
    page_general.add(&group_advanced);

    // ==========================================
    // PAGE 2: MOUSE
    // ==========================================
    let page_mouse = adw::PreferencesPage::new();

    let group_mouse = adw::PreferencesGroup::builder()
        .title("Cursor Dynamics and Acceleration")
        .description("Calibrate pointer velocity curve and wheel speed for couch navigation")
        .build();

    let base_speed_row = adw::SpinRow::with_range(1.0, 50.0, 1.0);
    base_speed_row.set_title("Base Speed (px/tick)");
    base_speed_row.set_subtitle("Starting cursor speed when D-Pad is first tapped");
    base_speed_row.set_value(config.borrow().mouse.base_speed);
    setup_reset_button(&base_speed_row, 12.0, "Reset to recommended default (12.0)", {
        let cfg = config.clone();
        let b = base_speed_row.clone();
        let chk = check_changes.clone();
        Rc::new(move || {
            cfg.borrow_mut().mouse.base_speed = b.value();
            chk();
        })
    });

    let max_speed_row = adw::SpinRow::with_range(10.0, 150.0, 5.0);
    max_speed_row.set_title("Maximum Speed (px/tick)");
    max_speed_row.set_subtitle("Top cursor speed reached during sustained holding");
    max_speed_row.set_value(config.borrow().mouse.max_speed);
    setup_reset_button(&max_speed_row, 75.0, "Reset to recommended default (75.0)", {
        let cfg = config.clone();
        let m = max_speed_row.clone();
        let chk = check_changes.clone();
        Rc::new(move || {
            cfg.borrow_mut().mouse.max_speed = m.value();
            chk();
        })
    });

    let accel_row = adw::SpinRow::with_range(1.0, 2.5, 0.05);
    accel_row.set_title("Acceleration Multiplier");
    accel_row.set_subtitle("Speed ramp-up rate applied every tick while holding direction");
    accel_row.set_value(config.borrow().mouse.acceleration);
    setup_reset_button(&accel_row, 1.25, "Reset to recommended default (1.25)", {
        let cfg = config.clone();
        let a = accel_row.clone();
        let chk = check_changes.clone();
        Rc::new(move || {
            cfg.borrow_mut().mouse.acceleration = a.value();
            chk();
        })
    });

    let poll_rate_row = adw::SpinRow::with_range(8.0, 32.0, 2.0);
    poll_rate_row.set_title("Poll Rate (ms)");
    poll_rate_row.set_subtitle("16ms corresponds to ~60 FPS smooth cursor rendering");
    poll_rate_row.set_value(config.borrow().mouse.poll_rate_ms as f64);
    setup_reset_button(&poll_rate_row, 16.0, "Reset to recommended default (16ms)", {
        let cfg = config.clone();
        let p = poll_rate_row.clone();
        let chk = check_changes.clone();
        Rc::new(move || {
            cfg.borrow_mut().mouse.poll_rate_ms = p.value() as u64;
            chk();
        })
    });

    let scroll_step_row = adw::SpinRow::with_range(1.0, 10.0, 1.0);
    scroll_step_row.set_title("Scroll Step Multiplier");
    scroll_step_row.set_subtitle("Lines scrolled per tick when scrolling feeds or web pages");
    scroll_step_row.set_value(config.borrow().mouse.scroll_step as f64);
    setup_reset_button(&scroll_step_row, 2.0, "Reset to recommended default (2.0)", {
        let cfg = config.clone();
        let s = scroll_step_row.clone();
        let chk = check_changes.clone();
        Rc::new(move || {
            cfg.borrow_mut().mouse.scroll_step = s.value() as i64;
            chk();
        })
    });

    group_mouse.add(&base_speed_row);
    group_mouse.add(&max_speed_row);
    group_mouse.add(&accel_row);
    group_mouse.add(&poll_rate_row);
    group_mouse.add(&scroll_step_row);

    page_mouse.add(&group_mouse);

    // ==========================================
    // PAGE 3: VOICE
    // ==========================================
    let page_voice = adw::PreferencesPage::new();

    let group_voice = adw::PreferencesGroup::builder()
        .title("Whisper Speech-to-Text Engine")
        .description("Hardware-accelerated neural transcription for remote mic and global desktop hotkey")
        .build();

    let voice_enable_row = adw::SwitchRow::builder()
        .title("Enable Voice Dictation")
        .subtitle("Hold Google Assistant button on remote to speak and dictate")
        .active(config.borrow().voice.enabled)
        .build();
    voice_enable_row.connect_active_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            cfg.borrow_mut().voice.enabled = r.is_active();
            chk();
        }
    });

    let models = ["tiny.en", "base.en", "small.en", "medium.en", "large-v3"];
    let current_model = config.borrow().voice.model.clone();
    let model_idx = models.iter().position(|&m| m == current_model).unwrap_or(3);

    let model_row = adw::ComboRow::builder()
        .title("Whisper Model")
        .subtitle("medium.en recommended (~0.6s latency on RTX GPU, exceptional accuracy)")
        .model(&gtk::StringList::new(&models))
        .selected(model_idx as u32)
        .build();
    model_row.connect_selected_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            let idx = r.selected() as usize;
            if idx < models.len() {
                cfg.borrow_mut().voice.model = models[idx].to_string();
                chk();
            }
        }
    });

    let devices = ["cuda", "cpu"];
    let current_device = config.borrow().voice.device.clone();
    let device_idx = devices.iter().position(|&d| d == current_device).unwrap_or(0);

    let device_row = adw::ComboRow::builder()
        .title("Compute Device")
        .subtitle("CUDA utilizes NVIDIA GPU Tensor Cores for instantaneous processing")
        .model(&gtk::StringList::new(&devices))
        .selected(device_idx as u32)
        .build();
    device_row.connect_selected_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            let idx = r.selected() as usize;
            if idx < devices.len() {
                cfg.borrow_mut().voice.device = devices[idx].to_string();
                chk();
            }
        }
    });

    let compute_types = ["float16", "int8", "int8_float16", "float32"];
    let current_ct = config.borrow().voice.compute_type.clone();
    let ct_idx = compute_types.iter().position(|&c| c == current_ct).unwrap_or(0);

    let compute_type_row = adw::ComboRow::builder()
        .title("Compute Precision")
        .subtitle("float16 is optimized for modern RTX graphics cards")
        .model(&gtk::StringList::new(&compute_types))
        .selected(ct_idx as u32)
        .build();
    compute_type_row.connect_selected_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            let idx = r.selected() as usize;
            if idx < compute_types.len() {
                cfg.borrow_mut().voice.compute_type = compute_types[idx].to_string();
                chk();
            }
        }
    });

    let paste_methods = ["keystrokes", "clipboard"];
    let current_paste = config.borrow().voice.paste_method.clone();
    let paste_idx = paste_methods.iter().position(|&p| p == current_paste).unwrap_or(0);

    let paste_row = adw::ComboRow::builder()
        .title("Text Insertion Method")
        .subtitle("Keystrokes: direct virtual uinput typing | Clipboard: wl-copy + Ctrl+V")
        .model(&gtk::StringList::new(&paste_methods))
        .selected(paste_idx as u32)
        .build();
    paste_row.connect_selected_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            let idx = r.selected() as usize;
            if idx < paste_methods.len() {
                cfg.borrow_mut().voice.paste_method = paste_methods[idx].to_string();
                chk();
            }
        }
    });

    let spacing_row = adw::SwitchRow::builder()
        .title("Auto-Spacing")
        .subtitle("Appends trailing space so multiple dictations flow smoothly")
        .active(config.borrow().voice.auto_spacing)
        .build();
    spacing_row.connect_active_notify({
        let cfg = config.clone();
        let chk = check_changes.clone();
        move |r| {
            cfg.borrow_mut().voice.auto_spacing = r.is_active();
            chk();
        }
    });

    let min_dur_row = adw::SpinRow::with_range(0.1, 1.0, 0.05);
    min_dur_row.set_title("Minimum Voice Duration (sec)");
    min_dur_row.set_subtitle("Short audio bursts below this threshold are discarded");
    min_dur_row.set_value(config.borrow().voice.min_duration_sec);
    setup_reset_button(&min_dur_row, 0.35, "Reset to recommended default (0.35s)", {
        let cfg = config.clone();
        let md = min_dur_row.clone();
        let chk = check_changes.clone();
        Rc::new(move || {
            cfg.borrow_mut().voice.min_duration_sec = md.value();
            chk();
        })
    });

    group_voice.add(&voice_enable_row);
    group_voice.add(&model_row);
    group_voice.add(&device_row);
    group_voice.add(&compute_type_row);
    group_voice.add(&paste_row);
    group_voice.add(&spacing_row);
    group_voice.add(&min_dur_row);

    // D-Bus Integration Test Group
    let group_dbus = adw::PreferencesGroup::builder()
        .title("Desktop Dictation D-Bus Service")
        .description("Test trigger voice typing from desktop without remote")
        .build();

    let test_row = adw::ActionRow::builder()
        .title("Trigger Dictation (Toggle)")
        .subtitle("Calls org.local.Dictation.Toggle via session bus")
        .build();

    let test_btn = gtk::Button::builder()
        .label("Toggle Mic")
        .valign(gtk::Align::Center)
        .build();

    test_btn.connect_clicked(|_| {
        let _ = Command::new("busctl")
            .args([
                "--user",
                "call",
                "org.local.Dictation",
                "/org/local/Dictation",
                "org.local.Dictation",
                "Toggle",
            ])
            .spawn();
    });

    test_row.add_suffix(&test_btn);
    group_dbus.add(&test_row);

    page_voice.add(&group_voice);
    page_voice.add(&group_dbus);

    // ==========================================
    // PAGE 4: BUTTONS (Simulated Remote + Direct Edit Dialog)
    // ==========================================
    let page_buttons = adw::PreferencesPage::new();

    let group_buttons_main = adw::PreferencesGroup::new();

    // Mode Switcher for Buttons tab (Media vs. Mouse)
    let switcher_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    switcher_box.set_halign(gtk::Align::Center);
    switcher_box.set_margin_bottom(6);

    let mode_stack = gtk::Stack::new();
    mode_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    let dummy_media = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let dummy_mouse = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mode_stack.add_titled(&dummy_media, Some("media"), "Media Mode");
    mode_stack.add_titled(&dummy_mouse, Some("mouse"), "Mouse Mode");

    let initial_is_mouse = config.borrow().general.initial_mode == "mouse";
    mode_stack.set_visible_child_name(if initial_is_mouse { "mouse" } else { "media" });

    let mode_seg_switcher = gtk::StackSwitcher::new();
    mode_seg_switcher.set_stack(Some(&mode_stack));
    mode_seg_switcher.set_css_classes(&["linked"]);
    switcher_box.append(&mode_seg_switcher);
    group_buttons_main.add(&switcher_box);

    // Card Container for Simulated Remote
    let card_box = gtk::Box::new(gtk::Orientation::Vertical, 14);
    card_box.set_css_classes(&["card", "remote-card"]);
    card_box.set_halign(gtk::Align::Center);
    card_box.set_valign(gtk::Align::Start);
    card_box.set_margin_top(8);
    card_box.set_margin_bottom(12);

    // Testing Mode Switch Row
    let test_switch_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    test_switch_box.set_halign(gtk::Align::Fill);
    test_switch_box.set_hexpand(true);
    test_switch_box.set_margin_start(4);
    test_switch_box.set_margin_end(4);

    let test_switch_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    test_switch_text.set_hexpand(true);

    let test_switch_title = gtk::Label::builder()
        .label("Disable Actions for Testing")
        .halign(gtk::Align::Start)
        .css_classes(["heading", "body"])
        .build();
    let test_switch_desc = gtk::Label::builder()
        .label("Test remote inputs safely without triggering desktop shortcuts")
        .halign(gtk::Align::Start)
        .css_classes(["caption", "dim-label"])
        .build();
    test_switch_text.append(&test_switch_title);
    test_switch_text.append(&test_switch_desc);

    let test_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(false)
        .build();

    test_switch_box.append(&test_switch_text);
    test_switch_box.append(&test_switch);
    card_box.append(&test_switch_box);

    let test_sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    test_sep.set_margin_top(2);
    test_sep.set_margin_bottom(2);
    card_box.append(&test_sep);

    // Live Event Status Banner inside card
    let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    status_box.set_halign(gtk::Align::Center);

    let status_icon = gtk::Image::from_icon_name("input-gaming-symbolic");
    status_icon.set_pixel_size(14);
    status_icon.set_css_classes(&["dim-label"]);

    let event_banner = gtk::Label::new(Some("Ready • Press physical remote buttons to test"));
    event_banner.set_css_classes(&["dim-label", "caption"]);

    status_box.append(&status_icon);
    status_box.append(&event_banner);
    card_box.append(&status_box);

    let is_test_mode = Arc::new(AtomicBool::new(false));
    let is_test_mode_clone = is_test_mode.clone();

    let banner_c = event_banner.clone();
    let icon_c = status_icon.clone();
    let toast_c = toast_overlay.clone();
    test_switch.connect_active_notify(move |sw| {
        let active = sw.is_active();
        is_test_mode_clone.store(active, Ordering::Relaxed);
        set_test_mode_active(active);
        if active {
            banner_c.set_text("Testing Mode Active — Remote actions are disabled");
            banner_c.remove_css_class("dim-label");
            banner_c.add_css_class("accent");
            icon_c.remove_css_class("dim-label");
            icon_c.add_css_class("accent");
            let toast = adw::Toast::new("Testing mode enabled: remote actions are disabled");
            toast.set_timeout(2);
            toast_c.add_toast(toast);
        } else {
            banner_c.set_text("Ready — Press physical remote buttons to test");
            banner_c.remove_css_class("accent");
            banner_c.add_css_class("dim-label");
            icon_c.remove_css_class("accent");
            icon_c.add_css_class("dim-label");
            let toast = adw::Toast::new("Testing mode disabled: remote actions are active");
            toast.set_timeout(2);
            toast_c.add_toast(toast);
        }
    });

    // Center layout: Simulated Remote + Side Volume Rocker
    let remote_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    remote_row.set_halign(gtk::Align::Center);
    remote_row.set_valign(gtk::Align::Center);

    let remote_container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    remote_container.set_css_classes(&["remote-body"]);
    remote_container.set_valign(gtk::Align::Center);
    remote_container.set_halign(gtk::Align::Center);

    let mut button_widgets: HashMap<String, gtk::Button> = HashMap::new();

    // 1. D-Pad Circular Dish
    let dpad_grid = gtk::Grid::new();
    dpad_grid.set_css_classes(&["remote-dpad-dish"]);
    dpad_grid.set_halign(gtk::Align::Center);
    dpad_grid.set_row_spacing(1);
    dpad_grid.set_column_spacing(1);

    let btn_up = gtk::Button::builder()
        .icon_name("pan-up-symbolic")
        .css_classes(["flat", "remote-btn-dpad"])
        .tooltip_text("D-Pad Up (KEY_UP)")
        .build();
    button_widgets.insert("KEY_UP".into(), btn_up.clone());

    let btn_down = gtk::Button::builder()
        .icon_name("pan-down-symbolic")
        .css_classes(["flat", "remote-btn-dpad"])
        .tooltip_text("D-Pad Down (KEY_DOWN)")
        .build();
    button_widgets.insert("KEY_DOWN".into(), btn_down.clone());

    let btn_left = gtk::Button::builder()
        .icon_name("pan-start-symbolic")
        .css_classes(["flat", "remote-btn-dpad"])
        .tooltip_text("D-Pad Left (KEY_LEFT)")
        .build();
    button_widgets.insert("KEY_LEFT".into(), btn_left.clone());

    let btn_right = gtk::Button::builder()
        .icon_name("pan-end-symbolic")
        .css_classes(["flat", "remote-btn-dpad"])
        .tooltip_text("D-Pad Right (KEY_RIGHT)")
        .build();
    button_widgets.insert("KEY_RIGHT".into(), btn_right.clone());

    let btn_center = gtk::Button::builder()
        .css_classes(["remote-btn-center"])
        .tooltip_text("Center Select (KEY_SELECT)")
        .build();
    let center_dot = gtk::Image::from_icon_name("media-record-symbolic");
    center_dot.set_pixel_size(8);
    center_dot.set_css_classes(&["dim-label"]);
    btn_center.set_child(Some(&center_dot));
    button_widgets.insert("KEY_SELECT".into(), btn_center.clone());

    dpad_grid.attach(&btn_up, 1, 0, 1, 1);
    dpad_grid.attach(&btn_left, 0, 1, 1, 1);
    dpad_grid.attach(&btn_center, 1, 1, 1, 1);
    dpad_grid.attach(&btn_right, 2, 1, 1, 1);
    dpad_grid.attach(&btn_down, 1, 2, 1, 1);
    remote_container.append(&dpad_grid);

    // 2. Row 1: Back (Left) & Google Assistant (Right)
    let row_nav = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row_nav.set_halign(gtk::Align::Center);

    let btn_back = gtk::Button::builder()
        .icon_name("go-previous-symbolic")
        .css_classes(["remote-btn-circle"])
        .tooltip_text("Back Button (KEY_BACK)")
        .build();
    button_widgets.insert("KEY_BACK".into(), btn_back.clone());

    let btn_assistant = gtk::Button::builder()
        .icon_name("google-assistant-symbolic")
        .css_classes(["remote-btn-circle", "remote-btn-assistant"])
        .tooltip_text("Google Assistant (Hardware Mic)\nStreams audio directly to Whisper for voice dictation\n(Click for Voice settings)")
        .build();
    button_widgets.insert("KEY_ASSISTANT".into(), btn_assistant.clone());
    button_widgets.insert("KEY_VOICECOMMAND".into(), btn_assistant.clone());

    row_nav.append(&btn_back);
    row_nav.append(&btn_assistant);
    remote_container.append(&row_nav);

    // 3. Row 2: Home (Left) & Mute (Right)
    let row_home_mute = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row_home_mute.set_halign(gtk::Align::Center);

    let btn_home = gtk::Button::builder()
        .icon_name("go-home-symbolic")
        .css_classes(["remote-btn-circle"])
        .tooltip_text("Home / Overview (KEY_HOMEPAGE)")
        .build();
    button_widgets.insert("KEY_HOMEPAGE".into(), btn_home.clone());

    let btn_mute = gtk::Button::builder()
        .icon_name("audio-volume-muted-symbolic")
        .css_classes(["remote-btn-circle"])
        .tooltip_text("Mute Button (KEY_MUTE)")
        .build();
    button_widgets.insert("KEY_MUTE".into(), btn_mute.clone());

    row_home_mute.append(&btn_home);
    row_home_mute.append(&btn_mute);
    remote_container.append(&row_home_mute);

    // 4. Row 3: YouTube (Left) & Netflix (Right)
    let row_apps = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row_apps.set_halign(gtk::Align::Center);

    let btn_yt = gtk::Button::builder()
        .icon_name("youtube-symbolic")
        .css_classes(["remote-btn-circle"])
        .tooltip_text("YouTube Button (KEY_KBDILLUMUP)")
        .build();
    button_widgets.insert("KEY_KBDILLUMUP".into(), btn_yt.clone());
    button_widgets.insert("KEY_CAMERA_ACCESS_DISABLE".into(), btn_yt.clone());

    let btn_nf = gtk::Button::builder()
        .icon_name("netflix-symbolic")
        .css_classes(["remote-btn-circle"])
        .tooltip_text("Netflix Button (KEY_CAMERA_ACCESS_TOGGLE)")
        .build();
    button_widgets.insert("KEY_CAMERA_ACCESS_TOGGLE".into(), btn_nf.clone());

    row_apps.append(&btn_yt);
    row_apps.append(&btn_nf);
    remote_container.append(&row_apps);

    // 5. Row 4: Power (Left), Mic Dash, TV/Input (Right)
    let bottom_tray = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    bottom_tray.set_css_classes(&["remote-bottom-tray"]);
    bottom_tray.set_halign(gtk::Align::Center);
    bottom_tray.set_valign(gtk::Align::Center);

    let btn_power = gtk::Button::builder()
        .icon_name("system-shutdown-symbolic")
        .css_classes(["remote-btn-circle-small"])
        .tooltip_text("Power Button (KEY_SCREENLOCK)")
        .build();
    button_widgets.insert("KEY_SCREENLOCK".into(), btn_power.clone());
    button_widgets.insert("KEY_COFFEE".into(), btn_power.clone());

    let mic_dash = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mic_dash.set_css_classes(&["remote-mic-dash"]);
    mic_dash.set_valign(gtk::Align::Center);

    let btn_tv = gtk::Button::builder()
        .icon_name("input-tv-symbolic")
        .css_classes(["remote-btn-circle-small"])
        .tooltip_text("TV / Input (KEY_TV)")
        .build();
    button_widgets.insert("KEY_TV".into(), btn_tv.clone());

    bottom_tray.append(&btn_power);
    bottom_tray.append(&mic_dash);
    bottom_tray.append(&btn_tv);
    remote_container.append(&bottom_tray);

    remote_row.append(&remote_container);

    // 6. Side Edge: Volume Rocker
    let vol_rocker = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vol_rocker.set_css_classes(&["linked", "remote-vol-rocker"]);
    vol_rocker.set_valign(gtk::Align::Start);
    vol_rocker.set_margin_top(88);
    vol_rocker.set_margin_start(4);

    let btn_vol_up = gtk::Button::builder()
        .icon_name("audio-volume-high-symbolic")
        .css_classes(["flat", "remote-vol-btn"])
        .tooltip_text("Volume Up (KEY_VOLUMEUP)")
        .build();
    button_widgets.insert("KEY_VOLUMEUP".into(), btn_vol_up.clone());

    let btn_vol_down = gtk::Button::builder()
        .icon_name("audio-volume-low-symbolic")
        .css_classes(["flat", "remote-vol-btn"])
        .tooltip_text("Volume Down (KEY_VOLUMEDOWN)")
        .build();
    button_widgets.insert("KEY_VOLUMEDOWN".into(), btn_vol_down.clone());

    vol_rocker.append(&btn_vol_up);
    vol_rocker.append(&btn_vol_down);
    remote_row.append(&vol_rocker);

    card_box.append(&remote_row);

    // Bottom hint inside card
    let click_caption = gtk::Label::new(Some("Click any remote button above to customize its actions"));
    click_caption.set_css_classes(&["dim-label", "caption"]);
    click_caption.set_halign(gtk::Align::Center);
    card_box.append(&click_caption);

    group_buttons_main.add(&card_box);
    page_buttons.add(&group_buttons_main);

    // Track active mode in buttons tab
    let active_mode = Rc::new(RefCell::new(
        if initial_is_mouse { "mouse".to_string() } else { "media".to_string() }
    ));

    // Dynamic Tooltip Updater for all buttons
    let update_all_tooltips = {
        let cfg_rc = config.clone();
        let act_mode_rc = active_mode.clone();
        let btns = vec![
            (btn_center.clone(), "Center (Select / OK)", "KEY_SELECT"),
            (btn_up.clone(), "D-Pad Up", "KEY_UP"),
            (btn_down.clone(), "D-Pad Down", "KEY_DOWN"),
            (btn_left.clone(), "D-Pad Left", "KEY_LEFT"),
            (btn_right.clone(), "D-Pad Right", "KEY_RIGHT"),
            (btn_back.clone(), "Back Button", "KEY_BACK"),
            (btn_home.clone(), "Home Button", "KEY_HOMEPAGE"),
            (btn_mute.clone(), "Mute Button", "KEY_MUTE"),
            (btn_yt.clone(), "YouTube Button", "KEY_KBDILLUMUP"),
            (btn_nf.clone(), "Netflix Button", "KEY_CAMERA_ACCESS_TOGGLE"),
            (btn_power.clone(), "Power Button", "KEY_SCREENLOCK"),
            (btn_tv.clone(), "TV / Input Button", "KEY_TV"),
            (btn_vol_up.clone(), "Volume Up", "KEY_VOLUMEUP"),
            (btn_vol_down.clone(), "Volume Down", "KEY_VOLUMEDOWN"),
        ];

        Rc::new(move || {
            let mode = act_mode_rc.borrow().clone();
            let cfg = cfg_rc.borrow();
            let mode_label = if mode == "mouse" { "Mouse Mode" } else { "Media Mode" };

            for (btn, name, code) in &btns {
                let (tap, lp, dt) = get_button_actions(&cfg, &mode, code);
                let mut text = format!("{} ({})\nMode: {}", name, code, mode_label);
                if let Some(t) = tap {
                    text.push_str(&format!("\n• Tap: {}", format_action_label(&t)));
                } else {
                    text.push_str("\n• Tap: (Unassigned)");
                }
                if let Some(l) = lp {
                    text.push_str(&format!("\n• Hold: {}", format_action_label(&l)));
                }
                if let Some(d) = dt {
                    text.push_str(&format!("\n• Double-Tap: {}", format_action_label(&d)));
                }
                btn.set_tooltip_text(Some(&text));
            }
        })
    };

    update_all_tooltips();

    // Mode switch listener
    {
        let act_mode_c = active_mode.clone();
        let u_tips = update_all_tooltips.clone();
        mode_stack.connect_visible_child_name_notify(move |stk| {
            let new_mode = stk.visible_child_name().unwrap_or_else(|| "media".into());
            *act_mode_c.borrow_mut() = new_mode.to_string();
            u_tips();
        });
    }

    // Function to open direct edit dialog for a specific button
    fn open_button_edit_dialog(
        parent_win: &adw::ApplicationWindow,
        btn_def: ButtonOption,
        mode_name: &str,
        cfg_ref: Rc<RefCell<AppConfig>>,
        toast_overlay_c: adw::ToastOverlay,
        chk_changes: Rc<dyn Fn()>,
        on_saved: Rc<dyn Fn()>,
    ) {
        let mode_title = if mode_name == "mouse" { "Mouse Mode" } else { "Media Mode" };
        let dialog = adw::PreferencesDialog::builder()
            .title(format!("{} — {}", btn_def.name, mode_title))
            .build();

        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::builder()
            .title("Configure Button Triggers")
            .description(format!(
                "Customize what {} does when tapped, held, or double-tapped in {}.",
                btn_def.name, mode_title
            ))
            .build();

        let (cur_tap, cur_lp, cur_dt) =
            get_button_actions(&cfg_ref.borrow(), mode_name, btn_def.key_code);

        let mut preset_labels: Vec<String> = Vec::new();
        preset_labels.push("(None / Unassigned)".to_string());
        for p in ACTION_PRESETS {
            preset_labels.push(p.name.to_string());
        }
        preset_labels.push("Custom Action / Command...".to_string());
        let custom_idx = (preset_labels.len() - 1) as u32;

        let str_list = gtk::StringList::new(
            &preset_labels
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        );

        let create_trigger_row = |title: &'static str, subtitle: &'static str, current_val: Option<String>| {
            let expander = adw::ExpanderRow::builder()
                .title(title)
                .subtitle(subtitle)
                .show_enable_switch(false)
                .expanded(current_val.is_some())
                .build();

            let initial_idx = match &current_val {
                None => 0,
                Some(v) => {
                    if let Some(pos) = ACTION_PRESETS.iter().position(|p| p.action_val == v.as_str()) {
                        (pos + 1) as u32
                    } else {
                        custom_idx
                    }
                }
            };

            let combo = adw::ComboRow::builder()
                .title("Preset")
                .model(&str_list)
                .selected(initial_idx)
                .build();

            let initial_text = current_val.unwrap_or_default();
            let entry = adw::EntryRow::builder()
                .title("Action String")
                .text(&initial_text)
                .visible(initial_idx == custom_idx)
                .build();

            let entry_c = entry.clone();
            combo.connect_selected_notify(move |c| {
                let sel = c.selected();
                if sel == 0 {
                    entry_c.set_visible(false);
                    entry_c.set_text("");
                } else if sel == custom_idx {
                    entry_c.set_visible(true);
                } else {
                    entry_c.set_visible(false);
                    let p_idx = (sel - 1) as usize;
                    if p_idx < ACTION_PRESETS.len() {
                        entry_c.set_text(ACTION_PRESETS[p_idx].action_val);
                    }
                }
            });

            expander.add_row(&combo);
            expander.add_row(&entry);

            let combo_c = combo.clone();
            let entry_c2 = entry.clone();
            let get_val = move || -> Option<String> {
                let sel = combo_c.selected();
                if sel == 0 {
                    None
                } else if sel == custom_idx {
                    let text = entry_c2.text().trim().to_string();
                    if text.is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                } else {
                    let p_idx = (sel - 1) as usize;
                    if p_idx < ACTION_PRESETS.len() {
                        Some(ACTION_PRESETS[p_idx].action_val.to_string())
                    } else {
                        None
                    }
                }
            };

            (expander, get_val)
        };

        let (row_tap, get_tap) = create_trigger_row("Tap Action", "Triggers upon a single click", cur_tap);
        let (row_lp, get_lp) = create_trigger_row(
            "Long-Press / Hold Action",
            "Triggers when holding button beyond threshold",
            cur_lp,
        );
        let (row_dt, get_dt) = create_trigger_row("Double-Tap Action", "Triggers upon a rapid double-tap", cur_dt);

        group.add(&row_tap);
        group.add(&row_lp);
        group.add(&row_dt);

        let apply_row = adw::ActionRow::builder()
            .title("Apply Action Changes")
            .subtitle("Staged changes apply when clicking Save in the main window")
            .build();

        let btn_apply = gtk::Button::builder()
            .label("Apply")
            .css_classes(["suggested-action"])
            .valign(gtk::Align::Center)
            .build();
        apply_row.add_suffix(&btn_apply);
        group.add(&apply_row);

        let d_weak = dialog.downgrade();
        let mode_str = mode_name.to_string();
        let key_code = btn_def.key_code.to_string();
        let btn_name = btn_def.name.to_string();

        btn_apply.connect_clicked({
            let cfg_clone = cfg_ref.clone();
            let t_overlay = toast_overlay_c.clone();
            let chk = chk_changes.clone();
            let os = on_saved.clone();
            move |_| {
                let new_tap = get_tap();
                let new_lp = get_lp();
                let new_dt = get_dt();

                set_button_actions(
                    &mut cfg_clone.borrow_mut(),
                    &mode_str,
                    &key_code,
                    new_tap,
                    new_lp,
                    new_dt,
                );

                chk();
                os();

                let toast = adw::Toast::new(&format!("Updated actions for {}", btn_name));
                toast.set_timeout(2);
                t_overlay.add_toast(toast);

                if let Some(d) = d_weak.upgrade() {
                    d.close();
                }
            }
        });

        page.add(&group);
        dialog.add(&page);
        dialog.present(Some(parent_win));
    }

    // Connect remote button clicks directly to dialog
    let wire_btn_dialog = |btn: &gtk::Button, btn_opt: ButtonOption| {
        let p_win = window.clone();
        let cfg_c = config.clone();
        let act_mode_c = active_mode.clone();
        let t_overlay = toast_overlay.clone();
        let chk = check_changes.clone();
        let u_tips = update_all_tooltips.clone();

        btn.connect_clicked(move |_| {
            let cur_mode = act_mode_c.borrow().clone();
            open_button_edit_dialog(
                &p_win,
                btn_opt,
                &cur_mode,
                cfg_c.clone(),
                t_overlay.clone(),
                chk.clone(),
                u_tips.clone(),
            );
        });
    };

    // ==========================================
    // STACK PAGES & SIDEBAR SWITCHING
    // ==========================================
    content_stack.add_named(&page_general, Some("general"));
    content_stack.add_named(&page_mouse, Some("mouse"));
    content_stack.add_named(&page_voice, Some("voice"));
    content_stack.add_named(&page_buttons, Some("buttons"));

    content_toolbar.set_content(Some(&content_stack));
    let content_page = adw::NavigationPage::new(&content_toolbar, "General");
    split_view.set_content(Some(&content_page));

    // Handle sidebar activation / selection (gnome-control-center style)
    let activate_page = {
        let c_stack = content_stack.clone();
        let c_page = content_page.clone();
        let s_view = split_view.clone();
        let nav = nav_list.clone();
        Rc::new(move |idx: i32| {
            let (target_id, title) = match idx {
                0 => ("general", "General"),
                1 => ("mouse", "Mouse"),
                2 => ("voice", "Voice"),
                3 => ("buttons", "Buttons"),
                _ => ("general", "General"),
            };
            c_stack.set_visible_child_name(target_id);
            c_page.set_title(title);
            s_view.set_show_content(true);
            if s_view.is_collapsed() {
                nav.unselect_all();
            } else if let Some(r) = nav.row_at_index(idx) {
                if nav.selected_row().map(|sr| sr.index()) != Some(idx) {
                    nav.select_row(Some(&r));
                }
            }
        })
    };

    wire_btn_dialog(&btn_center, CONFIGURABLE_BUTTONS[0]);
    wire_btn_dialog(&btn_up, CONFIGURABLE_BUTTONS[1]);
    wire_btn_dialog(&btn_down, CONFIGURABLE_BUTTONS[2]);
    wire_btn_dialog(&btn_left, CONFIGURABLE_BUTTONS[3]);
    wire_btn_dialog(&btn_right, CONFIGURABLE_BUTTONS[4]);
    wire_btn_dialog(&btn_back, CONFIGURABLE_BUTTONS[5]);
    wire_btn_dialog(&btn_home, CONFIGURABLE_BUTTONS[6]);
    wire_btn_dialog(&btn_mute, CONFIGURABLE_BUTTONS[7]);
    wire_btn_dialog(&btn_yt, CONFIGURABLE_BUTTONS[8]);
    wire_btn_dialog(&btn_nf, CONFIGURABLE_BUTTONS[9]);
    wire_btn_dialog(&btn_power, CONFIGURABLE_BUTTONS[10]);
    wire_btn_dialog(&btn_tv, CONFIGURABLE_BUTTONS[11]);
    wire_btn_dialog(&btn_vol_up, CONFIGURABLE_BUTTONS[12]);
    wire_btn_dialog(&btn_vol_down, CONFIGURABLE_BUTTONS[13]);

    // Dedicated Voice Assistant button handler (cannot be bound to standard actions)
    btn_assistant.connect_clicked({
        let p_win = window.clone();
        let act_page = activate_page.clone();
        move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading("Google Assistant / Voice")
                .body("This button controls the remote's hardware microphone over Bluetooth (BLE GATT voice service) and is dedicated to voice dictation with Whisper.\n\nBecause audio is streamed directly to the speech-to-text service, this button cannot be rebound to standard keyboard or media actions. Voice recognition, typing speed, and audio settings can be configured in the Voice settings tab.")
                .build();
            dialog.add_response("cancel", "Close");
            dialog.add_response("voice", "Voice Settings");
            dialog.set_response_appearance("voice", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("cancel"));
            dialog.choose(&p_win, gtk::gio::Cancellable::NONE, {
                let act_page = act_page.clone();
                move |choice| {
                    if choice == "voice" {
                        act_page(2);
                    }
                }
            });
        }
    });

    let act1 = activate_page.clone();
    nav_list.connect_row_activated(move |_, row| {
        act1(row.index());
    });

    let act2 = activate_page.clone();
    let s_view_sel = split_view.clone();
    nav_list.connect_row_selected(move |_, row| {
        if !s_view_sel.is_collapsed() {
            if let Some(r) = row {
                act2(r.index());
            }
        }
    });

    // Handle collapse & show-content state transitions
    let sb_hdr = sidebar_header.clone();
    let nav_col = nav_list.clone();
    let c_stack_col = content_stack.clone();
    split_view.connect_collapsed_notify(move |split| {
        let collapsed = split.is_collapsed();
        sb_hdr.set_show_end_title_buttons(collapsed);
        if collapsed {
            nav_col.set_selection_mode(gtk::SelectionMode::None);
            nav_col.unselect_all();
        } else {
            nav_col.set_selection_mode(gtk::SelectionMode::Single);
            let cur_page = c_stack_col.visible_child_name().unwrap_or_default();
            let idx = match cur_page.as_str() {
                "general" => 0,
                "mouse" => 1,
                "voice" => 2,
                "buttons" => 3,
                _ => 0,
            };
            if let Some(row) = nav_col.row_at_index(idx) {
                nav_col.select_row(Some(&row));
            }
        }
    });

    let nav_sc = nav_list.clone();
    split_view.connect_show_content_notify(move |split| {
        if split.is_collapsed() && !split.shows_content() {
            nav_sc.set_selection_mode(gtk::SelectionMode::None);
            nav_sc.unselect_all();
        }
    });

    toast_overlay.set_child(Some(&split_view));
    window.set_content(Some(&toast_overlay));

    // ==========================================
    // LIVE EVENT LISTENER THREAD (Unix Datagram)
    // ==========================================
    let (sender, receiver) = std::sync::mpsc::channel::<(String, i32)>();
    let socket_path = get_ui_socket_path();
    let is_running = Arc::new(AtomicBool::new(true));
    let is_running_clone = is_running.clone();

    thread::spawn(move || {
        let _ = fs::remove_file(&socket_path);
        let socket = match UnixDatagram::bind(&socket_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to bind UI live socket: {}", e);
                return;
            }
        };

        let mut buf = [0u8; 128];
        while is_running_clone.load(Ordering::Relaxed) {
            match socket.recv(&mut buf) {
                Ok(len) => {
                    if let Ok(msg) = std::str::from_utf8(&buf[..len]) {
                        if let Some((key, state_str)) = msg.split_once(':') {
                            let state: i32 = state_str.trim().parse().unwrap_or(0);
                            let _ = sender.send((key.trim().to_string(), state));
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = fs::remove_file(&socket_path);
    });

    let btn_map_rc = Rc::new(button_widgets);
    let event_banner_rc = event_banner.clone();
    let is_test_mode_rc = is_test_mode.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
        while let Ok((key_name, state)) = receiver.try_recv() {
            let is_pressed = state == 1 || state == 2;
            let state_label = if is_pressed { "Pressed" } else { "Released" };
            let prefix = if is_test_mode_rc.load(Ordering::Relaxed) {
                "Test Event: "
            } else {
                "Last Remote Event: "
            };
            event_banner_rc.set_text(&format!(
                "{}{} ({})",
                prefix, key_name, state_label
            ));

            if let Some(btn) = btn_map_rc.get(&key_name) {
                if is_pressed {
                    btn.add_css_class("remote-btn-pressed");
                } else {
                    btn.remove_css_class("remote-btn-pressed");
                }
            }
        }
        glib::ControlFlow::Continue
    });

    // Clean up socket and test mode on window destroy
    window.connect_destroy({
        let is_running_cleanup = is_running.clone();
        move |_| {
            is_running_cleanup.store(false, Ordering::Relaxed);
            let sock_path = get_ui_socket_path();
            let _ = fs::remove_file(sock_path);
            set_test_mode_active(false);
        }
    });

    // ==========================================
    // SAVE HANDLER
    // ==========================================
    let overlay_weak = toast_overlay.downgrade();
    let config_clone = config.clone();
    let saved_config_clone = saved_config.clone();
    let check_changes_clone = check_changes.clone();
    let u_remote_save = update_remote.clone();
    let u_voice_save = update_voice.clone();
    let u_atv_save = update_atv.clone();

    let perform_save = Rc::new(move || {
        let Some(overlay) = overlay_weak.upgrade() else { return };
        let cfg = config_clone.borrow().clone();

        match save_config(&cfg) {
            Ok(_) => {
                *saved_config_clone.borrow_mut() = cfg.clone();
                check_changes_clone();
                restart_all_services(cfg.device.enabled);
                let toast = adw::Toast::new("Configuration saved and services restarted");
                toast.set_timeout(3);
                overlay.add_toast(toast);

                let ur = u_remote_save.clone();
                let uv = u_voice_save.clone();
                let ua = u_atv_save.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
                    ur();
                    uv();
                    ua();
                });
            }
            Err(e) => {
                let toast = adw::Toast::new(&format!("Error saving config: {}", e));
                overlay.add_toast(toast);
            }
        }
    });

    save_btn.connect_clicked(move |_| {
        perform_save();
    });

    set_test_mode_active(false);
    window.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_action_label() {
        assert_eq!(
            format_action_label("action:play_pause"),
            "Play / Pause Toggle"
        );
        assert_eq!(
            format_action_label("exec:firefox"),
            "Run: firefox"
        );
        assert_eq!(
            format_action_label("key:KEY_ENTER"),
            "Enter Key"
        );
        assert_eq!(
            format_action_label("mouse:move_up"),
            "Move Pointer Up (Accelerated)"
        );
    }

    #[test]
    fn test_button_actions_get_and_set() {
        let mut cfg = AppConfig {
            device: DeviceConfig {
                enabled: true,
                name_pattern: "Chromecast Remote".into(),
                grab_device: true,
                reconnect_poll_interval: 1.0,
            },
            general: GeneralConfig {
                initial_mode: "media".into(),
                long_press_threshold_sec: 0.5,
                notifications: true,
                sound_feedback: false,
            },
            mouse: MouseConfig {
                base_speed: 10.0,
                max_speed: 100.0,
                acceleration: 1.5,
                poll_rate_ms: 10,
                scroll_step: 1,
            },
            voice: VoiceConfig {
                enabled: true,
                model: "base".into(),
                paste_method: "clipboard".into(),
                min_duration_sec: 0.5,
                auto_spacing: true,
                device: "cpu".into(),
                compute_type: "float32".into(),
            },
            mode: toml::Table::new(),
        };

        // Test single tap
        set_button_actions(
            &mut cfg,
            "media",
            "KEY_SELECT",
            Some("action:play_pause".into()),
            None,
            None,
        );

        let (tap, lp, dt) = get_button_actions(&cfg, "media", "KEY_SELECT");
        assert_eq!(tap, Some("action:play_pause".into()));
        assert_eq!(lp, None);
        assert_eq!(dt, None);

        // Test multi-action (tap + long_press)
        set_button_actions(
            &mut cfg,
            "media",
            "KEY_SELECT",
            Some("action:play_pause".into()),
            Some("action:toggle_mode".into()),
            None,
        );

        let (tap, lp, dt) = get_button_actions(&cfg, "media", "KEY_SELECT");
        assert_eq!(tap, Some("action:play_pause".into()));
        assert_eq!(lp, Some("action:toggle_mode".into()));
        assert_eq!(dt, None);
    }

    #[test]
    fn test_config_serialization() {
        let toml_str = r#"
[device]
enabled = true
name_pattern = "Chromecast Remote"
grab_device = true
reconnect_poll_interval = 1.0

[general]
initial_mode = "media"
long_press_threshold_sec = 0.5
notifications = true
sound_feedback = false

[mouse]
base_speed = 10.0
max_speed = 100.0
acceleration = 1.5
poll_rate_ms = 10
scroll_step = 1

[voice]
enabled = true
model = "base"
paste_method = "clipboard"
min_duration_sec = 0.5
auto_spacing = true
device = "cpu"
compute_type = "float32"

[mode.media]
KEY_SELECT = "action:play_pause"
"#;

        let cfg: AppConfig = toml::from_str(toml_str).expect("Failed to deserialize test config");
        assert!(cfg.device.enabled);
        assert_eq!(cfg.general.initial_mode, "media");
        let (tap, _, _) = get_button_actions(&cfg, "media", "KEY_SELECT");
        assert_eq!(tap, Some("action:play_pause".into()));
    }
}

