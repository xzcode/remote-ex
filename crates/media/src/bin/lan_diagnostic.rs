//! Explicit single-connection diagnostic for two independent processes.
//! Only synthetic video and fixed input TEST messages; NEVER captures a
//! remote desktop or invokes the operating system's input APIs.
use gstreamer_webrtc::WebRTCPeerConnectionState;
use prost::Message as ProstMessage;
use remote_ex_core::{Action, Permissions, SessionGate};
use remote_ex_media::{ControlChannel, Description, DescriptionKind, Event, IceCandidate, MediaResult, Peer, Source, VideoConfig};
use remote_ex_protocol::{Input, InputEnvelope, Key, PointerMove, ReleaseAll, VERSION, input_envelope};
use remote_ex_security::{DeviceIdentity, PublicKey, Role, VerifiedSession, pairing::{PendingPairing, new_session_id}, session::{VIEW, INPUT}};
use remote_ex_signaling::{Candidate, Message, Wire, decode_hex, hex, MAX_CANDIDATES};
use std::{io::{self, Write}, net::SocketAddr, time::{Duration, Instant}};

const GRANT: u8 = VIEW | INPUT;
const ACK: &[u8] = b"remote-ex-diagnostic-ok-v1";
fn err<T: std::fmt::Debug>(e: T) -> String { format!("{e:?}") }
fn out(line: &str) -> MediaResult<()> {
    println!("{line}");
    io::stdout().flush().map_err(|e| e.to_string())
}
fn line() -> MediaResult<String> {
    let mut text = String::new();
    io::stdin().read_line(&mut text).map_err(|e| e.to_string())?;
    Ok(text.trim().to_owned())
}
fn send(wire: &mut Wire, message: Message) -> MediaResult<()> {
    wire.send(message).map_err(err)
}
fn recv(wire: &mut Wire, session: Option<[u8;16]>) -> MediaResult<Message> {
    wire.recv(session).map_err(err)
}

fn local_identity() -> MediaResult<(DeviceIdentity, PublicKey)> {
    let identity = DeviceIdentity::generate();
    out(&format!("KEY {}", hex(&identity.public_key())))?;
    out("PEER_KEY (compare the full public key independently; paste it here)")?;
    let peer = decode_hex::<32>(&line()?).map_err(err)?;
    if peer == [0; 32] || peer == identity.public_key() { return Err("Invalid peer key".into()); }
    out("CONSENT (synthetic video and simulated input only; type ALLOW DIAGNOSTIC)")?;
    if line()? != "ALLOW DIAGNOSTIC" { return Err("Local operator declined".into()); }
    Ok((identity, peer))
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
fn receive_ice(wire: &mut Wire, peer: &Peer, session: [u8;16]) -> MediaResult<()> {
    let Message::Candidates { items, .. } = recv(wire, Some(session))? else {
        return Err("Expected candidates".into());
    };
    for item in items { peer.add_ice(&IceCandidate { mline: item.mline, candidate: item.candidate })?; }
    Ok(())
}
fn wait_ready(peer: &Peer, channel: &ControlChannel) -> MediaResult<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        peer.check_health()?;
        channel.check_health()?;
        if peer.connection_state() == WebRTCPeerConnectionState::Connected && channel.is_open() {
            return Ok(());
        }
        if Instant::now() >= deadline { return Err(format!("Connection timed out: {:?}", peer.connection_state())); }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn wait_offer(peer: &Peer) -> MediaResult<Description> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let offer = peer.create_description(DescriptionKind::Offer)?;
        if offer.sdp.lines().any(|l| l.starts_with("m=video "))
            && offer.sdp.lines().any(|l| l.starts_with("m=application ")) {
            return Ok(offer);
        }
        if Instant::now() >= deadline { return Err("Missing video/data SDP sections".into()); }
        std::thread::sleep(Duration::from_millis(40));
    }
}
fn activate_gate(session: [u8;16], proof: &VerifiedSession) -> MediaResult<SessionGate> {
    let permissions = Permissions { view: true, input: true };
    let mut gate = SessionGate::new(session, permissions).map_err(err)?;
    gate.approve_local(permissions).map_err(err)?;
    gate.on_verified_transport(proof).map_err(err)?;
    Ok(gate)
}
fn encoded(session: [u8;16], sequence: u64, event: input_envelope::Event) -> Vec<u8> {
    InputEnvelope { version: VERSION, session_id: session.to_vec(), sequence, event: Some(event) }.encode_to_vec()
}
fn host(mut wire: Wire, id: DeviceIdentity, expected: PublicKey) -> MediaResult<()> {
    let Message::Request { session, controller, nonce, requested } = recv(&mut wire, None)? else {
        return Err("Expected request".into());
    };
    if controller != expected || requested != GRANT { return Err("Unexpected identity or permissions".into()); }
    let p = PendingPairing::new(Role::Host, session, &id, expected, requested).map_err(err)?;
    let host_nonce = p.nonce();
    let p = p.approve_local(GRANT).map_err(err)?;
    send(&mut wire, Message::Accept { session, host: id.public_key(), nonce: host_nonce, granted: GRANT })?;

    let mut peer = Peer::sender(VideoConfig::default(), Source::TestPattern)?;
    peer.start()?;
    let channel = peer.create_control_channel()?;
    let offer = wait_offer(&peer)?;
    peer.set_description(true, &offer)?;
    send(&mut wire, Message::Offer { session, sdp: offer.sdp.clone() })?;
    let Message::Answer { sdp, .. } = recv(&mut wire, Some(session))? else { return Err("Expected answer".into()); };
    let answer = Description { kind: DescriptionKind::Answer, sdp };
    let signed = p.sign(&id, nonce, offer.sdp.as_bytes(), answer.sdp.as_bytes()).map_err(err)?;
    send(&mut wire, Message::Proof { session, signature: hex(&signed.signature()) })?;
    let Message::Proof { signature, .. } = recv(&mut wire, Some(session))? else { return Err("Expected peer proof".into()); };
    let proof = signed.finish(&decode_hex::<64>(&signature).map_err(err)?,
        offer.sdp.as_bytes(), answer.sdp.as_bytes()).map_err(err)?;
    // Only apply the remote SDP after its exact bytes and peer key are verified.
    peer.set_description(false, &answer)?;
    let items = gather(&peer)?;
    send(&mut wire, Message::Candidates { session, items })?;
    receive_ice(&mut wire, &peer, session)?;
    wait_ready(&peer, &channel)?;
    let mut gate = activate_gate(session, &proof)?;

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut count = 0;
    while count < 3 {
        peer.check_health()?;
        if Instant::now() >= deadline { return Err("Test input timed out".into()); }
        if let Some(data) = channel.receive()? {
            let action = gate.handle_input(&data).map_err(err)?;
            // Diagnostic only. No OS injector exists here.
            match (count, action) {
                (0, Action::Inject(Input::Move { x: 100, y: 200 })) => {}
                (1, Action::Inject(Input::Key { usage: 4, down: true })) => {}
                (2, Action::Release(released)) if released.keys == vec![4] && released.buttons.is_empty() => {}
                _ => return Err("Unexpected simulated input".into()),
            }
            count += 1;
        } else { std::thread::sleep(Duration::from_millis(10)); }
    }
    channel.send(ACK)?;
    let Message::Finished { .. } = recv(&mut wire, Some(session))? else { return Err("Expected finished".into()); };
    let releases = gate.close();
    if !releases.keys.is_empty() || !releases.buttons.is_empty() { return Err("Diagnostic release mismatch".into()); }
    out("PASS HOST: authenticated SDP transcript, reliable data channel, 3 simulated input messages")
}
fn controller(mut wire: Wire, id: DeviceIdentity, expected: PublicKey) -> MediaResult<()> {
    let session = new_session_id();
    let p = PendingPairing::new(Role::Controller, session, &id, expected, GRANT).map_err(err)?;
    let nonce = p.nonce();
    send(&mut wire, Message::Request { session, controller: id.public_key(), nonce, requested: GRANT })?;
    let Message::Accept { host, nonce: host_nonce, granted, .. } = recv(&mut wire, Some(session))? else {
        return Err("Expected acceptance".into());
    };
    if host != expected || granted != GRANT { return Err("Unexpected host or grant".into()); }
    let p = p.approve_local(granted).map_err(err)?;

    let mut peer = Peer::receiver(VideoConfig::default())?;
    peer.start()?;
    let channel = peer.create_control_channel()?;
    let Message::Offer { sdp, .. } = recv(&mut wire, Some(session))? else { return Err("Expected offer".into()); };
    let offer = Description { kind: DescriptionKind::Offer, sdp };
    // Answer generation requires applying the offer first. No real screen,
    // sensitive data, input dispatcher or ICE exchange is enabled here.
    peer.set_description(false, &offer)?;
    let answer = peer.create_description(DescriptionKind::Answer)?;
    peer.set_description(true, &answer)?;
    send(&mut wire, Message::Answer { session, sdp: answer.sdp.clone() })?;
    let signed = p.sign(&id, host_nonce, offer.sdp.as_bytes(), answer.sdp.as_bytes()).map_err(err)?;
    let Message::Proof { signature, .. } = recv(&mut wire, Some(session))? else { return Err("Expected host proof".into()); };
    let signature_local = signed.signature();
    let _proof = signed.finish(&decode_hex::<64>(&signature).map_err(err)?,
        offer.sdp.as_bytes(), answer.sdp.as_bytes()).map_err(err)?;
    send(&mut wire, Message::Proof { session, signature: hex(&signature_local) })?;
    receive_ice(&mut wire, &peer, session)?;
    let items = gather(&peer)?;
    send(&mut wire, Message::Candidates { session, items })?;
    wait_ready(&peer, &channel)?;

    use input_envelope::Event as InputEvent;
    channel.send(&encoded(session, 1, InputEvent::PointerMove(PointerMove { x: 100, y: 200 })))?;
    channel.send(&encoded(session, 2, InputEvent::Key(Key { usage: 4, down: true })))?;
    channel.send(&encoded(session, 3, InputEvent::ReleaseAll(ReleaseAll {})))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut frames = 0;
    let mut ack = false;
    while frames < 30 || !ack {
        if Instant::now() >= deadline { return Err(format!("Diagnostic timed out: frames={frames}, ack={ack}")); }
        if let Some(data) = channel.receive()? {
            if data != ACK || ack { return Err("Invalid diagnostic acknowledgement".into()); }
            ack = true;
        }
        if let Some(sample) = peer.pull_frame(Duration::from_millis(25))? {
            let info = Peer::frame_info(&sample)?;
            if info.width != 640 || info.height != 360 { return Err("Wrong frame dimensions".into()); }
            frames += 1;
        }
    }
    send(&mut wire, Message::Finished { session })?;
    out(&format!("PASS CONTROLLER: {frames} decoded frames and verified simulated input acknowledgement"))
}

fn run() -> MediaResult<()> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().ok_or("Usage: lan_diagnostic --listen|--connect IP:PORT")?;
    let address: SocketAddr = args.next().ok_or("Missing address")?.parse().map_err(err)?;
    if args.next().is_some() || !matches!(mode.as_str(), "--listen" | "--connect") {
        return Err("Usage: lan_diagnostic --listen|--connect IP:PORT".into());
    }
    let (id, expected) = local_identity()?;
    if mode == "--listen" {
        let listener = Wire::bind(address).map_err(err)?;
        out(&format!("LISTENING {}", listener.local_addr().map_err(err)?))?;
        host(Wire::accept_listener(listener).map_err(err)?, id, expected)
    } else {
        let wire = Wire::connect(address).map_err(err)?;
        controller(wire, id, expected)
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}
