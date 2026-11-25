//! Explicit, bounded view-only LAN prototype. No input injection, clipboard,
//! file transfer, background listener, unattended access, or public routing.
//! Screen pixels enter the native media pipeline only after both peers
//! authenticate, verify their actual DTLS certificates, and confirm readiness.
use gstreamer_webrtc::WebRTCPeerConnectionState;
use remote_ex_core::{Permissions, SessionGate};
use remote_ex_media::{Description, DescriptionKind, Event, IceCandidate, MediaResult, Peer, Source, VideoConfig};
use remote_ex_security::{DeviceIdentity, PublicKey, Role, pairing::{PendingPairing, new_session_id}, session::VIEW};
use remote_ex_signaling::{Candidate, Message, Wire, decode_hex, hex, MAX_CANDIDATES, secure::{NoiseIdentity, SecureWire}};
use std::{io::{self, IsTerminal, Write}, net::SocketAddr, time::{Duration, Instant}};

#[derive(Clone, Copy)]
enum Profile {
    Test,
    Screen,
}
impl Profile {
    fn synthetic(self) -> bool { matches!(self, Self::Test) }
    fn seconds(self) -> u32 { if self.synthetic() { 6 } else { 30 } }
}
#[derive(Clone, Copy)]
enum Mode { Host, Controller }
struct Arguments {
    mode: Mode,
    address: SocketAddr,
    profile: Profile,
    display: Option<u32>,
}
fn error<T: std::fmt::Debug>(e: T) -> String { format!("{e:?}") }
fn out(line: &str) -> MediaResult<()> {
    println!("{line}"); io::stdout().flush().map_err(|e| e.to_string())
}
fn input() -> MediaResult<String> {
    let mut text = String::new();
    io::stdin().read_line(&mut text).map_err(|e| e.to_string())?;
    Ok(text.trim().to_owned())
}
fn send(w: &mut SecureWire, msg: Message) -> MediaResult<()> { w.send(msg).map_err(error) }
fn recv(w: &mut SecureWire, id: Option<[u8;16]>) -> MediaResult<Message> { w.recv(id).map_err(error) }
fn usage() -> String {
    "Usage: lan_view --listen IP:PORT --test | --screen DISPLAY_ID; lan_view --connect IP:PORT --test | --view".into()
}
fn arguments() -> MediaResult<Arguments> {
    let mut args = std::env::args().skip(1);
    let mode = match args.next().as_deref() {
        Some("--listen") => Mode::Host, Some("--connect") => Mode::Controller, _ => return Err(usage()),
    };
    let address = args.next().ok_or_else(usage)?.parse().map_err(error)?;
    if !remote_ex_signaling::local_address(address) { return Err("Specific private or loopback address required".into()); }
    let (profile, display) = match (mode, args.next().as_deref()) {
        (_, Some("--test")) => (Profile::Test, None),
        (Mode::Controller, Some("--view")) => (Profile::Screen, None),
        (Mode::Host, Some("--screen")) => {
            if !cfg!(feature = "local-screen") { return Err("Build with local-screen to share a real display".into()); }
            (Profile::Screen, Some(args.next().ok_or_else(usage)?.parse().map_err(error)?))
        }
        _ => return Err(usage()),
    };
    if args.next().is_some() { return Err(usage()); }
    // No unattended screen sharing. Test-only automation is separate.
    if !profile.synthetic() && (!io::stdin().is_terminal() || !io::stdout().is_terminal()) {
        return Err("Real screen sharing requires an interactive local terminal".into());
    }
    Ok(Arguments { mode, address, profile, display })
}
fn identities(args: &Arguments) -> MediaResult<(DeviceIdentity, NoiseIdentity, PublicKey, [u8;32])> {
    let id = DeviceIdentity::generate();
    let noise = NoiseIdentity::generate().map_err(error)?;
    let mut bundle = Vec::with_capacity(64);
    bundle.extend_from_slice(&id.public_key());
    bundle.extend_from_slice(&noise.public_key());
    out(&format!("KEY {}", hex(&bundle)))?;
    out("PEER_KEY (independently compare the other device's complete 128-hex public key bundle)")?;
    let bytes = decode_hex::<64>(&input()?).map_err(error)?;
    let peer: PublicKey = bytes[..32].try_into().map_err(error)?;
    let peer_noise: [u8;32] = bytes[32..].try_into().map_err(error)?;
    if peer == [0;32] || peer_noise == [0;32] || peer == id.public_key() || peer_noise == noise.public_key() {
        return Err("Invalid peer identity".into());
    }
    let consent = if args.profile.synthetic() { "ALLOW TEST VIEW" } else if let Some(display) = args.display {
        out(&format!("Real screen sharing: display {display}, view only, 30 seconds maximum. No keyboard/mouse access. Ctrl-C cancels. Keep this terminal visible."))?;
        "SHARE SCREEN"
    } else {
        out("View the authorized remote display for up to 30 seconds, without input control. Ctrl-C disconnects.")?;
        "ALLOW SCREEN VIEW"
    };
    out(&format!("CONSENT (type {consent})"))?;
    if input()? != consent { return Err("Local consent denied".into()); }
    Ok((id, noise, peer, peer_noise))
}
fn gather(peer: &Peer) -> MediaResult<Vec<Candidate>> {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut items = Vec::new();
    loop {
        for event in peer.drain_events()? {
            if let Event::Ice(ice) = event {
                if items.len() >= MAX_CANDIDATES { return Err("ICE list exceeds limit".into()); }
                items.push(Candidate { mline: ice.mline, candidate: ice.candidate });
            }
        }
        if peer.ice_complete() {
            // Let the final candidate signal enter the bounded queue.
            std::thread::sleep(Duration::from_millis(30));
            for event in peer.drain_events()? {
                if let Event::Ice(ice) = event {
                    if items.len() >= MAX_CANDIDATES { return Err("ICE list exceeds limit".into()); }
                    items.push(Candidate { mline: ice.mline, candidate: ice.candidate });
                }
            }
            if !items.iter().any(|item| !item.candidate.is_empty()) { return Err("No ICE candidates".into()); }
            return Ok(items);
        }
        if Instant::now() >= deadline { return Err("ICE gathering timed out".into()); }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn receive_ice(wire: &mut SecureWire, peer: &Peer, session: [u8;16]) -> MediaResult<()> {
    let Message::Candidates { items, .. } = recv(wire, Some(session))? else { return Err("Expected encrypted ICE".into()); };
    for item in items { peer.add_ice(&IceCandidate { mline: item.mline, candidate: item.candidate })?; }
    Ok(())
}
fn wait_connected(peer: &Peer, remote_sdp: &str, sending: bool) -> MediaResult<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        peer.check_health()?;
        if peer.connection_state() == WebRTCPeerConnectionState::Connected {
            return peer.verify_video_peer(remote_sdp, sending);
        }
        if Instant::now() >= deadline { return Err("Authenticated media connection timed out".into()); }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn ensure_connected(peer: &Peer) -> MediaResult<()> {
    peer.check_health()?;
    if peer.connection_state() != WebRTCPeerConnectionState::Connected {
        return Err("Media connection lost; stopping screen sharing".into());
    }
    Ok(())
}
fn wait_offer(peer: &Peer) -> MediaResult<Description> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if peer.drain_events()?.iter().any(|e| matches!(e, Event::NegotiationNeeded)) {
            let offer = peer.create_description(DescriptionKind::Offer)?;
            if offer.sdp.lines().filter(|l| l.starts_with("m=")).count() == 1
                && offer.sdp.lines().any(|l| l.starts_with("m=video ")) { return Ok(offer); }
            return Err("Expected single-video offer".into());
        }
        if Instant::now() >= deadline { return Err("Video negotiation timed out".into()); }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn blank(config: VideoConfig) -> Vec<u8> {
    let mut pixels = vec![0; config.rgba_bytes()];
    for pixel in pixels.chunks_exact_mut(4) { pixel[3] = 255; }
    pixels
}
fn test_frame(config: VideoConfig, n: u32) -> Vec<u8> {
    let mut data = blank(config);
    for (i, pixel) in data.chunks_exact_mut(4).enumerate() {
        pixel[0] = ((i as u32 % config.width + n * 7) % 256) as u8;
        pixel[1] = ((i as u32 / config.width) % 256) as u8;
        pixel[2] = 120;
    }
    data
}
#[cfg(feature = "local-screen")]
fn capture(display: u32, config: VideoConfig) -> MediaResult<Vec<u8>> {
    use xcap::{Monitor, image::{imageops::{self, FilterType}, DynamicImage, Rgba, RgbaImage}};
    let monitor = Monitor::all().map_err(error)?.into_iter()
        .find(|m| m.id().ok() == Some(display)).ok_or("Authorized display disconnected")?;
    let w = monitor.width().map_err(error)?;
    let h = monitor.height().map_err(error)?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 32_000_000 {
        return Err("Display exceeds capture limits".into());
    }
    // The OS must grant its normal capture permission. Do not bypass it.
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" { fn CGPreflightScreenCaptureAccess() -> bool; }
        // SAFETY: parameterless system permission query.
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            return Err("Enable screen recording permission for this process using System Settings".into());
        }
    }
    let image = monitor.capture_image().map_err(error)?;
    if image.width() == 0 || image.height() == 0 || u64::from(image.width()) * u64::from(image.height()) > 32_000_000 {
        return Err("Invalid captured frame".into());
    }
    // Preserve aspect ratio using a black letterbox; do not send coordinates.
    let image = DynamicImage::ImageRgba8(image).resize(config.width, config.height, FilterType::Triangle).into_rgba8();
    let mut result = RgbaImage::from_pixel(config.width, config.height, Rgba([0, 0, 0, 255]));
    let x = i64::from((config.width - image.width()) / 2);
    let y = i64::from((config.height - image.height()) / 2);
    imageops::replace(&mut result, &image, x, y);
    Ok(result.into_raw())
}
#[cfg(not(feature = "local-screen"))]
fn capture(_display: u32, _config: VideoConfig) -> MediaResult<Vec<u8>> {
    Err("Real display capture was not compiled".into())
}

fn host(mut wire: SecureWire, identity: DeviceIdentity, expected: PublicKey, args: Arguments) -> MediaResult<()> {
    let Message::Request { session, controller, nonce, requested } = recv(&mut wire, None)? else { return Err("Expected view request".into()); };
    if controller != expected || requested != VIEW { return Err("View-only peer or grant mismatch".into()); }
    let Message::ViewProfile { synthetic, .. } = recv(&mut wire, Some(session))? else { return Err("Missing view profile".into()); };
    if synthetic != args.profile.synthetic() { return Err("View profile mismatch; no capture".into()); }
    let p = PendingPairing::new(Role::Host, session, &identity, expected, VIEW).map_err(error)?;
    let host_nonce = p.nonce();
    let p = p.approve_local(VIEW).map_err(error)?;
    send(&mut wire, Message::Accept { session, host: identity.public_key(), nonce: host_nonce, granted: VIEW })?;
    send(&mut wire, Message::ViewProfile { session, synthetic })?;

    let config = VideoConfig { width: 960, height: 540, fps: 10, bitrate: 1_500_000 };
    let mut peer = Peer::sender(config, Source::NativeRgba)?;
    peer.start()?;
    // Only black synthetic pixels may enter the pipeline before verification.
    peer.push_rgba(blank(config))?;
    let offer = wait_offer(&peer)?;
    peer.set_description(true, &offer)?;
    send(&mut wire, Message::Offer { session, sdp: offer.sdp.clone() })?;
    let Message::Answer { sdp, .. } = recv(&mut wire, Some(session))? else { return Err("Expected answer".into()); };
    let answer = Description { kind: DescriptionKind::Answer, sdp };
    let signed = p.sign(&identity, nonce, offer.sdp.as_bytes(), answer.sdp.as_bytes()).map_err(error)?;
    send(&mut wire, Message::Proof { session, signature: hex(&signed.signature()) })?;
    let Message::Proof { signature, .. } = recv(&mut wire, Some(session))? else { return Err("Expected signed controller proof".into()); };
    let proof = signed.finish(&decode_hex::<64>(&signature).map_err(error)?,
        offer.sdp.as_bytes(), answer.sdp.as_bytes()).map_err(error)?;
    peer.set_description(false, &answer)?;
    send(&mut wire, Message::Candidates { session, items: gather(&peer)? })?;
    receive_ice(&mut wire, &peer, session)?;
    wait_connected(&peer, &answer.sdp, true)?;
    let permissions = Permissions { view: true, input: false };
    let mut gate = SessionGate::new(session, permissions).map_err(error)?;
    gate.approve_local(permissions).map_err(error)?;
    gate.on_verified_transport(&proof).map_err(error)?;
    if !gate.may_view() { return Err("View permission not active".into()); }
    if recv(&mut wire, Some(session))? != (Message::Ready { session }) { return Err("Peer did not verify video transport".into()); }
    send(&mut wire, Message::Ready { session })?;
    out(if synthetic { "CONNECTED TEST VIEW" } else { "CONNECTED SCREEN VIEW — sharing selected display, Ctrl-C to stop" })?;

    let started = Instant::now();
    let cutoff = started + Duration::from_secs(u64::from(args.profile.seconds()));
    let mut count = 0u32;
    for seq in 1..=args.profile.seconds() {
        let Message::Heartbeat { sequence, .. } = recv(&mut wire, Some(session))? else { return Err("View ended or heartbeat missing".into()); };
        if sequence != seq || Instant::now() >= cutoff { break; }
        ensure_connected(&peer)?;
        send(&mut wire, Message::Heartbeat { session, sequence: seq })?;
        let batch = (Instant::now() + Duration::from_millis(900)).min(cutoff);
        while Instant::now() < batch {
            ensure_connected(&peer)?;
            if !gate.may_view() { return Err("View permission revoked".into()); }
            let next = Instant::now() + Duration::from_millis(100);
            let pixels = if synthetic { test_frame(config, count) }
                else { capture(args.display.ok_or("No authorized display")?, config)? };
            // A capture can take time; never push it after the deadline or a
            // detected connection loss.
            if Instant::now() >= cutoff { break; }
            ensure_connected(&peer)?;
            peer.push_rgba(pixels)?;
            count += 1;
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
    }
    gate.close();
    // Stop the native sender before acknowledging session end.
    drop(peer);
    if recv(&mut wire, Some(session))? != (Message::Finished { session }) { return Err("Expected end of view".into()); }
    out(&format!("PASS HOST VIEW: view-only, sent_frames={count}"))
}
fn controller(mut wire: SecureWire, identity: DeviceIdentity, expected: PublicKey, args: Arguments) -> MediaResult<()> {
    let session = new_session_id();
    let p = PendingPairing::new(Role::Controller, session, &identity, expected, VIEW).map_err(error)?;
    let nonce = p.nonce();
    send(&mut wire, Message::Request { session, controller: identity.public_key(), nonce, requested: VIEW })?;
    send(&mut wire, Message::ViewProfile { session, synthetic: args.profile.synthetic() })?;
    let Message::Accept { host, nonce: host_nonce, granted, .. } = recv(&mut wire, Some(session))? else { return Err("Expected encrypted acceptance".into()); };
    if host != expected || granted != VIEW { return Err("Unexpected host or grant".into()); }
    let Message::ViewProfile { synthetic, .. } = recv(&mut wire, Some(session))? else { return Err("Expected view profile".into()); };
    if synthetic != args.profile.synthetic() { return Err("View profile mismatch".into()); }
    let p = p.approve_local(VIEW).map_err(error)?;
    let config = VideoConfig { width: 960, height: 540, fps: 10, bitrate: 1_500_000 };
    let mut peer = Peer::receiver_with_preview(config, !synthetic)?;
    peer.start()?;
    let Message::Offer { sdp, .. } = recv(&mut wire, Some(session))? else { return Err("Expected offer".into()); };
    let offer = Description { kind: DescriptionKind::Offer, sdp };
    if offer.sdp.lines().filter(|l| l.starts_with("m=")).count() != 1
        || !offer.sdp.lines().any(|l| l.starts_with("m=video ")) { return Err("Expected single-video SDP".into()); }
    peer.set_description(false, &offer)?;
    let answer = peer.create_description(DescriptionKind::Answer)?;
    peer.set_description(true, &answer)?;
    send(&mut wire, Message::Answer { session, sdp: answer.sdp.clone() })?;
    let signed = p.sign(&identity, host_nonce, offer.sdp.as_bytes(), answer.sdp.as_bytes()).map_err(error)?;
    let Message::Proof { signature, .. } = recv(&mut wire, Some(session))? else { return Err("Expected signed host proof".into()); };
    let local_sig = signed.signature();
    let _proof = signed.finish(&decode_hex::<64>(&signature).map_err(error)?, offer.sdp.as_bytes(), answer.sdp.as_bytes()).map_err(error)?;
    send(&mut wire, Message::Proof { session, signature: hex(&local_sig) })?;
    receive_ice(&mut wire, &peer, session)?;
    send(&mut wire, Message::Candidates { session, items: gather(&peer)? })?;
    wait_connected(&peer, &offer.sdp, false)?;
    send(&mut wire, Message::Ready { session })?;
    if recv(&mut wire, Some(session))? != (Message::Ready { session }) { return Err("Host did not verify video transport".into()); }
    out(if synthetic { "CONNECTED TEST VIEW" } else { "CONNECTED SCREEN VIEW — view only, Ctrl-C to disconnect" })?;
    let mut frames = 0;
    for seq in 1..=args.profile.seconds() {
        ensure_connected(&peer)?;
        send(&mut wire, Message::Heartbeat { session, sequence: seq })?;
        if recv(&mut wire, Some(session))? != (Message::Heartbeat { session, sequence: seq }) {
            return Err("Unexpected encrypted heartbeat".into());
        }
        let until = Instant::now() + Duration::from_millis(950);
        while Instant::now() < until {
            if let Some(sample) = peer.pull_frame(Duration::from_millis(40))? {
                let info = Peer::frame_info(&sample)?;
                if info.width != config.width || info.height != config.height {
                    return Err("Unexpected view dimensions".into());
                }
                frames += 1;
            }
        }
    }
    send(&mut wire, Message::Finished { session })?;
    if frames < 5 { return Err("Insufficient decoded frames".into()); }
    out(&format!("PASS CONTROLLER VIEW: verified DTLS, decoded_frames={frames}"))
}
fn run() -> MediaResult<()> {
    let args = arguments()?;
    let (identity, noise, expected, peer_noise) = identities(&args)?;
    let is_controller = matches!(args.mode, Mode::Controller);
    let wire = if is_controller { Wire::connect(args.address).map_err(error)? } else {
        let listener = Wire::bind(args.address).map_err(error)?;
        out(&format!("LISTENING {}", listener.local_addr().map_err(error)?))?;
        Wire::accept_listener(listener).map_err(error)?
    };
    let wire = SecureWire::handshake(wire, noise, peer_noise, is_controller).map_err(error)?;
    if is_controller { controller(wire, identity, expected, args) }
    else { host(wire, identity, expected, args) }
}
fn main() {
    if let Err(err) = run() {
        eprintln!("FAIL: {err}"); std::process::exit(1);
    }
}
