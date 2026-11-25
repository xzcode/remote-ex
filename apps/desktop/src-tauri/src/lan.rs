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
    peer_id: Option<String>,
    decoded_fps: Option<f64>,
    decoded_bps: Option<u64>,
    transfer_done: Option<u64>,
    transfer_total: Option<u64>,
    transfer_verified: bool,
    transfer_paused: bool,
    checkpoint_saved: Option<u64>,
    directory_entries: Option<u32>,
    clipboard_sent: Option<u32>,
    clipboard_received: Option<u32>,
    catalog:Vec<remote_ex_desktop_features::directory::Entry>,
    catalog_ready:bool,
    catalog_selected:Option<usize>,
    manage_review:Option<remote_ex_desktop_features::directory::managed::Review>,
    manage_token:Option<String>,
    manage_submitted:bool,
    manage_approved:bool,
    manage_applied:Option<usize>,
    secondary_control:bool,
    resume_offset: Option<u64>,
    audio_volume: Option<u8>,
    network_bps: Option<u64>,
    rtt_ms: Option<u32>,
    loss_pct: Option<f64>,
    route_type: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self { stage: Stage::Idle, active: false, local_key: None, detail: None, profile: Profile::Diagnostic, input_released: false, device_id: None, peer_id: None, decoded_fps: None, decoded_bps: None, transfer_done:None, transfer_total:None, transfer_verified:false, transfer_paused:false, checkpoint_saved:None, directory_entries:None, clipboard_sent:None, clipboard_received:None, catalog:Vec::new(), catalog_ready:false,catalog_selected:None,manage_review:None,manage_token:None,manage_submitted:false,manage_approved:false,manage_applied:None,secondary_control:false,resume_offset:None, audio_volume:None, network_bps:None, rtt_ms:None, loss_pct:None, route_type:None }
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
enum Profile { Diagnostic, SecureTest, Screen, Control, RoutedTest, RoutedView, RoutedControl, RoutedFileHost, RoutedFileController, RoutedTextHost, RoutedTextController, RoutedPngHost, RoutedPngController, RoutedAudio, RoutedDirectoryHost, RoutedDirectoryController, RoutedClipboardSync, RoutedCatalog, RoutedAvView, RoutedAvControl, RoutedManage, RoutedCatalogBatch }
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
    fn routed(self)->bool{matches!(self,Self::RoutedTest|Self::RoutedView|Self::RoutedControl|Self::RoutedAvView|Self::RoutedAvControl)||self.exchange().is_some()||matches!(self,Self::RoutedAudio|Self::RoutedClipboardSync|Self::RoutedCatalog|Self::RoutedCatalogBatch|Self::RoutedManage)}
    fn exchange(self)->Option<(&'static str,bool)>{match self{Self::RoutedFileHost=>Some(("file",true)),Self::RoutedFileController=>Some(("file",false)),Self::RoutedTextHost=>Some(("text",true)),Self::RoutedTextController=>Some(("text",false)),Self::RoutedPngHost=>Some(("png",true)),Self::RoutedPngController=>Some(("png",false)),Self::RoutedDirectoryHost=>Some(("directory",true)),Self::RoutedDirectoryController=>Some(("directory",false)),_=>None}}
    fn graceful(self)->bool{self.requires_release()||self.exchange().is_some()||matches!(self,Self::RoutedAudio|Self::RoutedClipboardSync|Self::RoutedCatalog|Self::RoutedCatalogBatch|Self::RoutedManage)}
    fn av(self)->bool{matches!(self,Self::RoutedAvView|Self::RoutedAvControl)}
    fn view(self)->bool{matches!(self,Self::RoutedView|Self::RoutedAvView)}
    fn audio(self)->bool{self==Self::RoutedAudio||self.av()}
    fn requires_release(self)->bool{matches!(self,Self::Control|Self::RoutedControl|Self::RoutedView|Self::RoutedAvView|Self::RoutedAvControl)}
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
            Self::RoutedAvView if host=>"ALLOW ROUTED SCREEN AND AUDIO",
            Self::RoutedAvView=>"ALLOW ROUTED VIEW AND PLAY AUDIO",
            Self::RoutedAvControl if host=>"ALLOW ROUTED SCREEN AUDIO AND INPUT",
            Self::RoutedAvControl=>"ALLOW ROUTED INPUT AND PLAY AUDIO",
            Self::RoutedFileHost|Self::RoutedFileController=>if self.exchange().unwrap().1==host{"ALLOW SEND FILE"}else{"ALLOW RECEIVE FILE"},
            Self::RoutedTextHost|Self::RoutedTextController=>if self.exchange().unwrap().1==host{"ALLOW SEND CLIPBOARD TEXT"}else{"ALLOW RECEIVE CLIPBOARD TEXT"},
            Self::RoutedPngHost|Self::RoutedPngController=>if self.exchange().unwrap().1==host{"ALLOW SEND CLIPBOARD IMAGE"}else{"ALLOW RECEIVE CLIPBOARD IMAGE"},
            Self::RoutedAudio=>if host{"ALLOW SEND SYSTEM AUDIO"}else{"ALLOW PLAY SYSTEM AUDIO"},
            Self::RoutedClipboardSync=>"ALLOW LIVE CLIPBOARD TEXT SYNC",
            Self::RoutedCatalog=>if host{"ALLOW LIST AND DOWNLOAD SELECTED ROOT"}else{"ALLOW BROWSE AND RECEIVE ONE FILE"},
            Self::RoutedCatalogBatch=>if host{"ALLOW LIST AND DOWNLOAD FOUR FILES"}else{"ALLOW BROWSE AND RECEIVE FILE BATCH"},
            Self::RoutedManage=>if host{"ALLOW REVIEW FILE CHANGES"}else{"ALLOW PROPOSE FILE CHANGES"},
            Self::RoutedDirectoryHost|Self::RoutedDirectoryController=>if self.exchange().unwrap().1==host{"ALLOW SEND DIRECTORY"}else{"ALLOW RECEIVE DIRECTORY"},
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
        (p,true) if p.exchange().is_some()=>"routed_control.exe",
        (p,false) if p.exchange().is_some()=>"routed_control",
        (Profile::RoutedView|Profile::RoutedControl|Profile::RoutedAvView|Profile::RoutedAvControl|Profile::RoutedAudio|Profile::RoutedClipboardSync|Profile::RoutedCatalog|Profile::RoutedCatalogBatch|Profile::RoutedManage,true)=>"routed_control.exe",
        (Profile::RoutedView|Profile::RoutedControl|Profile::RoutedAvView|Profile::RoutedAvControl|Profile::RoutedAudio|Profile::RoutedClipboardSync|Profile::RoutedCatalog|Profile::RoutedCatalogBatch|Profile::RoutedManage,false)=>"routed_control",
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

enum WorkerCommand { Approve(String), Pause(bool), Volume(u8), CatalogSelect(usize), CatalogBatch(Vec<usize>), ManagePlan(String), ManageApprove(String) }
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
            } else if let Some(metrics)=text.strip_prefix("VIEW_STATS ") {
                let mut fields=metrics.split_ascii_whitespace();
                if let (Some(fps),Some(bps),None)=(fields.next().and_then(|s|s.parse::<f64>().ok()),fields.next().and_then(|s|s.parse::<u64>().ok()),fields.next()) {
                    if fps.is_finite()&&(0.0..=120.0).contains(&fps)&&bps<=20_000_000_000 {let mut snap=locked(shared);snap.decoded_fps=Some(fps);snap.decoded_bps=Some(bps);}
                }
            } else if let Some(metrics)=text.strip_prefix("NET_STATS ") {
                let fields:Vec<_>=metrics.split_ascii_whitespace().collect();
                if fields.len()==4{
                    let mut snap=locked(shared);
                    snap.network_bps=fields[0].parse::<u64>().ok().filter(|v|*v<=100_000_000_000);
                    snap.rtt_ms=fields[1].parse::<u32>().ok().filter(|v|*v<=120_000);
                    snap.loss_pct=fields[2].parse::<f64>().ok().filter(|v|v.is_finite()&&(0.0..=100.0).contains(v));
                    snap.route_type=match fields[3]{"direct"|"relay"=>Some(fields[3].into()),_=>None};
                }
            } else if let Some(progress)=text.strip_prefix("TRANSFER_PROGRESS ") {
                let mut fields=progress.split_ascii_whitespace();
                if let (Some(done),Some(total),None)=(fields.next().and_then(|s|s.parse::<u64>().ok()),fields.next().and_then(|s|s.parse::<u64>().ok()),fields.next()){
                    let mut snap=locked(shared);if (snap.profile.exchange().is_some()||matches!(snap.profile,Profile::RoutedCatalog|Profile::RoutedCatalogBatch)&&snap.catalog_selected.is_some())&&done<=total&&total<=64*1024*1024{snap.transfer_done=Some(done);snap.transfer_total=Some(total);}
                }
            } else if let Some(v)=text.strip_prefix("AUDIO_VOLUME "){
                let mut snap=locked(shared);if snap.profile.audio(){snap.audio_volume=v.parse::<u8>().ok().filter(|v|*v<=100);}
            } else if let Some(line)=text.strip_prefix("MANAGE_REVIEW "){
                let mut snap=locked(shared);
                if snap.profile==Profile::RoutedManage&&snap.stage==Stage::Connected&&snap.manage_review.is_none(){
                    let review=line.split_once(' ').and_then(|(token,json)|{
                        if token.len()!=64||!token.bytes().all(|b|b.is_ascii_hexdigit())||json.len()>remote_ex_desktop_features::directory::managed::MAX_REVIEW_BYTES{return None;}
                        let r:remote_ex_desktop_features::directory::managed::Review=serde_json::from_str(json).ok()?;
                        if r.items.is_empty()||r.items.len()>4{return None;}
                        Some((token.to_owned(),r))
                    });
                    if let Some((token,review))=review{snap.manage_token=Some(token);snap.manage_review=Some(review);}else{*failure=Some("Invalid local file review".into());}
                }
            } else if let Some(n)=text.strip_prefix("MANAGE_APPLIED "){
                let mut snap=locked(shared);
                if snap.profile==Profile::RoutedManage{
                    snap.manage_applied=n.parse::<usize>().ok().filter(|v|(1..=4).contains(v));
                    if snap.manage_applied.is_none(){*failure=Some("Invalid file change receipt".into());}
                }
            } else if let Some(line)=text.strip_prefix("CATALOG_ENTRY "){
                let mut snap=locked(shared);
                if matches!(snap.profile,Profile::RoutedCatalog|Profile::RoutedCatalogBatch|Profile::RoutedManage)&&snap.stage==Stage::Connected&&!snap.catalog_ready{
                    let valid=line.split_once(' ').and_then(|(i,json)|{
                        if i.parse::<usize>().ok()!=Some(snap.catalog.len())||snap.catalog.len()>=256{return None;}
                        serde_json::from_str::<remote_ex_desktop_features::directory::Entry>(json).ok()
                    });
                    if let Some(entry)=valid{snap.catalog.push(entry);}else{*failure=Some("Invalid catalog entry output".into());}
                }
            } else if let Some(n)=text.strip_prefix("CATALOG_READY "){
                let mut snap=locked(shared);
                if matches!(snap.profile,Profile::RoutedCatalog|Profile::RoutedCatalogBatch|Profile::RoutedManage)&&snap.stage==Stage::Connected&&!snap.catalog_ready{
                    let valid=n.parse::<usize>().ok()==Some(snap.catalog.len())
                        &&serde_json::to_vec(&snap.catalog).ok().and_then(|b|remote_ex_desktop_features::directory::decode_catalog(&b).ok()).is_some();
                    if valid{snap.catalog_ready=true;}else{*failure=Some("Invalid catalog manifest output".into());}
                }
            } else if let Some(n)=text.strip_prefix("CLIPBOARD_SYNC_COUNTS "){
                let mut s=locked(shared);if s.profile==Profile::RoutedClipboardSync{
                    let vals:Vec<_>=n.split_ascii_whitespace().filter_map(|v|v.parse::<u32>().ok()).collect();
                    if vals.len()==2&&vals.iter().all(|v|*v<=60){s.clipboard_sent=Some(vals[0]);s.clipboard_received=Some(vals[1]);}
                }
            } else if let Some(n)=text.strip_prefix("DIRECTORY_CONTENT "){
                let mut snap=locked(shared);if snap.profile.exchange().is_some_and(|(k,_)|k=="directory"){
                    snap.directory_entries=n.split_ascii_whitespace().next().and_then(|v|v.parse::<u32>().ok()).filter(|v|*v<=256);
                }
            } else if let Some(n)=text.strip_prefix("CHECKPOINT_SAVED "){
                let mut s=locked(shared);if s.profile.exchange().is_some_and(|(k,_)|k=="file"){s.checkpoint_saved=n.parse::<u64>().ok().filter(|v|*v>0&&*v<=64*1024*1024);}
            } else if let Some(n)=text.strip_prefix("TRANSFER_RESUME_OFFSET "){
                let mut s=locked(shared);if s.profile.exchange().is_some_and(|(k,_)|k=="file"){
                    let vals:Vec<_>=n.split_ascii_whitespace().filter_map(|v|v.parse::<u64>().ok()).collect();
                    if vals.len()==2&&vals[0]>0&&vals[0]<=vals[1]&&vals[1]<=64*1024*1024{s.resume_offset=Some(vals[0]);s.transfer_done=Some(vals[0]);s.transfer_total=Some(vals[1]);}
                }
            } else if text=="TRANSFER_PAUSED"||text=="TRANSFER_RESUMED"{
                let mut snap=locked(shared);if snap.profile.exchange().is_some_and(|(kind,_)|kind=="file"){snap.transfer_paused=text=="TRANSFER_PAUSED";}
            } else if text=="TRANSFER_VERIFIED" {
                let mut snap=locked(shared);if snap.profile.exchange().is_some()||matches!(snap.profile,Profile::RoutedCatalog|Profile::RoutedCatalogBatch){snap.transfer_verified=true;snap.transfer_paused=false;}
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
            if !profile.graceful() {
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
            stage(&shared, Stage::Failed, Some(if profile.requires_release(){"Control process did not stop in time; input release is not confirmed.".into()}else{"Transfer process did not stop in time; temporary-file cleanup is not confirmed.".into()}));
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
                let consent=if snapshot.secondary_control&&host{if profile.av(){"ALLOW SELECTED WINDOWS DISPLAY AUDIO AND INPUT"}else{"ALLOW SELECTED WINDOWS DISPLAY AND INPUT"}}else{profile.consent(host)};
                if writeln!(pipe, "{peer}\n{consent}").and_then(|_| pipe.flush()).is_err() {
                    failure = Some("Could not deliver local approval".into());
                    continue;
                }
                approved = true;
                if profile.routed(){
                    if let Ok(key)=remote_ex_signaling::decode_hex::<32>(&peer){locked(&shared).peer_id=Some(remote_ex_signaling::hex(&remote_ex_security::fingerprint(&key)));}
                }
                // Secure view child monitors the pipe for parent loss. Keep
                // it open until the supervisor exits; the diagnostic does not.
                if profile == Profile::Diagnostic { stdin.take(); }
                stage(&shared, Stage::Connecting, None);
            }
            Ok(WorkerCommand::Pause(paused)) => {
                if approved&&profile.exchange().is_some_and(|(kind,host_sends)|kind=="file"&&host_sends==host)&&locked(&shared).stage==Stage::Connected {
                    if let Some(pipe)=stdin.as_mut(){
                        if pipe.write_all(if paused{b"PAUSE\n".as_slice()}else{b"RESUME\n".as_slice()}).and_then(|_|pipe.flush()).is_err(){failure=Some("Could not deliver transfer control".into());}
                    }
                }
            }
            Ok(WorkerCommand::Volume(volume))=>{
                if approved&&profile.audio()&&!host&&volume<=100&&locked(&shared).stage==Stage::Connected{
                    if let Some(pipe)=stdin.as_mut(){if writeln!(pipe,"VOLUME {volume}").and_then(|_|pipe.flush()).is_err(){failure=Some("Could not deliver local audio volume".into());}}
                }
            }
            Ok(WorkerCommand::CatalogSelect(index))=>{
                let valid={let s=locked(&shared);s.stage==Stage::Connected&&s.catalog_ready&&s.catalog_selected==Some(index)&&s.catalog.get(index).is_some_and(|e|!e.directory)};
                if approved&&profile==Profile::RoutedCatalog&&!host&&valid{
                    if let Some(pipe)=stdin.as_mut(){if writeln!(pipe,"SELECT {index}").and_then(|_|pipe.flush()).is_err(){failure=Some("Could not deliver local file selection".into());}}
                }
            }
            Ok(WorkerCommand::CatalogBatch(indexes))=>{
                let valid={let s=locked(&shared);s.stage==Stage::Connected&&s.catalog_ready&&s.catalog_selected==indexes.first().copied()&&remote_ex_desktop_features::directory::selected_entries(&s.catalog,&indexes).is_ok()};
                if approved&&profile==Profile::RoutedCatalogBatch&&!host&&valid{
                    let value=indexes.iter().map(|n|n.to_string()).collect::<Vec<_>>().join(",");
                    if let Some(pipe)=stdin.as_mut(){if writeln!(pipe,"BATCH {value}").and_then(|_|pipe.flush()).is_err(){failure=Some("Could not deliver batch selection".into());}}
                }
            }
            Ok(WorkerCommand::ManagePlan(plan))=>{
                if approved&&profile==Profile::RoutedManage&&!host&&locked(&shared).stage==Stage::Connected{
                    if let Some(pipe)=stdin.as_mut(){if writeln!(pipe,"PLAN {plan}").and_then(|_|pipe.flush()).is_err(){failure=Some("Could not deliver local change request".into());}}
                }
            }
            Ok(WorkerCommand::ManageApprove(token))=>{
                let valid={let s=locked(&shared);s.stage==Stage::Connected&&s.manage_approved&&s.manage_token.as_deref()==Some(token.as_str())};
                if approved&&profile==Profile::RoutedManage&&host&&valid{
                    if let Some(pipe)=stdin.as_mut(){if writeln!(pipe,"APPROVE {token}").and_then(|_|pipe.flush()).is_err(){failure=Some("Could not deliver exact local review approval".into());}}
                }
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
    fn busy(&self)->bool{locked(&self.worker).as_ref().is_some_and(|w|!w.join.is_finished())}
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
            p if p.routed()=>return Err("Use the routed connection entry point".into()),
            Profile::RoutedTest|Profile::RoutedView|Profile::RoutedControl|Profile::RoutedFileHost|Profile::RoutedFileController|Profile::RoutedTextHost|Profile::RoutedTextController|Profile::RoutedPngHost|Profile::RoutedPngController|Profile::RoutedAudio|Profile::RoutedDirectoryHost|Profile::RoutedDirectoryController|Profile::RoutedClipboardSync|Profile::RoutedCatalog|Profile::RoutedCatalogBatch|Profile::RoutedAvView|Profile::RoutedAvControl|Profile::RoutedManage=>return Err("Use the routed connection entry point".into()),
            Profile::Screen => {
                if host {
                    let display = display_id.ok_or("Choose the specific display to share")?;
                    command.arg("--screen").arg(display.to_string());
                } else { command.arg("--view"); }
                command.arg("--desktop-session");
            }
        }
        self.spawn(command,profile,host,None,false)
    }
    fn prepare_routed(&self,role:&str,url:&str,server_id:&str,profile:Profile,display_id:Option<u32>,relay_only:bool,persistent:bool,storage_consent:bool,ca_path:Option<String>,history_path:Option<PathBuf>,quality:&str,transfer_path:Option<String>,volume:u8,extra_displays:Vec<u32>,identity_slot:u8,resume_from:Option<String>,checkpoint_path:Option<String>,shared_identity:bool,secondary:bool)->Result<Snapshot,String>{
        if !profile.routed()||url.len()>512{return Err("Invalid routed session configuration".into());}
        if persistent&&!storage_consent{return Err("Persistent routed identity requires explicit native storage consent".into());}
        if shared_identity&&(!persistent||role!="controller"||!matches!(profile,Profile::RoutedTest|Profile::RoutedView|Profile::RoutedControl|Profile::RoutedAvView|Profile::RoutedAvControl)){return Err("Shared identity requires an explicitly selected persistent outgoing desktop session".into());}
        let pin=remote_ex_signaling::decode_hex::<32>(server_id).map_err(|_|"Invalid expected server ID")?;
        let _=remote_ex_rendezvous_client::Endpoint::secure(url,pin)?;
        let host=match role{"host"=>true,"controller"=>false,_=>return Err("Unknown routed role".into())};
        if secondary&&(!cfg!(windows)||!host||!matches!(profile,Profile::RoutedControl|Profile::RoutedAvControl)){return Err("Experimental secondary display control requires Windows host control".into());}
        if (resume_from.is_some()||checkpoint_path.is_some())&&!profile.exchange().is_some_and(|(kind,sender)|kind=="file"&&sender!=host){return Err("Checkpoint options require the local file receiver".into());}
        let mut command=Command::new(sidecar_path(profile)?);
        command.arg(if host{"--host"}else{"--controller"}).arg(url).arg(remote_ex_signaling::hex(&pin));
        match profile{
            Profile::RoutedTest=>{},
            Profile::RoutedView|Profile::RoutedControl|Profile::RoutedAvView|Profile::RoutedAvControl=>{
                command.arg(match profile{Profile::RoutedView=>"--view",Profile::RoutedControl=>"--control",Profile::RoutedAvView=>"--av-view",_=>"--av-control"});
                if host{
                    let main=display_id.ok_or("Choose a display")?;
                    command.arg(main.to_string());
                    if !profile.view()&&!extra_displays.is_empty(){return Err("Control requires only the primary display".into());}
                    if extra_displays.len()>3||extra_displays.iter().enumerate().any(|(i,d)|*d==main||extra_displays[..i].contains(d)){return Err("Select up to four distinct displays".into());}
                    for d in extra_displays{command.arg("--view-extra").arg(d.to_string());}
                }else if !extra_displays.is_empty(){return Err("Only the host can authorize additional displays".into());}
                if profile.av(){if volume>100{return Err("Invalid audio volume".into());}command.arg("--volume").arg(volume.to_string());}
            },
            Profile::RoutedManage=>{
                command.arg("--manage");
                if host{
                    let root=PathBuf::from(transfer_path.ok_or("Choose local directory for reviewed changes")?);
                    if !root.is_absolute()||!std::fs::symlink_metadata(&root).is_ok_and(|m|m.file_type().is_dir()){return Err("Select an ordinary absolute local directory".into());}
                    command.arg(root);
                }
            },
            Profile::RoutedCatalog|Profile::RoutedCatalogBatch=>{
                let path=PathBuf::from(transfer_path.ok_or("Choose local catalog root or destination")?);
                if !path.is_absolute()||path.file_name().is_none(){return Err("Choose an absolute local path".into());}
                if host{
                    let m=std::fs::symlink_metadata(&path).map_err(|_|"Selected directory unavailable")?;
                    if !m.file_type().is_dir(){return Err("Select an ordinary local directory".into());}
                }else if path.exists()||!path.parent().is_some_and(|p|p.is_dir()){return Err("Choose a new file destination".into());}
                command.arg(if profile==Profile::RoutedCatalogBatch{"--catalog-batch"}else{"--catalog"}).arg(path);
            },
            Profile::RoutedClipboardSync=>{command.arg("--clipboard-sync");},
            Profile::RoutedAudio=>{
                if host&&!cfg!(any(target_os="windows",target_os="macos")){return Err("System audio source requires Windows or macOS".into());}
                if volume>100{return Err("Volume out of range".into());}
                command.arg("--audio").arg("--volume").arg(volume.to_string());
            },
            p if p.exchange().is_some()=>{
                let(kind,host_sends)=p.exchange().unwrap();
                command.arg("--exchange").arg(kind).arg(if host_sends{"host"}else{"controller"});
                if matches!(kind,"file"|"directory"){
                    let path=PathBuf::from(transfer_path.ok_or("Choose local source or destination path")?);
                    if !path.is_absolute()||path.file_name().is_none(){return Err("Choose an absolute local file path".into());}
                    if host_sends==host{
                        let meta=std::fs::symlink_metadata(&path).map_err(|_|"Source file unavailable")?;
                        if kind=="file"&&(!meta.file_type().is_file()||meta.len()>64*1024*1024){return Err("Select a regular file up to 64 MiB".into());}
                        if kind=="directory"&&!meta.file_type().is_dir(){return Err("Select an ordinary directory".into());}
                    }else if path.exists()||!path.parent().is_some_and(|p|p.is_dir()){return Err("Choose an unused destination in an existing local directory".into());}
                    command.arg(&path);
                    if let Some(from)=&resume_from{
                        let p=PathBuf::from(from);if !p.is_absolute()||p==path||!p.is_file(){return Err("Choose a separate existing checkpoint".into());}
                        command.arg("--resume-from").arg(p);
                    }
                    if let Some(save)=&checkpoint_path{
                        let p=PathBuf::from(save);if !p.is_absolute()||p==path||p.exists()||!p.parent().is_some_and(|d|d.is_dir())||resume_from.as_deref()==Some(save.as_str()){return Err("Choose a separate unused checkpoint path".into());}
                        command.arg("--save-checkpoint").arg(p);
                    }
                }
            },
            _=>return Err("Invalid routed session profile".into()),
        }
        remote_ex_desktop_features::quality::preset(quality)?;
        command.arg("--quality").arg(quality);
        if relay_only{command.arg("--relay-only");}
        if persistent{remote_ex_security::identity_store::credential_name(identity_slot)?;command.arg("--persistent").arg("--identity-slot").arg(identity_slot.to_string());}
        if shared_identity{command.arg("--shared-controller");}
        if secondary{command.arg("--secondary-control");}
        if let Some(ca)=ca_path.filter(|s|!s.is_empty()){
            let path=std::path::Path::new(&ca);
            if ca.len()>1024||!path.is_absolute()||!path.is_file(){return Err("Choose a valid absolute local CA certificate path".into());}
            command.arg("--ca").arg(path);
        }
        command.arg("--desktop-session");
        let history=history_path.map(|path|(path,remote_ex_signaling::hex(&pin)));
        self.spawn(command,profile,host,history,secondary)
    }
    fn spawn(&self,mut command:Command,profile:Profile,host:bool,history:Option<(PathBuf,String)>,secondary:bool)->Result<Snapshot,String>{
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
        *locked(&self.shared) = Snapshot { stage: Stage::Starting, active: true, local_key: None, detail: None, profile, input_released: false, device_id: None, peer_id: None, decoded_fps: None, decoded_bps: None, transfer_done:None, transfer_total:None, transfer_verified:false, transfer_paused:false, checkpoint_saved:None, directory_entries:None, clipboard_sent:None, clipboard_received:None, catalog:Vec::new(), catalog_ready:false,catalog_selected:None,manage_review:None,manage_token:None,manage_submitted:false,manage_approved:false,manage_applied:None,secondary_control:secondary,resume_offset:None, audio_volume:None, network_bps:None, rtt_ms:None, loss_pct:None, route_type:None };
        let (tx, rx) = mpsc::sync_channel(4);
        let shared = self.shared.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let join = thread::spawn(move || {
            let started=Instant::now();
            supervise(ChildGuard(child), rx, shared.clone(), profile, host, worker_stop);
            if let Some((path,server))=history{
                let snap=locked(&shared).clone();
                let mode=match profile{Profile::RoutedTest=>"test",Profile::RoutedView|Profile::RoutedAvView=>"view",Profile::RoutedControl|Profile::RoutedAvControl=>"control",_=>return};
                let outcome=match snap.stage{Stage::Passed=>"passed",Stage::Failed=>"failed",Stage::Canceled=>"canceled",_=>return};
                if let Some(id)=snap.peer_id {
                    // A history failure cannot change actual release confirmation.
                    if crate::trust::record_history(&path,&server,&id,mode,outcome,started.elapsed().as_secs()).is_err(){
                        let mut s=locked(&shared);if s.detail.is_none(){s.detail=Some("Local connection history could not be saved".into());}
                    }
                }
            }
        });
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
pub fn lan_prepare(window: WebviewWindow, manager: State<'_, SessionManager>, managers:State<'_,RoutedManagers>, role: String, address: String, preview: bool, profile: String, display_id: Option<u32>) -> Result<Snapshot, String> {
    require_main(&window)?;
    // Hold the routed registry lock across the LAN start to prevent races.
    let slots=locked(&managers.slots);
    if slots.sessions.iter().any(SessionManager::busy){return Err("Finish routed sessions first".into());}
    manager.prepare(&role, &address, preview, Profile::parse(&profile)?, display_id)
}
// Independent bounded controller slots. Host/capture, clipboard, file and
// audio sessions are exclusive; separate identities remain separate devices.
// The signaling server still refuses duplicate registrations of one key.
#[derive(Default)]
struct Slots{sessions:[SessionManager;3],exclusive:[bool;3],identity_slot:[Option<u8>;3],shared:[bool;3]}
#[derive(Default)]
pub struct RoutedManagers{slots:Mutex<Slots>}
fn slot_index(slot:Option<u8>)->Result<usize,String>{
    let i=usize::from(slot.unwrap_or(0));if i>=3{return Err("Invalid session slot".into());}Ok(i)
}
impl RoutedManagers{
    pub fn any_busy(&self)->bool{locked(&self.slots).sessions.iter().any(SessionManager::busy)}
    pub fn shutdown(&self){
        let slots=locked(&self.slots);
        for manager in &slots.sessions{manager.request_stop();}
        for manager in &slots.sessions{manager.shutdown();}
    }
}
impl Drop for RoutedManagers{fn drop(&mut self){self.shutdown();}}
#[derive(Serialize)]
pub struct SlotIdentityInfo{device_id:String,public_key:String}
#[tauri::command]
pub async fn routed_identity_info(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>,confirmed:bool,shared_identity:Option<bool>)->Result<SlotIdentityInfo,String>{
    require_main(&window)?;
    if !confirmed{return Err("Explicit system credential storage consent is required".into());}
    let i=slot_index(slot)?;
    let identity=if shared_identity.unwrap_or(false){0}else{i as u8};
    {if locked(&managers.slots).sessions[i].busy(){return Err("Finish this session before preparing its identity".into());}}
    #[cfg(any(target_os="windows",target_os="macos"))]
    {
        tauri::async_runtime::spawn_blocking(move||{
            let id=remote_ex_security::identity_store::native::open_slot(identity)?;
            Ok(SlotIdentityInfo{device_id:remote_ex_signaling::hex(&id.fingerprint()),public_key:remote_ex_signaling::hex(&id.public_key())})
        }).await.map_err(|_|"Identity task failed")?
    }
    #[cfg(not(any(target_os="windows",target_os="macos")))]
    {let _=i;Err("Native identity storage unavailable".into())}
}
#[tauri::command]
pub fn routed_status(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>)->Result<Snapshot,String>{
    require_main(&window)?;Ok(locked(&managers.slots).sessions[slot_index(slot)?].snapshot())
}
#[tauri::command]
pub fn routed_disconnect(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>)->Result<Snapshot,String>{
    require_main(&window)?;let slots=locked(&managers.slots);let manager=&slots.sessions[slot_index(slot)?];manager.request_stop();Ok(manager.snapshot())
}
#[tauri::command]
pub fn routed_transfer_pause(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>,paused:bool)->Result<Snapshot,String>{
    require_main(&window)?;
    let slots=locked(&managers.slots);let manager=&slots.sessions[slot_index(slot)?];
    let snap=manager.snapshot();
    if snap.stage!=Stage::Connected||!snap.profile.exchange().is_some_and(|(kind,_)|kind=="file")||snap.transfer_verified{return Err("No active file transfer".into());}
    let worker=locked(&manager.worker);
    worker.as_ref().ok_or("No active transfer")?.tx.try_send(WorkerCommand::Pause(paused)).map_err(|_|"Transfer control unavailable".to_owned())?;
    Ok(snap)
}

#[tauri::command]
pub fn routed_manage_submit(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>,plan:remote_ex_desktop_features::directory::managed::Plan,confirmed:bool)->Result<Snapshot,String>{
    require_main(&window)?;
    if !confirmed{return Err("Confirm sending the proposed changes for host review".into());}
    let slots=locked(&managers.slots);let manager=&slots.sessions[slot_index(slot)?];
    let mut snap=locked(&manager.shared);
    if snap.stage!=Stage::Connected||snap.profile!=Profile::RoutedManage||!snap.catalog_ready||snap.manage_submitted{return Err("No unsubmitted file change session".into());}
    // Session-bound review is computed by the authenticated child; this
    // parent only validates the same typed, bounded proposal and indices.
    plan.review(&snap.catalog,[1;16])?;
    let encoded=String::from_utf8(plan.encode()?).map_err(|_|"Invalid file plan encoding")?;
    let worker=locked(&manager.worker);
    worker.as_ref().ok_or("No active file-management process")?.tx.try_send(WorkerCommand::ManagePlan(encoded)).map_err(|_|"File change request unavailable".to_owned())?;
    snap.manage_submitted=true;Ok(snap.clone())
}
#[tauri::command]
pub fn routed_manage_approve(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>,token:String,confirmed:bool)->Result<Snapshot,String>{
    require_main(&window)?;
    if !confirmed{return Err("Explicitly confirm the exact local review".into());}
    let slots=locked(&managers.slots);let manager=&slots.sessions[slot_index(slot)?];let mut snap=locked(&manager.shared);
    if snap.stage!=Stage::Connected||snap.profile!=Profile::RoutedManage||snap.manage_review.is_none()||snap.manage_approved||snap.manage_token.as_deref()!=Some(token.as_str()){
        return Err("No matching pending local file review".into());
    }
    let worker=locked(&manager.worker);
    worker.as_ref().ok_or("No active file-management process")?.tx.try_send(WorkerCommand::ManageApprove(token)).map_err(|_|"Local review approval unavailable".to_owned())?;
    snap.manage_approved=true;Ok(snap.clone())
}
#[tauri::command]
pub fn routed_catalog_select(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>,index:usize,confirmed:bool)->Result<Snapshot,String>{
    require_main(&window)?;
    if !confirmed{return Err("Confirm the selected download".into());}
    let slots=locked(&managers.slots);let manager=&slots.sessions[slot_index(slot)?];
    let mut snap=locked(&manager.shared);
    if snap.stage!=Stage::Connected||snap.profile!=Profile::RoutedCatalog||!snap.catalog_ready||snap.catalog_selected.is_some()
        ||!snap.catalog.get(index).is_some_and(|e|!e.directory){return Err("Select one file in the current approved catalog".into());}
    let worker=locked(&manager.worker);
    worker.as_ref().ok_or("No active catalog process")?.tx.try_send(WorkerCommand::CatalogSelect(index)).map_err(|_|"File selection unavailable".to_owned())?;
    snap.catalog_selected=Some(index);
    Ok(snap.clone())
}
#[tauri::command]
pub fn routed_catalog_batch_select(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>,indexes:Vec<usize>,confirmed:bool)->Result<Snapshot,String>{
    require_main(&window)?;
    if !confirmed{return Err("Confirm the selected batch download".into());}
    let slots=locked(&managers.slots);let manager=&slots.sessions[slot_index(slot)?];
    let mut snap=locked(&manager.shared);
    if snap.stage!=Stage::Connected||snap.profile!=Profile::RoutedCatalogBatch||!snap.catalog_ready||snap.catalog_selected.is_some(){return Err("No active batch download".into());}
    remote_ex_desktop_features::directory::selected_entries(&snap.catalog,&indexes)?;
    let marker=indexes.first().copied();
    let worker=locked(&manager.worker);
    worker.as_ref().ok_or("No active catalog process")?.tx.try_send(WorkerCommand::CatalogBatch(indexes)).map_err(|_|"Batch selection unavailable".to_owned())?;
    snap.catalog_selected=marker;Ok(snap.clone())
}
#[tauri::command]
pub fn routed_audio_volume(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>,volume:u8)->Result<Snapshot,String>{
    require_main(&window)?;
    let slots=locked(&managers.slots);let manager=&slots.sessions[slot_index(slot)?];let snap=manager.snapshot();
    if snap.stage!=Stage::Connected||!snap.profile.audio()||volume>100{return Err("No active audio playback".into());}
    let worker=locked(&manager.worker);worker.as_ref().ok_or("No active audio process")?.tx.try_send(WorkerCommand::Volume(volume)).map_err(|_|"Audio control unavailable".to_owned())?;Ok(snap)
}
#[tauri::command]
pub async fn select_transfer_file(window:WebviewWindow,receive:bool,directory:Option<bool>)->Result<Option<String>,String>{
    require_main(&window)?;
    #[cfg(any(target_os="windows",target_os="macos"))]
    {
        let dialog=rfd::AsyncFileDialog::new();
        let file=if directory.unwrap_or(false){dialog.pick_folder().await}else if receive{dialog.save_file().await}else{dialog.pick_file().await};
        file.map(|f|f.path().to_str().map(str::to_owned).ok_or("Unsupported local path".to_owned())).transpose()
    }
    #[cfg(not(any(target_os="windows",target_os="macos")))]
    {let _=(receive,directory);Err("Native file selection unavailable".into())}
}
#[tauri::command]
pub fn routed_prepare(window:WebviewWindow,manager:State<'_,SessionManager>,managers:State<'_,RoutedManagers>,slot:Option<u8>,role:String,url:String,server_id:String,mode:String,display_id:Option<u32>,relay_only:bool,persistent:bool,storage_consent:bool,ca_path:Option<String>,save_history:Option<bool>,quality:Option<String>,sender:Option<String>,transfer_path:Option<String>,volume:Option<u8>,extra_displays:Option<Vec<u32>>,resume_from:Option<String>,checkpoint_path:Option<String>,shared_identity:Option<bool>,secondary_control:Option<bool>)->Result<Snapshot,String>{
    require_main(&window)?;
    let profile=match (mode.as_str(),sender.as_deref()){
        ("test",_)=>Profile::RoutedTest,("view",_)=>Profile::RoutedView,("control",_)=>Profile::RoutedControl,("av_view",_)=>Profile::RoutedAvView,("av_control",_)=>Profile::RoutedAvControl,("audio",_)=>Profile::RoutedAudio,("clipboard_sync",_)=>Profile::RoutedClipboardSync,("catalog",_)=>Profile::RoutedCatalog,("catalog_batch",_)=>Profile::RoutedCatalogBatch,("manage",_)=>Profile::RoutedManage,
        ("file",Some("host"))=>Profile::RoutedFileHost,("file",Some("controller"))=>Profile::RoutedFileController,
        ("text",Some("host"))=>Profile::RoutedTextHost,("text",Some("controller"))=>Profile::RoutedTextController,
        ("png",Some("host"))=>Profile::RoutedPngHost,("png",Some("controller"))=>Profile::RoutedPngController,
        ("directory",Some("host"))=>Profile::RoutedDirectoryHost,("directory",Some("controller"))=>Profile::RoutedDirectoryController,
        _=>return Err("Invalid routed mode or direction".into()),
    };
    let history=if save_history.unwrap_or(false){Some(crate::trust::database_path(&window)?)}else{None};
    let i=slot_index(slot)?;
    let mut slots=locked(&managers.slots);
    if manager.busy(){return Err("Finish the LAN session first".into());}
    let exclusive=role=="host"||profile.exchange().is_some()||matches!(profile,Profile::RoutedAudio|Profile::RoutedClipboardSync|Profile::RoutedCatalog|Profile::RoutedCatalogBatch|Profile::RoutedManage);
    let shared=shared_identity.unwrap_or(false);
    if shared&&(!persistent||exclusive){return Err("Shared identity is only for persistent outgoing test/view/control sessions".into());}
    let identity=if shared{0}else{i as u8};
    for (n,other) in slots.sessions.iter().enumerate(){
        if n==i||!other.busy(){continue;}
        if exclusive||slots.exclusive[n]{return Err("Host and content-sharing sessions require exclusive access".into());}
        if persistent&&slots.identity_slot[n]==Some(identity)&&!(shared&&slots.shared[n]){return Err("Choose shared identity in all slots using the same device ID".into());}
    }
    let result=slots.sessions[i].prepare_routed(&role,url.trim(),server_id.trim(),profile,display_id,relay_only,persistent,storage_consent,ca_path,history,quality.as_deref().unwrap_or("compat"),transfer_path,volume.unwrap_or(75),extra_displays.unwrap_or_default(),identity,resume_from,checkpoint_path,shared,secondary_control.unwrap_or(false))?;
    slots.exclusive[i]=exclusive;slots.identity_slot[i]=persistent.then_some(identity);slots.shared[i]=shared;Ok(result)
}
#[tauri::command]
pub fn lan_confirm(window: WebviewWindow, manager: State<'_, SessionManager>, peer_key: String, approved: bool) -> Result<Snapshot, String> {
    require_main(&window)?;
    if manager.snapshot().profile.routed(){return Err("Use device-ID confirmation for a routed session".into());}
    manager.confirm(&peer_key, approved)
}
#[tauri::command]
pub fn routed_confirm(window:WebviewWindow,managers:State<'_,RoutedManagers>,slot:Option<u8>,device_id:String,peer_key:String,approved:bool)->Result<Snapshot,String>{
    require_main(&window)?;
    let slots=locked(&managers.slots);let manager=&slots.sessions[slot_index(slot)?];
    if !manager.snapshot().profile.routed(){return Err("No routed session to confirm".into());}
    let key=remote_ex_signaling::decode_hex::<32>(peer_key.trim()).map_err(|_|"Invalid peer key")?;
    let expected=remote_ex_signaling::hex(&remote_ex_security::fingerprint(&key));
    if expected!=device_id.trim().to_ascii_lowercase(){return Err("Device ID does not match the verified public key".into());}
    manager.confirm(&peer_key,approved)
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
        let shared = Mutex::new(Snapshot { stage: Stage::Connected, active: true, local_key: None, detail: None, profile: Profile::Control, input_released: false, device_id: None, peer_id: None, decoded_fps: None, decoded_bps: None, transfer_done:None, transfer_total:None, transfer_verified:false, transfer_paused:false, checkpoint_saved:None, directory_entries:None, clipboard_sent:None, clipboard_received:None, catalog:Vec::new(), catalog_ready:false,catalog_selected:None,manage_review:None,manage_token:None,manage_submitted:false,manage_approved:false,manage_applied:None,secondary_control:false,resume_offset:None, audio_volume:None, network_bps:None, rtt_ms:None, loss_pct:None, route_type:None });
        let (mut pass, mut failure) = (false, None);
        stage(&shared, Stage::Stopping, None);
        assert!(locked(&shared).active);
        handle_output(&shared, Output::Stdout("INPUT_RELEASED".into()), &mut pass, &mut failure);
        assert!(locked(&shared).input_released);
        assert!(!pass);
    }
    #[test]
    fn status_requires_valid_local_key_and_terminal_state() {
        let shared = Mutex::new(Snapshot { stage: Stage::Starting, active: true, local_key: None, detail: None, profile: Profile::Diagnostic, input_released: false, device_id: None, peer_id: None, decoded_fps: None, decoded_bps: None, transfer_done:None, transfer_total:None, transfer_verified:false, transfer_paused:false, checkpoint_saved:None, directory_entries:None, clipboard_sent:None, clipboard_received:None, catalog:Vec::new(), catalog_ready:false,catalog_selected:None,manage_review:None,manage_token:None,manage_submitted:false,manage_approved:false,manage_applied:None,secondary_control:false,resume_offset:None, audio_volume:None, network_bps:None, rtt_ms:None, loss_pct:None, route_type:None });
        let (mut pass, mut fail) = (false, None);
        handle_output(&shared, Output::Stdout(format!("KEY {}", "ab".repeat(32))), &mut pass, &mut fail);
        assert_eq!(locked(&shared).stage, Stage::AwaitingApproval);
        stage(&shared, Stage::Canceled, None);
        assert!(!locked(&shared).active);
        assert!(locked(&shared).local_key.is_none());
    }
}
