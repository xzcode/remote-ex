//! Local-only two-peer media diagnostic. No external signaling endpoint.
use gstreamer_webrtc::WebRTCPeerConnectionState;
use remote_ex_media::{DescriptionKind, Event, IceCandidate, MediaResult, Peer, Source, VideoConfig};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Pattern, NativePattern, #[cfg(feature = "local-screen")] Screen(u32) }

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

fn synthetic_rgba(config: VideoConfig, n: u32) -> Vec<u8> {
    let mut pixels = vec![0; config.rgba_bytes()];
    for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let x = i as u32 % config.width;
        let y = i as u32 / config.width;
        pixel.copy_from_slice(&[((x + n * 7) % 256) as u8, (y % 256) as u8, 120, 255]);
    }
    pixels
}

#[cfg(feature = "local-screen")]
fn screen_rgba(id: u32, config: VideoConfig) -> MediaResult<Vec<u8>> {
    use xcap::{Monitor, image::{imageops::FilterType, DynamicImage}};
    let monitor = Monitor::all().map_err(|e| e.to_string())?.into_iter()
        .find(|m| m.id().ok() == Some(id)).ok_or("Display disconnected")?;
    let w = monitor.width().map_err(|e| e.to_string())?;
    let h = monitor.height().map_err(|e| e.to_string())?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 32_000_000 {
        return Err("Display exceeds diagnostic bounds".into());
    }
    let image = monitor.capture_image().map_err(|e| e.to_string())?;
    if image.width() == 0 || image.height() == 0
        || u64::from(image.width()) * u64::from(image.height()) > 32_000_000 {
        return Err("Frame exceeds diagnostic bounds".into());
    }
    // Diagnostic scaling only; production must preserve display aspect ratio,
    // native stride, and avoid repeated per-frame monitor enumeration.
    Ok(DynamicImage::ImageRgba8(image)
        .resize_exact(config.width, config.height, FilterType::Triangle)
        .into_rgba8().into_raw())
}

fn frame(mode: Mode, config: VideoConfig, n: u32) -> MediaResult<Vec<u8>> {
    match mode {
        Mode::NativePattern => Ok(synthetic_rgba(config, n)),
        #[cfg(feature = "local-screen")]
        Mode::Screen(id) => screen_rgba(id, config),
        Mode::Pattern => Err("Internal source mismatch".into()),
    }
}

fn run(mode: Mode, preview: bool) -> MediaResult<()> {
    let config = VideoConfig::default();
    let source = if mode == Mode::Pattern { Source::TestPattern } else { Source::NativeRgba };
    let mut sender = Peer::sender(config, source)?;
    let mut receiver = Peer::receiver_with_preview(config, preview)?;
    receiver.start()?;
    sender.start()?;

    // Push only synthetic pixels before negotiation. Real capture is allowed
    // only after both endpoints are connected and local consent was given.
    if source == Source::NativeRgba { sender.push_rgba(synthetic_rgba(config, 0))?; }

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
    let deadline = started + Duration::from_secs(if preview { 30 } else { 25 });
    let mut frames = 0;
    let mut native_frames = 0;
    let mut previous_pts = None;
    let mut first_frame_ms = None;
    let mut next_frame = Instant::now();
    let frame_interval = Duration::from_millis(if preview { 100 } else { 33 });
    let mut passed = false;
    while Instant::now() < deadline {
        forward(&sender, &receiver)?;
        forward(&receiver, &sender)?;
        let connected = sender.connection_state() == WebRTCPeerConnectionState::Connected
            && receiver.connection_state() == WebRTCPeerConnectionState::Connected;
        if source == Source::NativeRgba && Instant::now() >= next_frame {
            let pixels = if connected { frame(mode, config, native_frames)? }
                else { synthetic_rgba(config, native_frames) };
            sender.push_rgba(pixels)?;
            if connected { native_frames += 1; }
            next_frame = Instant::now() + frame_interval;
        }
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
            if connected {
                first_frame_ms.get_or_insert_with(|| started.elapsed().as_millis());
                frames += 1;
            }
            if !passed && frames >= 30 && connected
                && (source == Source::TestPattern || native_frames >= 30)
            {
                println!("PASS: WebRTC VP8, {}x{}, decoded_frames={}, native_input_frames={}, first_frame_after_sdp_ms={}",
                    config.width, config.height, frames, native_frames, first_frame_ms.unwrap_or_default());
                println!("Scope: two peers in one process; not Internet P2P or end-to-end latency.");
                passed = true;
                if !preview { return Ok(()); }
            }
        }
    }
    if passed { return Ok(()); }
    Err(format!("Media timeout: decoded_frames={frames}, sender={:?}, receiver={:?}",
        sender.connection_state(), receiver.connection_state()))
}

fn args() -> MediaResult<(Mode, bool)> {
    let mut mode = Mode::Pattern;
    let mut preview = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--preview" => preview = true,
            "--native-pattern" if mode == Mode::Pattern => mode = Mode::NativePattern,
            #[cfg(feature = "local-screen")]
            "--screen" if mode == Mode::Pattern => {
                let id = args.next().ok_or("Missing display ID")?.parse().map_err(|_| "Invalid display ID")?;
                mode = Mode::Screen(id);
                preview = true;
            }
            _ => return Err("Usage: loopback [--native-pattern] [--preview]; local-screen feature adds --screen DISPLAY_ID".into()),
        }
    }
    #[cfg(feature = "local-screen")]
    if let Mode::Screen(id) = mode {
        use std::io::{self, IsTerminal, Write};
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err("Screen diagnostic requires an interactive local terminal".into());
        }
        print!("Display {id}: preview real screen in a local window for up to 30 seconds. No external signaling or recording. Type CAPTURE to continue: ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut consent = String::new();
        io::stdin().read_line(&mut consent).map_err(|e| e.to_string())?;
        if consent.trim() != "CAPTURE" { return Err("Screen capture canceled".into()); }
    }
    Ok((mode, preview))
}

fn main() {
    if let Err(error) = args().and_then(|(mode, preview)| run(mode, preview)) {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}
