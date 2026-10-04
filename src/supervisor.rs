use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct WorkerSupervisor {
    remote_stop: Arc<AtomicBool>,
    remote_running: Arc<AtomicBool>,
    remote_child: Arc<Mutex<Option<Child>>>,
    voice_child: Arc<Mutex<Option<Child>>>,
    atv_child: Arc<Mutex<Option<Child>>>,
}

impl WorkerSupervisor {
    pub fn new() -> Self {
        Self {
            remote_stop: Arc::new(AtomicBool::new(false)),
            remote_running: Arc::new(AtomicBool::new(false)),
            remote_child: Arc::new(Mutex::new(None)),
            voice_child: Arc::new(Mutex::new(None)),
            atv_child: Arc::new(Mutex::new(None)),
        }
    }

    pub fn find_python() -> PathBuf {
        let home = glib_home_dir();
        let is_system_install = std::env::current_exe()
            .ok()
            .map(|p| p.starts_with("/usr") || p.starts_with("/app"))
            .unwrap_or(false);

        let mut candidates = Vec::new();
        if !is_system_install {
            candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/venv/bin/python3")));
            candidates.push(home.join(".local/share/pilot/venv/bin/python3"));
            candidates.push(home.join(".local/share/chromecast-remote/venv/bin/python3"));
            candidates.push(PathBuf::from("/app/bin/python3"));
            candidates.push(PathBuf::from("/usr/bin/python3"));
        } else {
            candidates.push(home.join(".local/share/pilot/venv/bin/python3"));
            candidates.push(home.join(".local/share/chromecast-remote/venv/bin/python3"));
            candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/venv/bin/python3")));
            candidates.push(PathBuf::from("/app/bin/python3"));
            candidates.push(PathBuf::from("/usr/bin/python3"));
        }

        for p in candidates {
            if p.exists() {
                return p;
            }
        }
        PathBuf::from("python3")
    }

    pub fn find_script(script_name: &str) -> Option<PathBuf> {
        let home = glib_home_dir();
        let is_system_install = std::env::current_exe()
            .ok()
            .map(|p| p.starts_with("/usr") || p.starts_with("/app"))
            .unwrap_or(false);

        let mut candidates = Vec::new();
        if !is_system_install {
            candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"))).join(script_name));
            if let Ok(cur_exe) = std::env::current_exe() {
                if let Some(p) = cur_exe.parent() {
                    candidates.push(p.join(script_name));
                    if let Some(pp) = p.parent() {
                        if let Some(ppp) = pp.parent() {
                            candidates.push(ppp.join(script_name));
                        }
                    }
                }
            }
            candidates.push(home.join(".local/share/pilot").join(script_name));
            candidates.push(PathBuf::from("/app/libexec/pilot").join(script_name));
            candidates.push(PathBuf::from("/usr/libexec/pilot").join(script_name));
        } else {
            candidates.push(home.join(".local/share/pilot").join(script_name));
            candidates.push(PathBuf::from("/app/libexec/pilot").join(script_name));
            candidates.push(PathBuf::from("/usr/libexec/pilot").join(script_name));
            candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"))).join(script_name));
        }
        candidates.push(home.join(".local/share/chromecast-remote").join(script_name));

        for p in candidates {
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    pub fn find_atvvoice() -> Option<PathBuf> {
        let home = glib_home_dir();
        let is_system_install = std::env::current_exe()
            .ok()
            .map(|p| p.starts_with("/usr") || p.starts_with("/app"))
            .unwrap_or(false);

        let mut candidates = Vec::new();
        if !is_system_install {
            candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/bin/atvvoice")));
            candidates.push(PathBuf::from("/app/bin/atvvoice"));
            candidates.push(PathBuf::from("/app/libexec/pilot/atvvoice"));
            candidates.push(PathBuf::from("/usr/libexec/pilot/atvvoice"));
            candidates.push(home.join(".local/bin/atvvoice"));
        } else {
            candidates.push(PathBuf::from("/app/bin/atvvoice"));
            candidates.push(PathBuf::from("/app/libexec/pilot/atvvoice"));
            candidates.push(PathBuf::from("/usr/libexec/pilot/atvvoice"));
            candidates.push(home.join(".local/bin/atvvoice"));
            candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/bin/atvvoice")));
        }
        candidates.push(home.join(".local/share/chromecast-remote/bin/atvvoice"));

        for p in candidates {
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    // --- Remote Controller Worker ---

    pub fn is_remote_running(&self) -> bool {
        if self.remote_running.load(Ordering::SeqCst) {
            return true;
        }
        let mut lock = self.remote_child.lock().unwrap();
        if check_child_running(&mut lock) {
            return true;
        }
        is_systemd_unit_active("pilot-remote.service")
            || is_systemd_unit_active("chromecast-remote.service")
    }

    pub fn start_remote(&self) -> bool {
        if self.remote_running.load(Ordering::SeqCst) {
            return true;
        }

        // Stop legacy systemd unit if active to avoid EVIOCGRAB conflict
        stop_systemd_unit_if_active(&["pilot-remote.service", "chromecast-remote.service"]);

        let cfg = match crate::load_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[Supervisor] Failed to load config for remote: {}", e);
                return false;
            }
        };

        self.remote_stop.store(false, Ordering::SeqCst);
        self.remote_running.store(true, Ordering::SeqCst);

        let stop_flag = self.remote_stop.clone();
        let running_flag = self.remote_running.clone();

        eprintln!("[Supervisor] Starting native Rust remote engine thread...");
        std::thread::spawn(move || {
            if let Err(e) = crate::remote::run_remote_controller(stop_flag, cfg) {
                eprintln!("[Supervisor] Remote controller error: {}", e);
            }
            running_flag.store(false, Ordering::SeqCst);
        });

        true
    }

    pub fn stop_remote(&self) {
        self.remote_stop.store(true, Ordering::SeqCst);
        self.remote_running.store(false, Ordering::SeqCst);

        let mut lock = self.remote_child.lock().unwrap();
        stop_child(&mut lock);
        stop_systemd_unit_if_active(&["pilot-remote.service", "chromecast-remote.service"]);
    }

    // --- Voice Dictation Worker ---

    pub fn is_voice_running(&self) -> bool {
        let mut lock = self.voice_child.lock().unwrap();
        if check_child_running(&mut lock) {
            return true;
        }
        is_systemd_unit_active("pilot-voice.service")
            || is_systemd_unit_active("chromecast-voice.service")
    }

    pub fn start_voice(&self) -> bool {
        let mut lock = self.voice_child.lock().unwrap();
        if check_child_running(&mut lock) {
            return true;
        }

        stop_systemd_unit_if_active(&["pilot-voice.service", "chromecast-voice.service"]);

        let python = Self::find_python();
        let script = match Self::find_script("voice_daemon.py") {
            Some(s) => s,
            None => {
                eprintln!("[Supervisor] voice_daemon.py not found");
                return false;
            }
        };

        eprintln!("[Supervisor] Spawning voice daemon: {:?} {:?}", python, script);
        match Command::new(&python)
            .arg(&script)
            .env("PYTHONUNBUFFERED", "1")
            .spawn()
        {
            Ok(child) => {
                *lock = Some(child);
                true
            }
            Err(e) => {
                eprintln!("[Supervisor] Failed to spawn voice daemon: {}", e);
                false
            }
        }
    }

    pub fn stop_voice(&self) {
        let mut lock = self.voice_child.lock().unwrap();
        stop_child(&mut lock);
        stop_systemd_unit_if_active(&["pilot-voice.service", "chromecast-voice.service"]);
    }

    // --- ATVVoice BLE Audio Worker ---

    pub fn is_atv_running(&self) -> bool {
        let mut lock = self.atv_child.lock().unwrap();
        if check_child_running(&mut lock) {
            return true;
        }
        is_systemd_unit_active("atvvoice.service")
    }

    pub fn start_atv(&self) -> bool {
        let mut lock = self.atv_child.lock().unwrap();
        if check_child_running(&mut lock) {
            return true;
        }

        stop_systemd_unit_if_active(&["atvvoice.service"]);

        let bin = match Self::find_atvvoice() {
            Some(b) => b,
            None => {
                eprintln!("[Supervisor] atvvoice binary not found");
                return false;
            }
        };

        let mac_opt = crate::remote::discover_chromecast_mac();
        let mut cmd = Command::new(&bin);
        if let Some(ref mac) = mac_opt {
            eprintln!("[Supervisor] Discovered Chromecast remote MAC: {}", mac);
            cmd.args(["-d", mac, "--frame-timeout", "0", "-g", "10"]);
        } else {
            cmd.args(["--frame-timeout", "0", "-g", "10"]);
        }

        eprintln!("[Supervisor] Spawning atvvoice: {:?}", bin);
        match cmd.spawn() {
            Ok(child) => {
                *lock = Some(child);
                true
            }
            Err(e) => {
                eprintln!("[Supervisor] Failed to spawn atvvoice: {}", e);
                false
            }
        }
    }

    pub fn stop_atv(&self) {
        let mut lock = self.atv_child.lock().unwrap();
        stop_child(&mut lock);
        stop_systemd_unit_if_active(&["atvvoice.service"]);
    }

    // --- Global Controls ---

    pub fn start_all(&self, remote_enabled: bool, voice_enabled: bool) {
        if remote_enabled {
            self.start_remote();
        }
        if voice_enabled {
            self.start_voice();
            self.start_atv();
        }
    }

    pub fn stop_all(&self) {
        eprintln!("[Supervisor] Shutting down all Pilot workers...");
        self.stop_remote();
        self.stop_voice();
        self.stop_atv();
    }

    pub fn restart_all(&self, remote_enabled: bool, voice_enabled: bool) {
        self.stop_all();
        std::thread::sleep(Duration::from_millis(200));
        self.start_all(remote_enabled, voice_enabled);
    }
}

impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        self.stop_all();
    }
}

// --- Internal Helpers ---

fn glib_home_dir() -> PathBuf {
    gtk::glib::home_dir()
}

fn check_child_running(child_opt: &mut Option<Child>) -> bool {
    if let Some(ref mut child) = child_opt {
        match child.try_wait() {
            Ok(Some(_status)) => {
                *child_opt = None;
                false
            }
            Ok(None) => true,
            Err(_) => {
                *child_opt = None;
                false
            }
        }
    } else {
        false
    }
}

fn stop_child(child_opt: &mut Option<Child>) {
    if let Some(mut child) = child_opt.take() {
        let pid = child.id();
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        std::thread::sleep(Duration::from_millis(120));
        if let Ok(None) = child.try_wait() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

fn is_systemd_unit_active(unit_name: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit_name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn stop_systemd_unit_if_active(units: &[&str]) {
    for unit in units {
        if is_systemd_unit_active(unit) {
            let _ = Command::new("systemctl")
                .args(["--user", "stop", unit])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}
