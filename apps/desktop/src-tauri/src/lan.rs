//! Supervise the trusted, locally installed diagnostic executable.
//! This interface does not accept arbitrary executable paths, commands,
//! environment variables, file transfers, capture requests or system input.
use serde::Serialize;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tauri::{State, WebviewWindow};

const SESSION_LIMIT: Duration = Duration::from_secs(180);
const MAX_LINE: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Stage { Idle, Starting, AwaitingApproval, Connecting, Listening, Connected, Stopping, Passed, Failed, Canceled }
impl Stage {
    fn active(self) -> bool {
        matches!(self, Self::Starting | Self::AwaitingApproval | Self::Connecting | Self::Listening | Self::Connected | Self::Stopping)
    }
}

#[derive(Clone, Serialize)]
pub struct Snapshot {
    stage: Stage,
    active: bool,
    local_key: Option<String>,
    detail: Option<String>,
    profile: Profile,
    input_released: bool,
    device_id: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self { stage: Stage::Idle, active: false, local_key: None, detail: None, profile: Profile::Diagnostic, input_released: false, device_id: None }
    }
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}
fn stage(shared: &Mutex<Snapshot>, next: Stage, detail: Option<String>) {
    let mut state = locked(shared);
    state.stage = next;
    state.active = next.active();
    state.detail = detail;
    if !state.active { state.local_key = None; }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Profile { Diagnostic, SecureTest, Screen, Control, RoutedTest, RoutedView, RoutedControl }
impl Profile {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "diagnostic" => Ok(Self::Diagnostic),
            "secure_test" => Ok(Self::SecureTest),
            "screen" => Ok(Self::Screen),
            "control" => Ok(Self::Control),
            _ => Err("Unknown session profile".into()),
        }
    }
    fn routed(self)->bool{matches!(self,Self::RoutedTest|Self::RoutedView|Self::RoutedControl)}
    fn requires_release(self)->bool{matches!(self,Self::Control|Self::RoutedControl|Self::RoutedView)}
    fn key_len(self) -> usize { if self == Self::Diagnostic || self.routed() { 64 } else { 128 } }
    fn consent(self, host: bool) -> &'static str {
        match self {
            Self::Diagnostic => "ALLOW DIAGNOSTIC",
            Self::SecureTest => "ALLOW TEST VIEW",
            Self::Screen if host => "SHARE SCREEN",
            Self::Screen => "ALLOW SCREEN VIEW",
            Self::Control if host => "ALLOW SCREEN AND INPUT",
            Self::Control => "ALLOW REMOTE INPUT",
            Self::RoutedTest => "ALLOW ROUTED TEST",
            Self::RoutedView if host=>"ALLOW ROUTED SCREEN",
            Self::RoutedView=>"ALLOW ROUTED VIEW",
            Self::RoutedControl if host=>"ALLOW ROUTED SCREEN AND INPUT",
            Self::RoutedControl=>"ALLOW ROUTED INPUT",
        }
    }
}
fn key_valid(value: &str, profile: Profile) -> bool {
    value.len() == profile.key_len() && value.bytes().all(|b| b.is_ascii_hexdigit())
        && value.as_bytes().chunks_exact(64).all(|key| key.iter().any(|b| *b != b'0'))
}
fn local_endpoint(value: &str) -> Result<SocketAddr, String> {
    let addr: SocketAddr = value.trim().parse().map_err(|_| "Enter an IP address and port".to_string())?;
    let allowed = match addr.ip() {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
        IpAddr::V6(ip) => ip.is_loopback(),
    };
    if !allowed || addr.port() == 0 {
        return Err("Use a specific private or loopback address and a nonzero port".into());
    }
    Ok(addr)
}

fn sidecar_path(profile: Profile) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = exe.parent().ok_or("Cannot find application directory")?;
    let name = match (profile, cfg!(windows)) {
        (Profile::Diagnostic, true) => "lan_diagnostic.exe",
        (Profile::Diagnostic, false) => "lan_diagnostic",
        (Profile::Control, true) => "lan_control.exe",
        (Profile::Control, false) => "lan_control",
        (Profile::RoutedTest,true)=>"routed_diagnostic.exe",
        (Profile::RoutedTest,false)=>"routed_diagnostic",
        (Profile::RoutedView|Profile::RoutedControl,true)=>"routed_control.exe",
        (Profile::RoutedView|Profile::RoutedControl,false)=>"routed_control",
        (_, true) => "lan_view.exe",
        (_, false) => "lan_view",
    };
    let path = parent.join(name);
    // Deliberately no PATH lookup, user-selected binary or shell.
    if !path.is_file() {
        return Err("Native media diagnostic is not installed next to the application executable. See the development setup instructions.".into());
    }
    Ok(path)
}

enum WorkerCommand { Approve(String) }
enum Output { Stdout(String), Stderr(String), Invalid }
struct Worker { tx: SyncSender<WorkerCommand>, stop: Arc<AtomicBool>, join: JoinHandle<()> }
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        // A direct child only, never a system-wide process kill.
        if !matches!(self.0.try_wait(), Ok(Some(_))) { let _ = self.0.kill(); }
        let _ = self.0.wait();
    }
}

fn reader<R: Read + Send + 'static>(
    input: R, sender: SyncSender<Output>, overflow: Arc<AtomicBool>, stderr: bool,
) {
    thread::spawn(move || {
        let mut reader = BufReader::new(input);
        loop {
            let mut line = Vec::new();
            let n = match reader.by_ref().take(MAX_LINE).read_until(b'\n', &mut line) {
                Ok(n) => n,
                Err(_) => { let _ = sender.try_send(Output::Invalid); return; }
            };
            if n == 0 { return; }
            if n as u64 == MAX_LINE && line.last() != Some(&b'\n') {
                overflow.store(true, Ordering::Release);
                return;
            }
            let line = String::from_utf8_lossy(&line).trim_end().to_owned();
            let message = if stderr { Output::Stderr(line) } else { Output::Stdout(line) };
            if sender.try_send(message).is_err() {
                overflow.store(true, Ordering::Release);
                return;
            }
        }
    });
}

fn handle_output(shared: &Mutex<Snapshot>, output: Output, pass: &mut bool, failure: &mut Option<String>) {
    match output {
        Output::Invalid => *failure = Some("Invalid diagnostic process output".into()),
        Output::Stderr(text) => {
            if let Some(reason) = text.strip_prefix("FAIL: ") {
                // Bound the error detail; Vue renders it as text, not HTML.
                *failure = Some(reason.chars().take(350).collect());
            }
        }
        Output::Stdout(text) => {
            if let Some(key) = text.strip_prefix("KEY ") {
                let mut snapshot = locked(shared);
                if snapshot.stage == Stage::Starting && key_valid(key, snapshot.profile) {
                    snapshot.local_key = Some(key.to_ascii_lowercase());
                    snapshot.stage = Stage::AwaitingApproval;
                } else {
                    *failure = Some("Unexpected diagnostic identity".into());
                }
            } else if let Some(id)=text.strip_prefix("ONLINE ") {
                let mut snapshot=locked(shared);
                let expected=snapshot.local_key.as_ref().and_then(|k|remote_ex_signaling::decode_hex::<32>(k).ok()).map(|k|remote_ex_signaling::hex(&remote_ex_security::fingerprint(&k)));
                if snapshot.profile.routed()&&expected.as_deref()==Some(id){snapshot.device_id=Some(id.into());}
                else{*failure=Some("Unexpected routed registration identity".into());}
            } else if text.starts_with("LISTENING ") {
                let current = locked(shared).stage;
                if current == Stage::Connecting { stage(shared, Stage::Listening, None); }
            } else if matches!(text.as_str(), "CONNECTED" | "CONNECTED TEST VIEW")
                || text.starts_with("CONNECTED SCREEN VIEW") || text.starts_with("CONNECTED CONTROL") || text.starts_with("CONNECTED ROUTED ") {
                let current = locked(shared).stage;
                if matches!(current, Stage::Connecting | Stage::Listening) {
                    stage(shared, Stage::Connected, None);
                }
            } else if text == "INPUT_RELEASED" {
                let mut snapshot = locked(shared);
                if snapshot.profile.requires_release() { snapshot.input_released = true; }
            } else if text.starts_with("PASS HOST:") || text.starts_with("PASS CONTROLLER:")
                || text.starts_with("PASS HOST VIEW:") || text.starts_with("PASS CONTROLLER VIEW:")
                || text.starts_with("PASS HOST CONTROL:") || text.starts_with("PASS CONTROLLER CONTROL:") || text.starts_with("PASS ROUTED HOST:") || text.starts_with("PASS ROUTED CONTROLLER:") {
                *pass = true;
            }
        }
    }
}
fn supervise(
    mut child: ChildGuard, commands: Receiver<WorkerCommand>, shared: Arc<Mutex<Snapshot>>,
    profile: Profile, host: bool, stop: Arc<AtomicBool>,
) {
    let (tx, output) = mpsc::sync_channel(64);
    let overflow = Arc::new(AtomicBool::new(false));
    let mut stdin = child.0.stdin.take();
    let mut approved = false;
    let Some(stdout) = child.0.stdout.take() else {
        stage(&shared, Stage::Failed, Some("Missing process stdout".into())); return;
    };
    let Some(stderr) = child.0.stderr.take() else {
        stage(&shared, Stage::Failed, Some("Missing process stderr".into())); return;
    };
    reader(stdout, tx.clone(), overflow.clone(), false);
    reader(stderr, tx, overflow.clone(), true);
    let started = Instant::now();
    let mut pass = false;
    let mut failure = None;
    let mut stopping: Option<Instant> = None;
    let mut stop_error: Option<String> = None;
    loop {
        for message in output.try_iter().take(64) {
            handle_output(&shared, message, &mut pass, &mut failure);
        }
        let reason = if overflow.load(Ordering::Acquire) { Some("Media process output limit exceeded".into()) }
            else if started.elapsed() >= SESSION_LIMIT { Some("Session expired".into()) }
            else { failure.clone() };
        if stopping.is_none() && (stop.load(Ordering::Acquire) || reason.is_some()) {
            if !profile.requires_release() {
                stage(&shared, if reason.is_some() { Stage::Failed } else { Stage::Canceled }, reason);
                return;
            }
            stop_error = reason;
            // Closing the retained consent pipe revokes the local input grant.
            // Keep polling for the child's explicit cleanup marker and exit.
            stdin.take();
            stopping = Some(Instant::now());
            stage(&shared, Stage::Stopping, None);
        }
        if stopping.is_some_and(|since| since.elapsed() >= Duration::from_secs(8)) {
            stage(&shared, Stage::Failed, Some("Control process did not stop in time; forced termination. Input release is not confirmed.".into()));
            return;
        }
        if stopping.is_none() { match commands.try_recv() {
            Err(TryRecvError::Disconnected) => { stop.store(true, Ordering::Release); continue; }
            Ok(WorkerCommand::Approve(peer)) => {
                let snapshot = locked(&shared).clone();
                if approved || snapshot.stage != Stage::AwaitingApproval || !key_valid(&peer, profile)
                    || snapshot.local_key.as_deref() == Some(peer.as_str())
                {
                    failure = Some("Invalid local approval state".into());
                    continue;
                }
                let Some(pipe) = stdin.as_mut() else {
                    failure = Some("Missing approval channel".into()); continue;
                };
                // Only fixed-size validated public key and a fixed consent line.
                // No arbitrary text, path, password or command is forwarded.
                if writeln!(pipe, "{peer}\n{}", profile.consent(host)).and_then(|_| pipe.flush()).is_err() {
                    failure = Some("Could not deliver local approval".into());
                    continue;
                }
                approved = true;
                // Secure view child monitors the pipe for parent loss. Keep
                // it open until the supervisor exits; the diagnostic does not.
                if profile == Profile::Diagnostic { stdin.take(); }
                stage(&shared, Stage::Connecting, None);
            }
            Err(TryRecvError::Empty) => {}
        }}
        match child.0.try_wait() {
            Ok(Some(status)) => {
                // The child is reaped. Drain its final bounded stdout/stderr.
                let until = Instant::now() + Duration::from_millis(200);
                while Instant::now() < until {
                    match output.recv_timeout(Duration::from_millis(15)) {
                        Ok(message) => handle_output(&shared, message, &mut pass, &mut failure),
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
                let released = locked(&shared).input_released;
                let unconfirmed = profile.requires_release() && approved && !released;
                if unconfirmed {
                    let reason = failure.or(stop_error).unwrap_or_else(|| "Control session ended".into());
                    stage(&shared, Stage::Failed, Some(format!("{reason}; input release not confirmed.")));
                } else if stopping.is_some() {
                    stage(&shared, if stop_error.is_some() { Stage::Failed } else { Stage::Canceled }, stop_error);
                } else if status.success() && pass && failure.is_none() && !overflow.load(Ordering::Acquire) {
                    stage(&shared, Stage::Passed, None);
                } else {
                    stage(&shared, Stage::Failed, failure.or_else(|| Some("Native session exited unsuccessfully".into())));
                }
                return;
            }
            Ok(None) => {}
            Err(e) => {
                let reason = if profile.requires_release() {
                    format!("Process status unavailable: {e}; input release not confirmed.")
                } else { e.to_string() };
                stage(&shared, Stage::Failed, Some(reason));
                return;
            }
        }
        thread::sleep(Duration::from_millis(40));
    }
}

#[derive(Default)]
pub struct SessionManager {
    worker: Mutex<Option<Worker>>,
    shared: Arc<Mutex<Snapshot>>,
}
impl SessionManager {
    fn snapshot(&self) -> Snapshot { locked(&self.shared).clone() }
    fn prepare(&self, role: &str, address: &str, preview: bool, profile: Profile, display_id: Option<u32>) -> Result<Snapshot, String> {
        let addr = local_endpoint(address)?;
        let flag = match role { "host" => "--listen", "controller" => "--connect", _ => return Err("Unknown diagnostic role".into()) };
        let host = role == "host";
        let mut command = Command::new(sidecar_path(profile)?);
        command.arg(flag).arg(addr.to_string());
        match profile {
            Profile::Diagnostic => {
                if preview && !host { command.arg("--preview"); }
            }
            Profile::SecureTest => { command.arg("--test").arg("--desktop-session"); }
            Profile::Control => {
                command.arg("--control");
                if host { command.arg(display_id.ok_or("Choose the primary display to control")?.to_string()); }
                command.arg("--desktop-session");
            }
            Profile::RoutedTest|Profile::RoutedView|Profile::RoutedControl=>return Err("Use the routed connection entry point".into()),
            Profile::Screen => {
                if host {
                    let display = display_id.ok_or("Choose the specific display to share")?;
                    command.arg("--screen").arg(display.to_string());
                } else { command.arg("--view"); }
                command.arg("--desktop-session");
            }
        }
        self.spawn(command,profile,host)
    }
    fn prepare_routed(&self,role:&str,url:&str,server_id:&str,profile:Profile,display_id:Option<u32>,relay_only:bool,persistent:bool,storage_consent:bool,ca_path:Option<String>)->Result<Snapshot,String>{
        if !profile.routed()||url.len()>512{return Err("Invalid routed session configuration".into());}
        if persistent&&(!storage_consent||profile==Profile::RoutedTest){return Err("Persistent routed identity requires explicit native storage consent".into());}
        let pin=remote_ex_signaling::decode_hex::<32>(server_id).map_err(|_|"Invalid expected server ID")?;
        let _=remote_ex_rendezvous_client::Endpoint::secure(url,pin)?;
        let host=match role{"host"=>true,"controller"=>false,_=>return Err("Unknown routed role".into())};
        let mut command=Command::new(sidecar_path(profile)?);
        command.arg(if host{"--host"}else{"--controller"}).arg(url).arg(remote_ex_signaling::hex(&pin));
        match profile{
            Profile::RoutedTest=>{},
            Profile::RoutedView|Profile::RoutedControl=>{
                command.arg(if profile==Profile::RoutedView{"--view"}else{"--control"});
                if host{command.arg(display_id.ok_or("Choose primary display")?.to_string());}
            },
            _=>return Err("Invalid routed session profile".into()),
        }
        if relay_only{command.arg("--relay-only");}
        if persistent{command.arg("--persistent");}
        if let Some(ca)=ca_path.filter(|s|!s.is_empty()){
            let path=std::path::Path::new(&ca);
            if ca.len()>1024||!path.is_absolute()||!path.is_file(){return Err("Choose a valid absolute local CA certificate path".into());}
            command.arg("--ca").arg(path);
        }
        command.arg("--desktop-session");
        self.spawn(command,profile,host)
    }
    fn spawn(&self,mut command:Command,profile:Profile,host:bool)->Result<Snapshot,String>{
        let mut worker=locked(&self.worker);
        if worker.as_ref().is_some_and(|w|!w.join.is_finished()){return Err("A remote session is already running".into());}
        if let Some(old)=worker.take(){let _=old.join.join();}
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW; status remains visible in main UI.
        }
        let child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().map_err(|e| format!("Cannot start native media diagnostic: {e}"))?;
        *locked(&self.shared) = Snapshot { stage: Stage::Starting, active: true, local_key: None, detail: None, profile, input_released: false, device_id: None };
        let (tx, rx) = mpsc::sync_channel(4);
        let shared = self.shared.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let join = thread::spawn(move || supervise(ChildGuard(child), rx, shared, profile, host, worker_stop));
        *worker = Some(Worker { tx, stop, join });
        Ok(self.snapshot())
    }
    fn confirm(&self, peer_key: &str, approved: bool) -> Result<Snapshot, String> {
        let peer = peer_key.trim().to_ascii_lowercase();
        let snapshot = self.snapshot();
        if !approved || snapshot.stage != Stage::AwaitingApproval || !key_valid(&peer, snapshot.profile)
            || (snapshot.profile.routed()&&snapshot.device_id.is_none())
            || snapshot.local_key.as_deref() == Some(peer.as_str()) {
            return Err("Verify the other device's full public key and explicitly approve this diagnostic".into());
        }
        let worker = locked(&self.worker);
        let w = worker.as_ref().ok_or("No diagnostic process")?;
        w.tx.try_send(WorkerCommand::Approve(peer)).map_err(|_| "Diagnostic approval channel unavailable".to_string())?;
        Ok(snapshot)
    }
    pub fn request_stop(&self) {
        if let Some(w) = locked(&self.worker).as_ref() {
            w.stop.store(true, Ordering::Release);
        }
    }
    pub fn shutdown(&self) {
        let mut worker = locked(&self.worker);
        if let Some(w) = worker.take() {
            w.stop.store(true, Ordering::Release);
            drop(w.tx);
            let _ = w.join.join();
        }
    }
}
impl Drop for SessionManager {
    fn drop(&mut self) { self.shutdown(); }
}
fn require_main(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" { Ok(()) } else { Err("Local main window required".into()) }
}
#[tauri::command]
pub fn lan_status(window: WebviewWindow, manager: State<'_, SessionManager>) -> Result<Snapshot, String> {
    require_main(&window)?; Ok(manager.snapshot())
}
#[tauri::command]
pub fn lan_prepare(window: WebviewWindow, manager: State<'_, SessionManager>, role: String, address: String, preview: bool, profile: String, display_id: Option<u32>) -> Result<Snapshot, String> {
    require_main(&window)?; manager.prepare(&role, &address, preview, Profile::parse(&profile)?, display_id)
}
#[tauri::command]
pub fn routed_prepare(window:WebviewWindow,manager:State<'_,SessionManager>,role:String,url:String,server_id:String,mode:String,display_id:Option<u32>,relay_only:bool,persistent:bool,storage_consent:bool,ca_path:Option<String>)->Result<Snapshot,String>{
    require_main(&window)?;
    let profile=match mode.as_str(){"test"=>Profile::RoutedTest,"view"=>Profile::RoutedView,"control"=>Profile::RoutedControl,_=>return Err("Invalid routed mode".into())};
    manager.prepare_routed(&role,url.trim(),server_id.trim(),profile,display_id,relay_only,persistent,storage_consent,ca_path)
}
#[tauri::command]
pub fn lan_confirm(window: WebviewWindow, manager: State<'_, SessionManager>, peer_key: String, approved: bool) -> Result<Snapshot, String> {
    require_main(&window)?; manager.confirm(&peer_key, approved)
}
#[tauri::command]
pub fn lan_disconnect(window: WebviewWindow, manager: State<'_, SessionManager>) -> Result<Snapshot, String> {
    require_main(&window)?; manager.request_stop(); Ok(manager.snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_specific_private_endpoints_and_valid_keys() {
        assert!(local_endpoint("127.0.0.1:47821").is_ok());
        assert!(local_endpoint("192.168.1.8:47821").is_ok());
        assert!(local_endpoint("[::1]:47821").is_ok());
        assert!(local_endpoint("0.0.0.0:47821").is_err());
        assert!(local_endpoint("8.8.8.8:47821").is_err());
        assert!(local_endpoint("127.0.0.1:0").is_err());
        assert!(key_valid(&"ab".repeat(32), Profile::Diagnostic));
        assert!(!key_valid(&"00".repeat(32), Profile::Diagnostic));
        assert!(!key_valid("x\nALLOW DIAGNOSTIC", Profile::Diagnostic));
        assert!(key_valid(&"ab".repeat(64), Profile::Screen));
        assert!(!key_valid(&"ab".repeat(32), Profile::Screen));
        assert!(!key_valid(&("ab".repeat(32) + &"00".repeat(32)), Profile::Screen));
    }
    #[test]
    fn control_requires_distinct_consent_and_release_marker() {
        assert_eq!(Profile::Control.consent(true), "ALLOW SCREEN AND INPUT");
        assert_eq!(Profile::Control.consent(false), "ALLOW REMOTE INPUT");
        let shared = Mutex::new(Snapshot { stage: Stage::Connected, active: true, local_key: None, detail: None, profile: Profile::Control, input_released: false, device_id: None });
        let (mut pass, mut failure) = (false, None);
        stage(&shared, Stage::Stopping, None);
        assert!(locked(&shared).active);
        handle_output(&shared, Output::Stdout("INPUT_RELEASED".into()), &mut pass, &mut failure);
        assert!(locked(&shared).input_released);
        assert!(!pass);
    }
    #[test]
    fn status_requires_valid_local_key_and_terminal_state() {
        let shared = Mutex::new(Snapshot { stage: Stage::Starting, active: true, local_key: None, detail: None, profile: Profile::Diagnostic, input_released: false, device_id: None });
        let (mut pass, mut fail) = (false, None);
        handle_output(&shared, Output::Stdout(format!("KEY {}", "ab".repeat(32))), &mut pass, &mut fail);
        assert_eq!(locked(&shared).stage, Stage::AwaitingApproval);
        stage(&shared, Stage::Canceled, None);
        assert!(!locked(&shared).active);
        assert!(locked(&shared).local_key.is_none());
    }
}
