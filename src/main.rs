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

const CONFIG_PATH: &str = "/home/magnotec/Projects/desktop/python/chromecast-pc-remote/config.toml";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub device: DeviceConfig,
    pub general: GeneralConfig,
    pub mouse: MouseConfig,
    pub voice: VoiceConfig,
    #[serde(default)]
    pub mode: toml::Table,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceConfig {
    pub name_pattern: String,
    pub grab_device: bool,
    pub reconnect_poll_interval: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GeneralConfig {
    pub initial_mode: String,
    pub long_press_threshold_sec: f64,
    pub notifications: bool,
    pub sound_feedback: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MouseConfig {
    pub base_speed: f64,
    pub max_speed: f64,
    pub acceleration: f64,
    pub poll_rate_ms: u64,
    pub scroll_step: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub model: String,
    pub paste_method: String,
    pub min_duration_sec: f64,
    pub auto_spacing: bool,
    pub device: String,
    pub compute_type: String,
}

fn load_config() -> Result<AppConfig, String> {
    let path = PathBuf::from(CONFIG_PATH);
    if !path.exists() {
        return Err(format!("Config file not found at {}", CONFIG_PATH));
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    toml::from_str(&content).map_err(|e| e.to_string())
}

fn save_config(config: &AppConfig) -> Result<(), String> {
    let toml_str = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(CONFIG_PATH, toml_str).map_err(|e| e.to_string())
}

fn restart_service(name: &str) {
    let _ = Command::new("systemctl")
        .args(["--user", "restart", name])
        .spawn();
}

fn restart_all_services() {
    let _ = Command::new("systemctl")
        .args([
            "--user",
            "restart",
            "atvvoice.service",
            "chromecast-remote.service",
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
}

fn get_ui_socket_path() -> String {
    let runtime_dir = glib::user_runtime_dir();
    format!("{}/chromecast_remote_ui.sock", runtime_dir.display())
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
    ActionPreset { name: "Blank / Turn Off Monitors", action_val: "action:screen_off" },
    ActionPreset { name: "Suspend / Sleep PC", action_val: "action:suspend" },
    ActionPreset { name: "Lock Screen", action_val: "action:lock_screen" },
    ActionPreset { name: "GNOME Quick Settings Menu", action_val: "action:quick_settings" },
    ActionPreset { name: "Smart Select (Enter / Space)", action_val: "action:smart_select" },
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
    ActionPreset { name: "Play / Pause Toggle", action_val: "action:play_pause" },
    ActionPreset { name: "System Volume Up", action_val: "key:KEY_VOLUMEUP" },
    ActionPreset { name: "System Volume Down", action_val: "key:KEY_VOLUMEDOWN" },
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

fn main() {
    let app = adw::Application::builder()
        .application_id("com.chromecast.Settings")
        .build();

    app.connect_startup(|_| {
        load_custom_css();
    });

    app.connect_activate(build_ui);
    app.run();
}

fn load_custom_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        .remote-body {
            background: alpha(@window_fg_color, 0.05);
            border: 2px solid alpha(@window_fg_color, 0.12);
            border-radius: 40px;
            padding: 24px 16px;
        }
        .remote-dpad-ring {
            background: alpha(@window_fg_color, 0.08);
            border: 1px solid alpha(@window_fg_color, 0.12);
            border-radius: 50%;
            padding: 4px;
        }
        .remote-btn {
            border-radius: 50%;
            min-width: 44px;
            min-height: 44px;
            font-size: 13px;
            font-weight: bold;
            padding: 0;
            transition: all 120ms ease-out;
        }
        .remote-btn-pill {
            border-radius: 18px;
            min-width: 60px;
            min-height: 38px;
            font-size: 11px;
            font-weight: 600;
        }
        .remote-btn-center {
            border-radius: 50%;
            min-width: 52px;
            min-height: 52px;
            font-size: 14px;
            background: alpha(@window_fg_color, 0.12);
        }
        .remote-btn-pressed {
            background: @accent_bg_color;
            color: @accent_fg_color;
            box-shadow: 0 0 14px @accent_bg_color;
            transform: scale(0.94);
        }
        .navigation-sidebar row {
            border-radius: 8px;
            margin: 2px 6px;
            padding: 0px;
        }
        .menu-box {
            padding: 4px;
        }
        .menu-btn {
            border-radius: 8px;
            padding: 6px 10px;
            margin: 0px 0px;
            font-weight: normal;
            transition: none;
        }
        .menu-btn:hover {
            background-color: alpha(currentColor, 0.08);
        }
        .menu-separator {
            margin: 4px 0;
            background-color: alpha(currentColor, 0.1);
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

    let config = Rc::new(RefCell::new(config_data));

    // Main Window
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Chromecast Remote")
        .default_width(860)
        .default_height(720)
        .build();

    let toast_overlay = adw::ToastOverlay::new();

    // ==========================================
    // NAVIGATION SPLIT VIEW
    // ==========================================
    let split_view = adw::NavigationSplitView::new();
    split_view.set_min_sidebar_width(220.0);
    split_view.set_max_sidebar_width(260.0);

    // Sidebar:
    let sidebar_toolbar = adw::ToolbarView::new();
    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_show_title(true);

    // Primary Menu Button in Sidebar Header (Nautilus / Obelisk Popover Menu)
    let popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .build();

    let menu_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["menu-box"])
        .width_request(210)
        .spacing(2)
        .build();

    fn build_menu_item(title: &str, icon_name: &str) -> gtk::Button {
        let btn = gtk::Button::builder()
            .has_frame(false)
            .css_classes(["flat", "menu-btn"])
            .build();
        let hbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        let icon = gtk::Image::builder()
            .icon_name(icon_name)
            .pixel_size(16)
            .build();
        let label = gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        hbox.append(&icon);
        hbox.append(&label);
        btn.set_child(Some(&hbox));
        btn
    }

    let restart_btn = build_menu_item("Restart Daemons", "view-refresh-symbolic");
    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.add_css_class("menu-separator");
    let about_btn = build_menu_item("About Chromecast Remote", "help-about-symbolic");

    menu_box.append(&restart_btn);
    menu_box.append(&sep);
    menu_box.append(&about_btn);
    popover.set_child(Some(&menu_box));

    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .popover(&popover)
        .build();
    sidebar_header.pack_end(&menu_btn);
    sidebar_toolbar.add_top_bar(&sidebar_header);

    let popover_c = popover.clone();
    let toast_c = toast_overlay.clone();
    restart_btn.connect_clicked(move |_| {
        popover_c.popdown();
        restart_all_services();
        toast_c.add_toast(adw::Toast::new("Restarting background daemons..."));
    });

    let popover_c2 = popover.clone();
    let win_ref_about = window.clone();
    about_btn.connect_clicked(move |_| {
        popover_c2.popdown();
        let about = adw::AboutDialog::builder()
            .application_name("Chromecast Remote Settings")
            .developer_name("magnotec")
            .version("1.0.0")
            .comments("Configure and customize your Bluetooth Chromecast remote for PC control and voice dictation on Linux.")
            .license_type(gtk::License::MitX11)
            .build();
        about.present(Some(&win_ref_about));
    });

    // Sidebar ListBox (Nautilus style from Obelisk Launcher)
    let nav_list = gtk::ListBox::new();
    nav_list.set_selection_mode(gtk::SelectionMode::Single);
    nav_list.add_css_class("navigation-sidebar");

    fn create_sidebar_row(title: &str, icon_name: &str) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::builder().activatable(true).build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(6)
            .margin_end(6)
            .margin_top(10)
            .margin_bottom(10)
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
    nav_list.select_row(nav_list.row_at_index(0).as_ref());

    let sidebar_scroll = gtk::ScrolledWindow::new();
    sidebar_scroll.set_child(Some(&nav_list));
    sidebar_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    sidebar_toolbar.set_content(Some(&sidebar_scroll));

    let sidebar_page = adw::NavigationPage::new(&sidebar_toolbar, "Chromecast");
    split_view.set_sidebar(Some(&sidebar_page));

    // Content:
    let content_toolbar = adw::ToolbarView::new();
    let content_header = adw::HeaderBar::new();

    // Clean "Save" Button (Text only, no icon)
    let save_btn = gtk::Button::builder()
        .label("Save")
        .css_classes(["suggested-action"])
        .valign(gtk::Align::Center)
        .build();
    content_header.pack_end(&save_btn);
    content_toolbar.add_top_bar(&content_header);

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
        .active(is_service_active("chromecast-remote.service"))
        .build();
    group_master.add(&switch_master);

    // Service Daemons Group (With clean GNOME spinner feedback)
    let group_services = adw::PreferencesGroup::builder()
        .title("Service Daemons")
        .description("Background daemons translating Bluetooth remote signals and speech")
        .build();

    // Helper to create daemon row
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

        let restart_btn = gtk::Button::builder()
            .icon_name("view-refresh-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat", "circular"])
            .tooltip_text("Restart daemon")
            .build();
        row.add_suffix(&restart_btn);

        let s_label_c = status_label.clone();
        let update_status = Rc::new(move || {
            let active = is_service_active(service_name);
            s_label_c.set_text(if active { "Running" } else { "Stopped" });
        });

        update_status();

        restart_btn.connect_clicked({
            let btn_c = restart_btn.clone();
            let u_c = update_status.clone();
            let s_label_c2 = status_label.clone();
            move |_| {
                let spinner = gtk::Spinner::new();
                spinner.start();
                btn_c.set_child(Some(&spinner));
                btn_c.set_sensitive(false);
                s_label_c2.set_text("Restarting...");

                restart_service(service_name);

                let btn_restore = btn_c.clone();
                let u_restore = u_c.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(700), move || {
                    btn_restore.set_child(None::<&gtk::Widget>);
                    btn_restore.set_icon_name("view-refresh-symbolic");
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
        let u_remote = update_remote.clone();
        move |sw| {
            let is_on = sw.is_active();
            set_service_state("chromecast-remote.service", is_on);
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

    let long_press_row = adw::SpinRow::with_range(0.1, 2.0, 0.05);
    long_press_row.set_title("Long-Press Duration (sec)");
    long_press_row.set_subtitle("Hold duration required to trigger sleep or secondary actions");
    long_press_row.set_value(config.borrow().general.long_press_threshold_sec);

    let btn_reset_lp = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text("Reset to recommended default (0.4s)")
        .build();
    btn_reset_lp.connect_clicked({
        let lp = long_press_row.clone();
        move |_| lp.set_value(0.4)
    });
    long_press_row.add_suffix(&btn_reset_lp);

    let notif_row = adw::SwitchRow::builder()
        .title("Desktop Notifications")
        .subtitle("Show notification banner when switching modes")
        .active(config.borrow().general.notifications)
        .build();

    let sound_row = adw::SwitchRow::builder()
        .title("Audio Feedback")
        .subtitle("Play sound chime when switching modes")
        .active(config.borrow().general.sound_feedback)
        .build();

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

    let grab_row = adw::SwitchRow::builder()
        .title("Exclusively Grab Remote")
        .subtitle("Hides physical remote buttons from other apps to prevent conflicting double-actions")
        .active(config.borrow().device.grab_device)
        .build();

    let poll_row = adw::SpinRow::with_range(0.5, 10.0, 0.5);
    poll_row.set_title("Reconnect Poll Interval (sec)");
    poll_row.set_subtitle("Delay between automatic reconnect checks when remote enters Bluetooth sleep");
    poll_row.set_value(config.borrow().device.reconnect_poll_interval);

    let btn_reset_poll = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text("Reset to recommended default (1.5s)")
        .build();
    btn_reset_poll.connect_clicked({
        let p = poll_row.clone();
        move |_| p.set_value(1.5)
    });
    poll_row.add_suffix(&btn_reset_poll);

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

    let btn_reset_base = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text("Reset to recommended default (12.0)")
        .build();
    btn_reset_base.connect_clicked({
        let b = base_speed_row.clone();
        move |_| b.set_value(12.0)
    });
    base_speed_row.add_suffix(&btn_reset_base);

    let max_speed_row = adw::SpinRow::with_range(10.0, 150.0, 5.0);
    max_speed_row.set_title("Maximum Speed (px/tick)");
    max_speed_row.set_subtitle("Top cursor speed reached during sustained holding");
    max_speed_row.set_value(config.borrow().mouse.max_speed);

    let btn_reset_max = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text("Reset to recommended default (75.0)")
        .build();
    btn_reset_max.connect_clicked({
        let m = max_speed_row.clone();
        move |_| m.set_value(75.0)
    });
    max_speed_row.add_suffix(&btn_reset_max);

    let accel_row = adw::SpinRow::with_range(1.0, 2.5, 0.05);
    accel_row.set_title("Acceleration Multiplier");
    accel_row.set_subtitle("Speed ramp-up rate applied every tick while holding direction");
    accel_row.set_value(config.borrow().mouse.acceleration);

    let btn_reset_accel = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text("Reset to recommended default (1.25)")
        .build();
    btn_reset_accel.connect_clicked({
        let a = accel_row.clone();
        move |_| a.set_value(1.25)
    });
    accel_row.add_suffix(&btn_reset_accel);

    let poll_rate_row = adw::SpinRow::with_range(8.0, 32.0, 2.0);
    poll_rate_row.set_title("Poll Rate (ms)");
    poll_rate_row.set_subtitle("16ms corresponds to ~60 FPS smooth cursor rendering");
    poll_rate_row.set_value(config.borrow().mouse.poll_rate_ms as f64);

    let btn_reset_poll_rate = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text("Reset to recommended default (16ms)")
        .build();
    btn_reset_poll_rate.connect_clicked({
        let p = poll_rate_row.clone();
        move |_| p.set_value(16.0)
    });
    poll_rate_row.add_suffix(&btn_reset_poll_rate);

    let scroll_step_row = adw::SpinRow::with_range(1.0, 10.0, 1.0);
    scroll_step_row.set_title("Scroll Step Multiplier");
    scroll_step_row.set_subtitle("Lines scrolled per tick when scrolling feeds or web pages");
    scroll_step_row.set_value(config.borrow().mouse.scroll_step as f64);

    let btn_reset_scroll = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text("Reset to recommended default (2.0)")
        .build();
    btn_reset_scroll.connect_clicked({
        let s = scroll_step_row.clone();
        move |_| s.set_value(2.0)
    });
    scroll_step_row.add_suffix(&btn_reset_scroll);

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

    let models = ["tiny.en", "base.en", "small.en", "medium.en", "large-v3"];
    let current_model = config.borrow().voice.model.clone();
    let model_idx = models.iter().position(|&m| m == current_model).unwrap_or(3);

    let model_row = adw::ComboRow::builder()
        .title("Whisper Model")
        .subtitle("medium.en recommended (~0.6s latency on RTX GPU, exceptional accuracy)")
        .model(&gtk::StringList::new(&models))
        .selected(model_idx as u32)
        .build();

    let devices = ["cuda", "cpu"];
    let current_device = config.borrow().voice.device.clone();
    let device_idx = devices.iter().position(|&d| d == current_device).unwrap_or(0);

    let device_row = adw::ComboRow::builder()
        .title("Compute Device")
        .subtitle("CUDA utilizes NVIDIA GPU Tensor Cores for instantaneous processing")
        .model(&gtk::StringList::new(&devices))
        .selected(device_idx as u32)
        .build();

    let compute_types = ["float16", "int8", "int8_float16", "float32"];
    let current_ct = config.borrow().voice.compute_type.clone();
    let ct_idx = compute_types.iter().position(|&c| c == current_ct).unwrap_or(0);

    let compute_type_row = adw::ComboRow::builder()
        .title("Compute Precision")
        .subtitle("float16 is optimized for modern RTX graphics cards")
        .model(&gtk::StringList::new(&compute_types))
        .selected(ct_idx as u32)
        .build();

    let paste_methods = ["keystrokes", "clipboard"];
    let current_paste = config.borrow().voice.paste_method.clone();
    let paste_idx = paste_methods.iter().position(|&p| p == current_paste).unwrap_or(0);

    let paste_row = adw::ComboRow::builder()
        .title("Text Insertion Method")
        .subtitle("Keystrokes: direct virtual uinput typing | Clipboard: wl-copy + Ctrl+V")
        .model(&gtk::StringList::new(&paste_methods))
        .selected(paste_idx as u32)
        .build();

    let spacing_row = adw::SwitchRow::builder()
        .title("Auto-Spacing")
        .subtitle("Appends trailing space so multiple dictations flow smoothly")
        .active(config.borrow().voice.auto_spacing)
        .build();

    let min_dur_row = adw::SpinRow::with_range(0.1, 1.0, 0.05);
    min_dur_row.set_title("Minimum Voice Duration (sec)");
    min_dur_row.set_subtitle("Short audio bursts below this threshold are discarded");
    min_dur_row.set_value(config.borrow().voice.min_duration_sec);

    let btn_reset_min_dur = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .tooltip_text("Reset to recommended default (0.35s)")
        .build();
    btn_reset_min_dur.connect_clicked({
        let md = min_dur_row.clone();
        move |_| md.set_value(0.35)
    });
    min_dur_row.add_suffix(&btn_reset_min_dur);

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

    let group_buttons_main = adw::PreferencesGroup::builder()
        .title("Remote Button Bindings")
        .description("Click any button on the simulated remote to edit its actions")
        .build();

    // Mode Switcher for Buttons tab
    let mode_stack = gtk::Stack::new();
    mode_stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);

    let mode_seg_switcher = gtk::StackSwitcher::new();
    mode_seg_switcher.set_stack(Some(&mode_stack));
    mode_seg_switcher.set_css_classes(&["linked"]);
    mode_seg_switcher.set_halign(gtk::Align::Center);
    group_buttons_main.add(&mode_seg_switcher);

    // Live Event Status Banner
    let event_banner = gtk::Label::new(Some("Press physical remote buttons to test live input"));
    event_banner.set_css_classes(&["dim-label"]);
    event_banner.set_halign(gtk::Align::Center);
    group_buttons_main.add(&event_banner);

    // Center layout: Simulated Remote in Middle
    let remote_container = gtk::Box::new(gtk::Orientation::Vertical, 10);
    remote_container.set_css_classes(&["remote-body"]);
    remote_container.set_valign(gtk::Align::Start);
    remote_container.set_halign(gtk::Align::Center);

    let mut button_widgets: HashMap<String, gtk::Button> = HashMap::new();

    // Row: Power & TV
    let row_top_power = gtk::Box::new(gtk::Orientation::Horizontal, 28);
    row_top_power.set_halign(gtk::Align::Center);

    let btn_power = gtk::Button::with_label("⏻");
    btn_power.set_css_classes(&["remote-btn"]);
    btn_power.set_tooltip_text(Some("Power Button (KEY_SCREENLOCK)"));
    button_widgets.insert("KEY_SCREENLOCK".into(), btn_power.clone());
    button_widgets.insert("KEY_COFFEE".into(), btn_power.clone());

    let btn_tv = gtk::Button::with_label("TV");
    btn_tv.set_css_classes(&["remote-btn"]);
    btn_tv.set_tooltip_text(Some("Input / Mode Toggle (KEY_TV)"));
    button_widgets.insert("KEY_TV".into(), btn_tv.clone());

    row_top_power.append(&btn_power);
    row_top_power.append(&btn_tv);
    remote_container.append(&row_top_power);

    // D-Pad Circular Grid
    let dpad_grid = gtk::Grid::new();
    dpad_grid.set_css_classes(&["remote-dpad-ring"]);
    dpad_grid.set_halign(gtk::Align::Center);
    dpad_grid.set_row_spacing(2);
    dpad_grid.set_column_spacing(2);

    let btn_up = gtk::Button::with_label("▲");
    btn_up.set_css_classes(&["remote-btn"]);
    btn_up.set_tooltip_text(Some("D-Pad Up (KEY_UP)"));
    button_widgets.insert("KEY_UP".into(), btn_up.clone());

    let btn_down = gtk::Button::with_label("▼");
    btn_down.set_css_classes(&["remote-btn"]);
    btn_down.set_tooltip_text(Some("D-Pad Down (KEY_DOWN)"));
    button_widgets.insert("KEY_DOWN".into(), btn_down.clone());

    let btn_left = gtk::Button::with_label("◀");
    btn_left.set_css_classes(&["remote-btn"]);
    btn_left.set_tooltip_text(Some("D-Pad Left (KEY_LEFT)"));
    button_widgets.insert("KEY_LEFT".into(), btn_left.clone());

    let btn_right = gtk::Button::with_label("▶");
    btn_right.set_css_classes(&["remote-btn"]);
    btn_right.set_tooltip_text(Some("D-Pad Right (KEY_RIGHT)"));
    button_widgets.insert("KEY_RIGHT".into(), btn_right.clone());

    let btn_center = gtk::Button::with_label("OK");
    btn_center.set_css_classes(&["remote-btn-center"]);
    btn_center.set_tooltip_text(Some("Center Select (KEY_SELECT)"));
    button_widgets.insert("KEY_SELECT".into(), btn_center.clone());

    dpad_grid.attach(&btn_up, 1, 0, 1, 1);
    dpad_grid.attach(&btn_left, 0, 1, 1, 1);
    dpad_grid.attach(&btn_center, 1, 1, 1, 1);
    dpad_grid.attach(&btn_right, 2, 1, 1, 1);
    dpad_grid.attach(&btn_down, 1, 2, 1, 1);
    remote_container.append(&dpad_grid);

    // Row: Back & Home
    let row_nav = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    row_nav.set_halign(gtk::Align::Center);

    let btn_back = gtk::Button::with_label("⮌");
    btn_back.set_css_classes(&["remote-btn"]);
    btn_back.set_tooltip_text(Some("Back Button (KEY_BACK)"));
    button_widgets.insert("KEY_BACK".into(), btn_back.clone());

    let btn_home = gtk::Button::with_label("⌂");
    btn_home.set_css_classes(&["remote-btn"]);
    btn_home.set_tooltip_text(Some("Home / Overview (KEY_HOMEPAGE)"));
    button_widgets.insert("KEY_HOMEPAGE".into(), btn_home.clone());

    row_nav.append(&btn_back);
    row_nav.append(&btn_home);
    remote_container.append(&row_nav);

    // Row: Voice & Mute
    let row_voice_mute = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    row_voice_mute.set_halign(gtk::Align::Center);

    let btn_voice = gtk::Button::with_label("🎙");
    btn_voice.set_css_classes(&["remote-btn"]);
    btn_voice.set_tooltip_text(Some("Google Assistant / Voice (KEY_ASSISTANT)"));
    button_widgets.insert("KEY_ASSISTANT".into(), btn_voice.clone());
    button_widgets.insert("KEY_VOICECOMMAND".into(), btn_voice.clone());

    let btn_mute = gtk::Button::with_label("🔇");
    btn_mute.set_css_classes(&["remote-btn"]);
    btn_mute.set_tooltip_text(Some("Mute Button (KEY_MUTE)"));
    button_widgets.insert("KEY_MUTE".into(), btn_mute.clone());

    row_voice_mute.append(&btn_voice);
    row_voice_mute.append(&btn_mute);
    remote_container.append(&row_voice_mute);

    // Row: YouTube & Netflix
    let row_apps = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    row_apps.set_halign(gtk::Align::Center);

    let btn_yt = gtk::Button::with_label("YouTube");
    btn_yt.set_css_classes(&["remote-btn-pill"]);
    btn_yt.set_tooltip_text(Some("YouTube Button (KEY_KBDILLUMUP)"));
    button_widgets.insert("KEY_KBDILLUMUP".into(), btn_yt.clone());
    button_widgets.insert("KEY_CAMERA_ACCESS_DISABLE".into(), btn_yt.clone());

    let btn_nf = gtk::Button::with_label("Netflix");
    btn_nf.set_css_classes(&["remote-btn-pill"]);
    btn_nf.set_tooltip_text(Some("Netflix Button (KEY_CAMERA_ACCESS_TOGGLE)"));
    button_widgets.insert("KEY_CAMERA_ACCESS_TOGGLE".into(), btn_nf.clone());

    row_apps.append(&btn_yt);
    row_apps.append(&btn_nf);
    remote_container.append(&row_apps);

    // Row: Volume
    let row_vol = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row_vol.set_halign(gtk::Align::Center);

    let btn_vol_up = gtk::Button::with_label("Vol +");
    btn_vol_up.set_css_classes(&["remote-btn-pill"]);
    btn_vol_up.set_tooltip_text(Some("Volume Up (KEY_VOLUMEUP)"));
    button_widgets.insert("KEY_VOLUMEUP".into(), btn_vol_up.clone());

    let btn_vol_down = gtk::Button::with_label("Vol -");
    btn_vol_down.set_css_classes(&["remote-btn-pill"]);
    btn_vol_down.set_tooltip_text(Some("Volume Down (KEY_VOLUMEDOWN)"));
    button_widgets.insert("KEY_VOLUMEDOWN".into(), btn_vol_down.clone());

    row_vol.append(&btn_vol_up);
    row_vol.append(&btn_vol_down);
    remote_container.append(&row_vol);

    group_buttons_main.add(&remote_container);

    // Subtitle caption under remote
    let click_caption = gtk::Label::new(Some("Click any remote button above to customize its action"));
    click_caption.set_css_classes(&["dim-label"]);
    click_caption.set_halign(gtk::Align::Center);
    group_buttons_main.add(&click_caption);

    page_buttons.add(&group_buttons_main);

    // Function to open direct edit dialog for a specific button
    fn open_button_edit_dialog(
        parent_win: &adw::ApplicationWindow,
        btn_def: ButtonOption,
        mode_name: &'static str,
        cfg_ref: Rc<RefCell<AppConfig>>,
        toast_overlay_c: adw::ToastOverlay,
    ) {
        let dialog = adw::PreferencesDialog::builder()
            .title(format!("{} ({} Mode)", btn_def.name, if mode_name == "mouse" { "Mouse" } else { "Media" }))
            .build();

        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::builder()
            .title("Assign Action")
            .description("Choose what action triggers when pressing this button")
            .build();

        let trigger_labels = ["Tap Action", "Long-Press / Hold Action", "Double-Tap Action"];
        let combo_trigger = adw::ComboRow::builder()
            .title("Trigger Event")
            .model(&gtk::StringList::new(&trigger_labels))
            .build();

        let mut preset_names: Vec<&str> = ACTION_PRESETS.iter().map(|a| a.name).collect();
        preset_names.push("Custom Shell Command / Action String");

        let combo_preset = adw::ComboRow::builder()
            .title("Action Preset")
            .model(&gtk::StringList::new(&preset_names))
            .build();

        let entry_raw = adw::EntryRow::builder()
            .title("Action String")
            .text(ACTION_PRESETS[0].action_val)
            .build();

        combo_preset.connect_selected_notify({
            let entry_c = entry_raw.clone();
            move |c| {
                let idx = c.selected() as usize;
                if idx < ACTION_PRESETS.len() {
                    entry_c.set_text(ACTION_PRESETS[idx].action_val);
                }
            }
        });

        let btn_save_action = gtk::Button::builder()
            .label("Apply")
            .css_classes(["suggested-action"])
            .valign(gtk::Align::Center)
            .build();

        let apply_row = adw::ActionRow::builder()
            .title("Save Action for this Button")
            .build();
        apply_row.add_suffix(&btn_save_action);

        let dialog_weak = dialog.downgrade();
        btn_save_action.connect_clicked({
            let cfg_clone = cfg_ref.clone();
            let trig_sel = combo_trigger.clone();
            let entry_val = entry_raw.clone();
            let t_overlay = toast_overlay_c.clone();
            let btn_title = btn_def.name;
            let key = btn_def.key_code;
            move |_| {
                let action_val = entry_val.text().to_string();
                let trigger_idx = trig_sel.selected();

                let mut cfg = cfg_clone.borrow_mut();
                if let Some(mode_table) = cfg.mode.get_mut(mode_name).and_then(|v| v.as_table_mut()) {
                    if trigger_idx == 0 {
                        if let Some(existing) = mode_table.get_mut(key) {
                            if let Some(tab) = existing.as_table_mut() {
                                tab.insert("tap".into(), toml::Value::String(action_val.clone()));
                            } else {
                                *existing = toml::Value::String(action_val.clone());
                            }
                        } else {
                            mode_table.insert(key.into(), toml::Value::String(action_val.clone()));
                        }
                    } else if trigger_idx == 1 {
                        if let Some(existing) = mode_table.get_mut(key) {
                            if let Some(tab) = existing.as_table_mut() {
                                tab.insert("long_press".into(), toml::Value::String(action_val.clone()));
                            } else {
                                let old_tap = existing.as_str().unwrap_or("").to_string();
                                let mut tab = toml::Table::new();
                                if !old_tap.is_empty() {
                                    tab.insert("tap".into(), toml::Value::String(old_tap));
                                }
                                tab.insert("long_press".into(), toml::Value::String(action_val.clone()));
                                *existing = toml::Value::Table(tab);
                            }
                        } else {
                            let mut tab = toml::Table::new();
                            tab.insert("long_press".into(), toml::Value::String(action_val.clone()));
                            mode_table.insert(key.into(), toml::Value::Table(tab));
                        }
                    } else if trigger_idx == 2 {
                        if let Some(existing) = mode_table.get_mut(key) {
                            if let Some(tab) = existing.as_table_mut() {
                                tab.insert("double_tap".into(), toml::Value::String(action_val.clone()));
                            } else {
                                let old_tap = existing.as_str().unwrap_or("").to_string();
                                let mut tab = toml::Table::new();
                                if !old_tap.is_empty() {
                                    tab.insert("tap".into(), toml::Value::String(old_tap));
                                }
                                tab.insert("double_tap".into(), toml::Value::String(action_val.clone()));
                                *existing = toml::Value::Table(tab);
                            }
                        } else {
                            let mut tab = toml::Table::new();
                            tab.insert("double_tap".into(), toml::Value::String(action_val.clone()));
                            mode_table.insert(key.into(), toml::Value::Table(tab));
                        }
                    }

                    let toast = adw::Toast::new(&format!("Assigned '{}' to {}", action_val, btn_title));
                    toast.set_timeout(2);
                    t_overlay.add_toast(toast);

                    if let Some(d) = dialog_weak.upgrade() {
                        d.close();
                    }
                }
            }
        });

        group.add(&combo_trigger);
        group.add(&combo_preset);
        group.add(&entry_raw);
        group.add(&apply_row);
        page.add(&group);
        dialog.add(&page);

        dialog.present(Some(parent_win));
    }

    // Connect remote button clicks directly to dialog
    let wire_btn_dialog = |btn: &gtk::Button, btn_idx: usize| {
        let p_win = window.clone();
        let cfg_c = config.clone();
        let m_stack_c = mode_stack.clone();
        let t_overlay = toast_overlay.clone();

        btn.connect_clicked(move |_| {
            let mode = if m_stack_c.visible_child_name().as_deref() == Some("mouse") {
                "mouse"
            } else {
                "media"
            };
            open_button_edit_dialog(
                &p_win,
                CONFIGURABLE_BUTTONS[btn_idx],
                mode,
                cfg_c.clone(),
                t_overlay.clone(),
            );
        });
    };

    wire_btn_dialog(&btn_center, 0);
    wire_btn_dialog(&btn_up, 1);
    wire_btn_dialog(&btn_down, 2);
    wire_btn_dialog(&btn_left, 3);
    wire_btn_dialog(&btn_right, 4);
    wire_btn_dialog(&btn_back, 5);
    wire_btn_dialog(&btn_home, 6);
    wire_btn_dialog(&btn_mute, 7);
    wire_btn_dialog(&btn_yt, 8);
    wire_btn_dialog(&btn_nf, 9);
    wire_btn_dialog(&btn_power, 10);
    wire_btn_dialog(&btn_tv, 11);
    wire_btn_dialog(&btn_vol_up, 12);
    wire_btn_dialog(&btn_vol_down, 13);

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

    // Handle sidebar selection
    let c_stack = content_stack.clone();
    let c_page = content_page.clone();
    nav_list.connect_row_selected(move |_, row| {
        if let Some(r) = row {
            let idx = r.index();
            let (target_id, title) = match idx {
                0 => ("general", "General"),
                1 => ("mouse", "Mouse"),
                2 => ("voice", "Voice"),
                3 => ("buttons", "Buttons"),
                _ => ("general", "General"),
            };
            c_stack.set_visible_child_name(target_id);
            c_page.set_title(title);
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

    glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
        while let Ok((key_name, state)) = receiver.try_recv() {
            let is_pressed = state == 1 || state == 2;
            let state_label = if is_pressed { "Pressed" } else { "Released" };
            event_banner_rc.set_text(&format!(
                "Last Remote Event: {} ({})",
                key_name, state_label
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

    // Clean up socket on window destroy
    window.connect_destroy({
        let is_running_cleanup = is_running.clone();
        move |_| {
            is_running_cleanup.store(false, Ordering::Relaxed);
            let sock_path = get_ui_socket_path();
            let _ = fs::remove_file(sock_path);
        }
    });

    // ==========================================
    // SAVE HANDLER
    // ==========================================
    let overlay_weak = toast_overlay.downgrade();
    let config_clone = config.clone();
    let u_remote_save = update_remote.clone();
    let u_voice_save = update_voice.clone();
    let u_atv_save = update_atv.clone();

    save_btn.connect_clicked(move |_| {
        let Some(overlay) = overlay_weak.upgrade() else { return };

        let mut cfg = config_clone.borrow_mut();

        // Device
        cfg.device.name_pattern = pattern_row.text().to_string();
        cfg.device.grab_device = grab_row.is_active();
        cfg.device.reconnect_poll_interval = poll_row.value();

        // General
        cfg.general.initial_mode = if initial_mode_row.selected() == 1 {
            "mouse".into()
        } else {
            "media".into()
        };
        cfg.general.long_press_threshold_sec = long_press_row.value();
        cfg.general.notifications = notif_row.is_active();
        cfg.general.sound_feedback = sound_row.is_active();

        // Mouse
        cfg.mouse.base_speed = base_speed_row.value();
        cfg.mouse.max_speed = max_speed_row.value();
        cfg.mouse.acceleration = accel_row.value();
        cfg.mouse.poll_rate_ms = poll_rate_row.value() as u64;
        cfg.mouse.scroll_step = scroll_step_row.value() as i64;

        // Voice
        cfg.voice.enabled = voice_enable_row.is_active();
        cfg.voice.model = models[model_row.selected() as usize].to_string();
        cfg.voice.device = devices[device_row.selected() as usize].to_string();
        cfg.voice.compute_type = compute_types[compute_type_row.selected() as usize].to_string();
        cfg.voice.paste_method = paste_methods[paste_row.selected() as usize].to_string();
        cfg.voice.auto_spacing = spacing_row.is_active();
        cfg.voice.min_duration_sec = min_dur_row.value();

        match save_config(&cfg) {
            Ok(_) => {
                restart_all_services();
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

    window.present();
}
