//! Two real GStreamer WebRTC endpoints with in-process signaling.
//! No screen capture, public signaling endpoint, external STUN or TURN.
use gstreamer_webrtc::WebRTCPeerConnectionState;
use remote_ex_media::{DescriptionKind, Event, IceCandidate, MediaResult, Peer, Source, VideoConfig};
use std::time::{Duration, Instant};

fn take_ice(peer: &Peer, pending: &mut Vec<IceCandidate>) -> MediaResult<bool> {
    let mut negotiation = false;
    for event in peer.drain_events()? {
        match event {
            Event::NegotiationNeeded => negotiation = true,
            Event::Ice(ice) => {
                if pending.len() >= 256 { return Err("Too many pending ICE candidates".into()); }
                pending.push(ice);
            }
        }
    }
    Ok(negotiation)
}

fn forward(from: &Peer, to: &Peer) -> MediaResult<()> {
    for event in from.drain_events()? {
        if let Event::Ice(ice) = event { to.add_ice(&ice)?; }
    }
    Ok(())
}

fn run() -> MediaResult<()> {
    let config = VideoConfig::default();
    let mut sender = Peer::sender(config, Source::TestPattern)?;
    let mut receiver = Peer::receiver(config)?;
    receiver.start()?;
    sender.start()?;

    let mut sender_ice = Vec::new();
    let mut receiver_ice = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if take_ice(&sender, &mut sender_ice)? { break; }
        take_ice(&receiver, &mut receiver_ice)?;
        if Instant::now() >= deadline { return Err("No negotiation event".into()); }
        std::thread::sleep(Duration::from_millis(10));
    }
    let offer = sender.create_description(DescriptionKind::Offer)?;
    sender.set_description(true, &offer)?;
    receiver.set_description(false, &offer)?;
    let answer = receiver.create_description(DescriptionKind::Answer)?;
    receiver.set_description(true, &answer)?;
    sender.set_description(false, &answer)?;
    for ice in sender_ice { receiver.add_ice(&ice)?; }
    for ice in receiver_ice { sender.add_ice(&ice)?; }

    let started = Instant::now();
    let deadline = started + Duration::from_secs(25);
    let mut frames = 0;
    let mut previous_pts = None;
    let mut first_frame_ms = None;
    while Instant::now() < deadline {
        forward(&sender, &receiver)?;
        forward(&receiver, &sender)?;
        if let Some(sample) = receiver.pull_frame(Duration::from_millis(20))? {
            let info = Peer::frame_info(&sample)?;
            if info.width != config.width || info.height != config.height
                || info.bytes < config.rgba_bytes()
            {
                return Err(format!("Unexpected decoded frame: {info:?}"));
            }
            if let (Some(previous), Some(current)) = (previous_pts, info.pts_ns) {
                if current <= previous { return Err("Non-increasing frame timestamps".into()); }
            }
            previous_pts = info.pts_ns;
            first_frame_ms.get_or_insert_with(|| started.elapsed().as_millis());
            frames += 1;
            if frames >= 30
                && sender.connection_state() == WebRTCPeerConnectionState::Connected
                && receiver.connection_state() == WebRTCPeerConnectionState::Connected
            {
                println!("PASS: WebRTC VP8, {}x{}, decoded_frames={}, first_frame_after_sdp_ms={}",
                    config.width, config.height, frames, first_frame_ms.unwrap_or_default());
                println!("Scope: two peers in one process; not desktop capture, Internet P2P, or end-to-end latency.");
                return Ok(());
            }
        }
    }
    Err(format!("Media timeout: decoded_frames={frames}, sender={:?}, receiver={:?}",
        sender.connection_state(), receiver.connection_state()))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}
