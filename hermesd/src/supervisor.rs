use crate::logs::LogBuffer;
use crate::manifest::{RestartPolicy, ServiceSpec};
use crate::rss::{is_alive, read_tree_rss_kb};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ServiceStatus {
    Stopped,
    Running,
    Failed,
    Starting,
}

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ServiceSnapshot {
    pub stack: String,
    pub name: String,
    pub status: ServiceStatus,
    pub pid: Option<u32>,
    pub restarts: u32,
    pub rss_kb: Option<u64>,
    pub uptime_secs: Option<u64>,
    pub last_exit: Option<i32>,
    pub max_rss_mb: Option<u64>,
}

pub struct ServiceState {
    pub status: ServiceStatus,
    pub child: Option<Child>,
    pub pid: Option<u32>,
    pub started_at: Option<Instant>,
    pub restarts: u32,
    pub last_exit: Option<i32>,
    pub backoff_until: Option<Instant>,
    pub oom_killed: bool,
    pub pending_restart: bool,
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            status: ServiceStatus::Stopped,
            child: None,
            pid: None,
            started_at: None,
            restarts: 0,
            last_exit: None,
            backoff_until: None,
            oom_killed: false,
            pending_restart: false,
        }
    }
}

pub struct ServiceHandle {
    pub stack: String,
    pub spec: ServiceSpec,
    pub log: LogBuffer,
    pub state: Arc<Mutex<ServiceState>>,
    pub native_dir: Option<String>,
    pub files_dir: Option<String>,
    pub rootfs: Option<String>,
}

impl ServiceHandle {
    pub fn id(&self) -> String {
        format!("{}/{}", self.stack, self.spec.name)
    }

    pub fn should_restart(policy: RestartPolicy, exit_code: Option<i32>) -> bool {
        match policy {
            RestartPolicy::No => false,
            RestartPolicy::Always => true,
            RestartPolicy::OnFailure => !matches!(exit_code, Some(0)),
        }
    }

    pub fn backoff_delay(restarts: u32) -> Duration {
        let secs = 1u64 << restarts.min(6);
        Duration::from_secs(secs.min(60))
    }

    fn build_command(&self) -> Command {
        let cwd = self.spec.cwd.clone().unwrap_or_else(|| "/".to_string());
        if let (Some(native), Some(files), Some(rootfs)) = (&self.native_dir, &self.files_dir, &self.rootfs) {
            // prepare dirs + symlink (I/O side effect)
            let tmp_dir = format!("{}/tmp", files);
            let lib_dir = format!("{}/lib", files);
            let _ = std::fs::create_dir_all(&tmp_dir);
            let _ = std::fs::create_dir_all(&lib_dir);
            let link = format!("{}/libtalloc.so.2", lib_dir);
            let target = format!("{}/libtalloc.so", native);
            let _ = std::fs::remove_file(&link);
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&target, &link);

            let inv = crate::proot::build_proot_command(native, files, rootfs, &self.spec.cmd, &cwd);
            let mut cmd = Command::new(&inv.program);
            cmd.args(&inv.args);
            for (k, v) in &inv.env {
                cmd.env(k, v);
            }
            for (k, v) in &self.spec.env {
                cmd.env(k, v);
            }
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd
        } else {
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-c").arg(&self.spec.cmd);
            if let Some(cwd) = &self.spec.cwd {
                cmd.current_dir(cwd);
            }
            for (k, v) in &self.spec.env {
                cmd.env(k, v);
            }
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        if st.status == ServiceStatus::Running {
            return Ok(());
        }
        // backoff check
        if let Some(until) = st.backoff_until {
            if Instant::now() < until {
                return Err("backoff".into());
            }
        }
        st.status = ServiceStatus::Starting;
        drop(st);

        let mut cmd = self.build_command();
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        // stream stdout/stderr
        if let Some(out) = child.stdout.take() {
            let log = self.log.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines().flatten() {
                    log.push(line);
                }
            });
        }
        if let Some(err) = child.stderr.take() {
            let log = self.log.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(err);
                for line in reader.lines().flatten() {
                    log.push(format!("[stderr] {}", line));
                }
            });
        }

        {
            let mut st = self.state.lock().unwrap();
            st.status = ServiceStatus::Running;
            st.child = Some(child);
            st.pid = Some(pid);
            st.started_at = Some(Instant::now());
            st.oom_killed = false;
        }

        // wait thread
        let mut handle = self.clone_for_wait();
        std::thread::spawn(move || handle.wait_loop());
        Ok(())
    }

    fn clone_for_wait(&self) -> WaitContext {
        WaitContext {
            stack: self.stack.clone(),
            name: self.spec.name.clone(),
            policy: self.spec.restart.clone(),
            max_rss_mb: self.spec.max_rss_mb,
            state: self.state.clone(),
            log: self.log.clone(),
            started_at: None,
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        if let Some(child) = st.child.take() {
            // kill
            let pid = child.id();
            // kill process group? just child
            drop(child); // dropping doesn't kill; need kill
            // We'll kill via stored child; but we took ownership. Re-open? Use pid kill.
            let _ = unsafe { libc_kill(pid) };
            st.status = ServiceStatus::Stopped;
            st.pid = None;
            st.started_at = None;
            st.backoff_until = None;
            st.pending_restart = false;
        } else {
            st.status = ServiceStatus::Stopped;
        }
        Ok(())
    }

    pub fn restart(&self) -> Result<(), String> {
        let _ = self.stop();
        // small delay
        std::thread::sleep(Duration::from_millis(200));
        self.start()
    }

    pub fn snapshot(&self) -> ServiceSnapshot {
        let st = self.state.lock().unwrap();
        let rss = st.pid.map(|p| read_tree_rss_kb(p));
        let uptime = st.started_at.map(|s| s.elapsed().as_secs());
        ServiceSnapshot {
            stack: self.stack.clone(),
            name: self.spec.name.clone(),
            status: st.status,
            pid: st.pid,
            restarts: st.restarts,
            rss_kb: rss,
            uptime_secs: uptime,
            last_exit: st.last_exit,
            max_rss_mb: self.spec.max_rss_mb,
        }
    }

    pub fn check_rss_and_kill(&self) {
        let (pid, max) = {
            let st = self.state.lock().unwrap();
            if st.status != ServiceStatus::Running {
                return;
            }
            (st.pid, self.spec.max_rss_mb)
        };
        if let (Some(pid), Some(max)) = (pid, max) {
            let rss_kb = read_tree_rss_kb(pid);
            if rss_kb > max * 1024 {
                // kill
                let _ = unsafe { libc_kill(pid) };
                let mut st = self.state.lock().unwrap();
                st.oom_killed = true;
                st.status = ServiceStatus::Failed;
                st.last_exit = Some(-9);
                self.log.push(format!(
                    "[hermesd] killed by RSS limit: {} KB > {} MB",
                    rss_kb, max
                ));
            }
        }
    }
}

struct WaitContext {
    stack: String,
    name: String,
    policy: RestartPolicy,
    max_rss_mb: Option<u64>,
    state: Arc<Mutex<ServiceState>>,
    log: LogBuffer,
    started_at: Option<Instant>,
}

impl WaitContext {
    fn wait_loop(&mut self) {
        // We need access to child; take it
        // The child is in state; we'll wait by polling? Simpler: take child out, wait.
        // But state holds child. Let's extract.
        let mut child_opt = {
            let mut st = self.state.lock().unwrap();
            st.child.take()
        };
        if let Some(mut child) = child_opt.take() {
            self.started_at = Some(Instant::now());
            let status = child.wait();
            let code = status.ok().and_then(|s| s.code());
            let ran_for = self.started_at.unwrap().elapsed();
            // update state
            {
                let mut st = self.state.lock().unwrap();
                st.child = None;
                st.pid = None;
                st.last_exit = code;
                if code == Some(0) && st.status != ServiceStatus::Stopped {
                    // completed normally
                }
            }
            let should = ServiceHandle::should_restart(self.policy.clone(), code);
            let stopped = {
                let st = self.state.lock().unwrap();
                st.status == ServiceStatus::Stopped
            };
            if stopped {
                return;
            }
            if should {
                let restarts = {
                    let mut st = self.state.lock().unwrap();
                    if ran_for.as_secs() > 30 {
                        st.restarts = 0;
                    }
                    st.restarts += 1;
                    st.restarts
                };
                let delay = ServiceHandle::backoff_delay(restarts.saturating_sub(1));
                self.log.push(format!(
                    "[hermesd] service exited (code {:?}); will restart in {:?}",
                    code, delay
                ));
                let mut st = self.state.lock().unwrap();
                st.status = ServiceStatus::Failed;
                st.backoff_until = Some(Instant::now() + delay);
                st.pending_restart = true;
            } else {
                let mut st = self.state.lock().unwrap();
                st.status = ServiceStatus::Stopped;
                st.pending_restart = false;
            }
        }
    }
}

// Manager
pub struct ServiceManager {
    pub native_dir: Option<String>,
    pub files_dir: Option<String>,
    pub rootfs: Option<String>,
    pub services: HashMap<String, Arc<ServiceHandle>>,
}

impl ServiceManager {
    pub fn new(native_dir: Option<String>, files_dir: Option<String>, rootfs: Option<String>) -> Self {
        Self {
            native_dir,
            files_dir,
            rootfs,
            services: HashMap::new(),
        }
    }

    pub fn add_service(&mut self, stack: &str, spec: ServiceSpec) {
        let log = LogBuffer::new(1000);
        let handle = Arc::new(ServiceHandle {
            stack: stack.to_string(),
            spec,
            log,
            state: Arc::new(Mutex::new(ServiceState::default())),
            native_dir: self.native_dir.clone(),
            files_dir: self.files_dir.clone(),
            rootfs: self.rootfs.clone(),
        });
        self.services.insert(handle.id(), handle);
    }

    pub fn load_manifest(&mut self, stack: &str, manifest: &crate::manifest::StackManifest) {
        for svc in &manifest.service {
            if svc.enabled {
                self.add_service(stack, svc.clone());
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<ServiceHandle>> {
        self.services.get(id).cloned()
    }

    pub fn list(&self) -> Vec<Arc<ServiceHandle>> {
        self.services.values().cloned().collect()
    }

    pub fn monitor_once(&self) {
        for h in self.services.values() {
            // RSS check
            h.check_rss_and_kill();
            // restart eligible
            let eligible = {
                let st = h.state.lock().unwrap();
                if !st.pending_restart {
                    false
                } else {
                    match st.backoff_until {
                        Some(until) => Instant::now() >= until,
                        None => true,
                    }
                }
            };
            if eligible {
                let mut st = h.state.lock().unwrap();
                st.pending_restart = false;
                st.backoff_until = None;
                drop(st);
                let _ = h.start();
            }
        }
    }

    pub fn start_monitor(self: std::sync::Arc<Self>) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || loop {
            self.monitor_once();
            std::thread::sleep(Duration::from_secs(2));
        })
    }

    pub fn start_all_autostart(&self, manifests: &[(String, crate::manifest::StackManifest)]) {
        for (stack, m) in manifests {
            if m.stack.autostart {
                for svc in &m.service {
                    if svc.enabled {
                        if let Some(h) = self.get(&format!("{}/{}", stack, svc.name)) {
                            let _ = h.start();
                        }
                    }
                }
            }
        }
    }
}

// libc kill helper
unsafe fn libc_kill(pid: u32) -> i32 {
    let _ = std::process::Command::new("kill")
        .arg("-9")
        .arg(format!("-{}", pid))
        .status();
    let _ = std::process::Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .status();
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{RestartPolicy, ServiceSpec};
    use std::collections::BTreeMap;

    fn dummy_spec(name: &str, cmd: &str, restart: RestartPolicy) -> ServiceSpec {
        ServiceSpec {
            name: name.to_string(),
            cmd: cmd.to_string(),
            cwd: None,
            restart,
            max_rss_mb: None,
            env: BTreeMap::new(),
            enabled: true,
        }
    }

    #[test]
    fn should_restart_policy() {
        assert!(!ServiceHandle::should_restart(RestartPolicy::No, Some(1)));
        assert!(ServiceHandle::should_restart(RestartPolicy::Always, Some(0)));
        assert!(ServiceHandle::should_restart(RestartPolicy::OnFailure, Some(1)));
        assert!(!ServiceHandle::should_restart(RestartPolicy::OnFailure, Some(0)));
    }

    #[test]
    fn backoff_increases() {
        assert_eq!(ServiceHandle::backoff_delay(0).as_secs(), 1);
        assert_eq!(ServiceHandle::backoff_delay(1).as_secs(), 2);
        assert_eq!(ServiceHandle::backoff_delay(2).as_secs(), 4);
        assert_eq!(ServiceHandle::backoff_delay(10).as_secs(), 60);
    }

    #[test]
    fn spawn_echo_service() {
        let mut mgr = ServiceManager::new(None, None, None);
        mgr.add_service("test", dummy_spec("echo", "echo hello", RestartPolicy::No));
        let h = mgr.get("test/echo").unwrap();
        h.start().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let snap = h.snapshot();
        // process may have exited already
        let logs = h.log.tail(10);
        assert!(logs.iter().any(|l| l.contains("hello")), "logs: {:?}", logs);
    }

    #[test]
    fn spawn_failing_service_no_restart() {
        let mut mgr = ServiceManager::new(None, None, None);
        mgr.add_service("test", dummy_spec("fail", "exit 42", RestartPolicy::No));
        let h = mgr.get("test/fail").unwrap();
        h.start().unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let snap = h.snapshot();
        // should not be running
        assert_ne!(snap.status, ServiceStatus::Running);
    }

    #[test]
    fn rss_kill_logic() {
        // just ensure read_tree_rss_kb works on current pid
        let rss = read_tree_rss_kb(std::process::id());
        assert!(rss > 0);
    }
}
