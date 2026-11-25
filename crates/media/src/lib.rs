#![forbid(unsafe_code)]
//! Native video primitives. No public listener, discovery, remote authorization,
//! desktop capture or input injection is provided by this crate.
//! A future authenticated session owner must control creation and shutdown.
use gstreamer as gst;
use gst::prelude::*;
use gstreamer_app as app;
use gstreamer_sdp as sdp;
use gstreamer_webrtc as rtc;
pub mod certificate;
pub mod stats;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
    Arc,
};
use std::time::Duration;

pub type MediaResult<T> = Result<T, String>;
const EVENT_LIMIT: usize = 256;
const SDP_LIMIT: usize = 64 * 1024;
const ICE_LIMIT: usize = 4096;
const PROMISE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
}
impl Default for VideoConfig {
    fn default() -> Self {
        Self { width: 640, height: 360, fps: 30, bitrate: 1_200_000 }
    }
}
impl VideoConfig {
    pub fn validate(self) -> MediaResult<Self> {
        if !(16..=3840).contains(&self.width) || !(16..=2160).contains(&self.height)
            || self.width % 2 != 0 || self.height % 2 != 0
            || !(1..=60).contains(&self.fps) || !(100_000..=20_000_000).contains(&self.bitrate)
        {
            return Err("Unsupported video configuration".into());
        }
        Ok(self)
    }
    pub fn rgba_bytes(self) -> usize {
        self.width as usize * self.height as usize * 4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    TestPattern,
    NativeRgba,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptionKind { Offer, Answer }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    pub kind: DescriptionKind,
    pub sdp: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceCandidate {
    pub mline: u32,
    /// Empty string means end-of-candidates (GStreamer 1.24+).
    pub candidate: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    NegotiationNeeded,
    Ice(IceCandidate),
}

#[derive(Debug, Clone, Copy)]
pub struct FrameInfo {
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    pub pts_ns: Option<u64>,
}

pub struct Peer {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
    input: Option<app::AppSrc>,
    output: Option<app::AppSink>,
    config: VideoConfig,
    events: Receiver<Event>,
    overflow: Arc<AtomicBool>,
    running: bool,
    data_enabled: bool,
}

fn queue_event(sender: &SyncSender<Event>, overflow: &AtomicBool, event: Event) {
    if let Err(TrySendError::Full(_)) = sender.try_send(event) {
        overflow.store(true, Ordering::Release);
    }
}

fn make_pipeline(description: &str) -> MediaResult<gst::Pipeline> {
    gst::parse::launch(description).map_err(|e| e.to_string())?
        .downcast::<gst::Pipeline>().map_err(|_| "Expected GStreamer pipeline".into())
}

fn named(pipeline: &gst::Pipeline, name: &str) -> MediaResult<gst::Element> {
    pipeline.by_name(name).ok_or_else(|| format!("Missing element: {name}"))
}

impl Peer {
    /// Builds a one-way native sender. Callers may supply native tightly packed
    /// RGBA frames; do not push screenshots through the ordinary UI IPC.
    pub fn sender(config: VideoConfig, source: Source) -> MediaResult<Self> {
        gst::init().map_err(|e| e.to_string())?;
        let config = config.validate()?;
        let source_text = match source {
            Source::TestPattern => "videotestsrc is-live=true pattern=ball",
            Source::NativeRgba => "appsrc name=raw is-live=true do-timestamp=true format=time block=false max-buffers=2 max-bytes=0 max-time=0 leaky-type=downstream",
        };
        let caps_format = if source == Source::NativeRgba { "RGBA" } else { "I420" };
        let pipeline = make_pipeline(&format!(
            "webrtcbin name=rtc bundle-policy=max-bundle latency=100 \
             {source_text} ! video/x-raw,format={caps_format},width={w},height={h},framerate={fps}/1 \
             ! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream \
             ! videoconvert ! video/x-raw,format=I420 \
             ! vp8enc name=encoder deadline=1 cpu-used=8 target-bitrate={bitrate} keyframe-max-dist={fps} \
             ! rtpvp8pay pt=96 picture-id-mode=15-bit \
             ! application/x-rtp,media=video,encoding-name=VP8,clock-rate=90000,payload=96 ! rtc.",
            w=config.width, h=config.height, fps=config.fps, bitrate=config.bitrate,
        ))?;
        let input = if source == Source::NativeRgba {
            let raw = named(&pipeline, "raw")?.downcast::<app::AppSrc>()
                .map_err(|_| "Invalid appsrc")?;
            let caps = gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("width", config.width as i32)
                .field("height", config.height as i32)
                .field("framerate", gst::Fraction::new(config.fps as i32, 1))
                .build();
            raw.set_caps(Some(&caps));
            Some(raw)
        } else { None };
        Self::new(pipeline, input, None, config)
    }

    /// Receives VP8 RTP over WebRTC. Decoded frames remain in native GstSample
    /// buffers. Native rendering can consume the sample without serializing it.
    pub fn receiver(config: VideoConfig) -> MediaResult<Self> {
        Self::receiver_with_preview(config, false)
    }

    /// Optional native GStreamer video window; never uses Webview IPC.
    /// The local caller must explicitly request the preview.
    pub fn receiver_with_preview(config: VideoConfig, preview: bool) -> MediaResult<Self> {
        gst::init().map_err(|e| e.to_string())?;
        let config = config.validate()?;
        let output_branch = if preview {
            "tee name=frames frames. ! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream ! appsink name=decoded sync=false max-buffers=2 drop=true frames. ! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream ! videoconvert ! autovideosink sync=false"
        } else {
            "appsink name=decoded sync=false max-buffers=2 drop=true"
        };
        let pipeline = make_pipeline(&format!(
            "webrtcbin name=rtc bundle-policy=max-bundle latency=100 \
             queue name=rx max-size-buffers=12 max-size-bytes=0 max-size-time=0 \
             ! rtpvp8depay ! vp8dec ! videoconvert ! videoscale \
             ! video/x-raw,format=RGBA,width={w},height={h} \
             ! {output_branch}",
            w=config.width, h=config.height
        ))?;
        let out = named(&pipeline, "decoded")?.downcast::<app::AppSink>()
            .map_err(|_| "Invalid appsink")?;
        let mut peer = Self::new(pipeline, None, Some(out), config)?;
        let sink = named(&peer.pipeline, "rx")?.static_pad("sink").ok_or("Missing RTP sink")?;
        let failure = peer.overflow.clone();
        peer.webrtc.connect_pad_added(move |_, pad| {
            if pad.direction() != gst::PadDirection::Src { return; }
            if sink.is_linked() || pad.link(&sink).is_err() {
                failure.store(true, Ordering::Release);
            }
        });
        // Keep the explicit receive branch alive before the first pad is added.
        peer.running = false;
        Ok(peer)
    }

    fn new(
        pipeline: gst::Pipeline, input: Option<app::AppSrc>, output: Option<app::AppSink>,
        config: VideoConfig,
    ) -> MediaResult<Self> {
        let webrtc = named(&pipeline, "rtc")?;
        let (tx, rx) = mpsc::sync_channel(EVENT_LIMIT);
        let overflow = Arc::new(AtomicBool::new(false));
        {
            let tx = tx.clone();
            let overflow = overflow.clone();
            webrtc.connect("on-negotiation-needed", false, move |_| {
                queue_event(&tx, &overflow, Event::NegotiationNeeded);
                None
            });
        }
        {
            let overflow = overflow.clone();
            webrtc.connect("on-ice-candidate", false, move |args| {
                let parsed = args.get(1).and_then(|v| v.get::<u32>().ok())
                    .zip(args.get(2).and_then(|v| v.get::<Option<String>>().ok()).map(|v| v.unwrap_or_default()));
                if let Some((mline, candidate)) = parsed {
                    if candidate.len() <= ICE_LIMIT {
                        queue_event(&tx, &overflow, Event::Ice(IceCandidate { mline, candidate }));
                    } else {
                        overflow.store(true, Ordering::Release);
                    }
                } else {
                    overflow.store(true, Ordering::Release);
                }
                None
            });
        }
        Ok(Self { pipeline, webrtc, input, output, config, events: rx, overflow, running: false, data_enabled: false })
    }

    /// Configure operator-provided ICE servers before starting negotiation.
    /// Default "all" allows direct candidate preference and TURN fallback;
    /// relay-only requires a valid temporary TURN credential.
    pub fn configure_ice(&mut self,config:&remote_ex_rendezvous_protocol::IceConfig,relay_only:bool)->MediaResult<()>{
        use percent_encoding::{utf8_percent_encode,NON_ALPHANUMERIC};
        use zeroize::Zeroizing;
        if self.running{return Err("ICE configuration must precede media start".into());}
        config.validate().map_err(str::to_owned)?;
        if relay_only&&config.turn.is_none(){return Err("Relay-only requires a TURN credential".into());}
        if let Some(stun)=&config.stun{self.webrtc.set_property("stun-server",stun.as_str());}
        if let Some(turn)=&config.turn{
            let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_|"System clock invalid")?.as_secs();
            if turn.expires_at<=now+10||turn.expires_at>now+600{return Err("TURN credential outside accepted lifetime".into());}
            let parsed=remote_ex_rendezvous_protocol::ice_url(&turn.url,&["turn","turns"]).map_err(str::to_owned)?;
            let user=utf8_percent_encode(&turn.username,NON_ALPHANUMERIC);
            let password=utf8_percent_encode(&turn.password,NON_ALPHANUMERIC);
            let uri=Zeroizing::new(format!("{}://{user}:{password}@{}:{}",parsed.scheme(),parsed.host_str().ok_or("Invalid TURN host")?,parsed.port().ok_or("Invalid TURN port")?));
            if !self.webrtc.emit_by_name::<bool>("add-turn-server",&[&uri.as_str()]){
                return Err("Media engine rejected TURN configuration".into());
            }
        }
        let policy=if relay_only{rtc::WebRTCICETransportPolicy::Relay}else{rtc::WebRTCICETransportPolicy::All};
        self.webrtc.set_property("ice-transport-policy",policy);
        Ok(())
    }

    pub fn start(&mut self) -> MediaResult<()> {
        if self.running { return Err("Peer already running".into()); }
        self.pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;
        self.running = true;
        Ok(())
    }

    pub fn config(&self)->VideoConfig{self.config}
    pub fn request_transport_stats(&self)->std::sync::mpsc::Receiver<Option<stats::Counters>>{
        let(tx,rx)=std::sync::mpsc::sync_channel(1);
        let promise=gst::Promise::with_change_func(move|reply|{
            let stats=reply.ok().flatten().map(stats::parse);
            let _=tx.try_send(stats);
        });
        self.webrtc.emit_by_name::<()>("get-stats", &[&None::<gst::Pad>,&promise]);rx
    }
    pub fn set_video_bitrate(&self,bitrate:u32)->MediaResult<()> {
        if !(100_000..=self.config.bitrate).contains(&bitrate){return Err("Invalid adaptive bitrate".into());}
        let encoder=named(&self.pipeline,"encoder")?;
        encoder.set_property("target-bitrate",bitrate as i32);Ok(())
    }
    pub fn connection_state(&self) -> rtc::WebRTCPeerConnectionState {
        self.webrtc.property("connection-state")
    }

    pub fn check_health(&self) -> MediaResult<()> {
        if self.overflow.load(Ordering::Acquire) {
            return Err("Media event overflow or unexpected RTP stream".into());
        }
        if matches!(self.connection_state(), rtc::WebRTCPeerConnectionState::Failed | rtc::WebRTCPeerConnectionState::Closed) {
            return Err("WebRTC peer failed or closed".into());
        }
        if let Some(bus) = self.pipeline.bus() {
            while let Some(message) = bus.pop() {
                use gst::MessageView;
                match message.view() {
                    MessageView::Error(e) => return Err(format!("GStreamer: {}", e.error())),
                    MessageView::Eos(_) => return Err("Unexpected end of media".into()),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub fn drain_events(&self) -> MediaResult<Vec<Event>> {
        self.check_health()?;
        Ok(self.events.try_iter().take(EVENT_LIMIT).collect())
    }

    pub fn create_description(&self, kind: DescriptionKind) -> MediaResult<Description> {
        if !self.running { return Err("Peer is not running".into()); }
        let (signal, field) = match kind {
            DescriptionKind::Offer => ("create-offer", "offer"),
            DescriptionKind::Answer => ("create-answer", "answer"),
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let promise = gst::Promise::with_change_func(move |result| {
            let value = result.map_err(|e| format!("{e:?}")).and_then(|reply| {
                reply.ok_or_else(|| "Missing SDP reply".to_string())?
                    .get::<rtc::WebRTCSessionDescription>(field).map_err(|e| e.to_string())
            }).and_then(|description| description.sdp().as_text().map_err(|e| e.to_string()));
            let _ = tx.try_send(value);
        });
        self.webrtc.emit_by_name::<()>(signal, &[&None::<gst::Structure>, &promise]);
        let text = rx.recv_timeout(PROMISE_TIMEOUT).map_err(|_| "SDP creation timed out")??;
        if text.is_empty() || text.len() > SDP_LIMIT { return Err("SDP exceeds limits".into()); }
        Ok(Description { kind, sdp: text })
    }

    /// Only call with descriptions obtained from a verified, authorized peer.
    /// This primitive does not itself authenticate an SDP fingerprint.
    pub fn set_description(&self, local: bool, description: &Description) -> MediaResult<()> {
        if !self.running || description.sdp.is_empty() || description.sdp.len() > SDP_LIMIT {
            return Err("Invalid description or peer state".into());
        }
        let parsed = sdp::SDPMessage::parse_buffer(description.sdp.as_bytes()).map_err(|e| e.to_string())?;
        let kind = match description.kind {
            DescriptionKind::Offer => rtc::WebRTCSDPType::Offer,
            DescriptionKind::Answer => rtc::WebRTCSDPType::Answer,
        };
        let native = rtc::WebRTCSessionDescription::new(kind, parsed);
        let (tx, rx) = mpsc::sync_channel(1);
        let promise = gst::Promise::with_change_func(move |result| {
            let result = result.map_err(|e| format!("{e:?}")).and_then(|reply| {
                if let Some(reply) = reply {
                    if reply.has_field("error") { return Err(format!("SDP rejected: {reply:?}")); }
                }
                Ok(())
            });
            let _ = tx.try_send(result);
        });
        let signal = if local { "set-local-description" } else { "set-remote-description" };
        self.webrtc.emit_by_name::<()>(signal, &[&native, &promise]);
        rx.recv_timeout(PROMISE_TIMEOUT).map_err(|_| "SDP application timed out")?
    }

    pub fn add_ice(&self, ice: &IceCandidate) -> MediaResult<()> {
        if !self.running { return Err("Peer is not running".into()); }
        validate_ice(ice, if self.data_enabled { 1 } else { 0 })?;
        self.webrtc.emit_by_name::<()>("add-ice-candidate", &[&ice.mline, &ice.candidate]);
        Ok(())
    }

    /// Exactly one externally negotiated, reliable ordered binary channel.
    /// Both endpoints must create it before the initial SDP negotiation.
    /// The owner must withhold all sensitive messages until identity/consent
    /// and actual WebRTC connection have been checked.
    pub fn create_control_channel(&mut self) -> MediaResult<ControlChannel> {
        if !self.running || self.data_enabled { return Err("Invalid control channel state".into()); }
        let options = gst::Structure::builder("application/data-channel")
            .field("ordered", true)
            .field("negotiated", true)
            .field("id", 0i32)
            .field("protocol", "remote-ex-input-v1")
            .build();
        let channel = self.webrtc.emit_by_name::<Option<rtc::WebRTCDataChannel>>(
            "create-data-channel", &[&"remote-ex-control", &options]
        ).ok_or("Cannot create control data channel")?;
        self.data_enabled = true;
        Ok(ControlChannel::new(channel))
    }

    /// Check the certificate on the actual video DTLS transport, independently
    /// of webrtcbin's SDP acceptance. Fail closed on missing certificate,
    /// disconnected transport, unsupported fingerprint, or mismatched DER hash.
    pub fn verify_video_peer(&self, remote_sdp: &str, sending: bool) -> MediaResult<()> {
        if self.connection_state() != rtc::WebRTCPeerConnectionState::Connected {
            return Err("WebRTC is not connected".into());
        }
        let transceiver = self.webrtc.emit_by_name::<Option<rtc::WebRTCRTPTransceiver>>(
            "get-transceiver", &[&0i32]
        ).ok_or("Missing video transceiver")?;
        let transport = if sending {
            let sender = transceiver.property::<Option<rtc::WebRTCRTPSender>>("sender").ok_or("Missing video sender")?;
            sender.property::<Option<rtc::WebRTCDTLSTransport>>("transport")
        } else {
            let receiver = transceiver.property::<Option<rtc::WebRTCRTPReceiver>>("receiver").ok_or("Missing video receiver")?;
            receiver.property::<Option<rtc::WebRTCDTLSTransport>>("transport")
        }.ok_or("Missing video DTLS transport")?;
        if transport.property::<rtc::WebRTCDTLSTransportState>("state") != rtc::WebRTCDTLSTransportState::Connected {
            return Err("Video DTLS is not connected".into());
        }
        let certificate = transport.property::<Option<String>>("remote-certificate").ok_or("Missing peer DTLS certificate")?;
        certificate::verify(remote_sdp, &certificate)?;
        // If a negotiated control channel exists, verify its actual transport
        // too, rather than assuming it shares the video BUNDLE transport.
        if self.data_enabled {
            let sctp = self.webrtc.property::<Option<rtc::WebRTCSCTPTransport>>("sctp-transport").ok_or("Missing SCTP transport")?;
            let dtls = sctp.property::<Option<rtc::WebRTCDTLSTransport>>("transport").ok_or("Missing data DTLS transport")?;
            if dtls.property::<rtc::WebRTCDTLSTransportState>("state") != rtc::WebRTCDTLSTransportState::Connected {
                return Err("Data DTLS is not connected".into());
            }
            let cert = dtls.property::<Option<String>>("remote-certificate").ok_or("Missing data DTLS certificate")?;
            certificate::verify(remote_sdp, &cert)?;
        }
        Ok(())
    }

    pub fn ice_complete(&self) -> bool {
        self.webrtc.property::<rtc::WebRTCICEGatheringState>("ice-gathering-state")
            == rtc::WebRTCICEGatheringState::Complete
    }

    /// Accepts a frame owned by the native capture adapter, with tight RGBA stride.
    /// GStreamer appsrc has a bounded leaky queue; stale frames are dropped.
    pub fn push_rgba(&self, pixels: Vec<u8>) -> MediaResult<()> {
        if !self.running || pixels.len() != self.config.rgba_bytes() {
            return Err("Unexpected frame size or stopped sender".into());
        }
        let input = self.input.as_ref().ok_or("No native frame source")?;
        input.push_buffer(gst::Buffer::from_mut_slice(pixels)).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn pull_frame(&self, timeout: Duration) -> MediaResult<Option<gst::Sample>> {
        self.check_health()?;
        let out = self.output.as_ref().ok_or("No receive sink")?;
        let timeout_ms = timeout.as_millis().min(500) as u64;
        Ok(out.try_pull_sample(gst::ClockTime::from_mseconds(timeout_ms)))
    }

    pub fn frame_info(sample: &gst::Sample) -> MediaResult<FrameInfo> {
        let caps = sample.caps().ok_or("Missing frame caps")?;
        let structure = caps.structure(0).ok_or("Missing frame structure")?;
        let width = structure.get::<i32>("width").map_err(|e| e.to_string())?;
        let height = structure.get::<i32>("height").map_err(|e| e.to_string())?;
        let buffer = sample.buffer().ok_or("Missing frame buffer")?;
        if width <= 0 || height <= 0 || width > 3840 || height > 2160 {
            return Err("Invalid decoded dimensions".into());
        }
        Ok(FrameInfo {
            width: width as u32, height: height as u32, bytes: buffer.size(),
            pts_ns: buffer.pts().map(|t| t.nseconds()),
        })
    }
}

const MAX_CONTROL_BYTES: usize = 1024;
const MAX_CONTROL_BUFFER: u64 = 64 * 1024;

pub struct ControlChannel {
    channel: rtc::WebRTCDataChannel,
    messages: Receiver<Vec<u8>>,
    failed: Arc<AtomicBool>,
}
impl ControlChannel {
    fn new(channel: rtc::WebRTCDataChannel) -> Self {
        let (tx, messages) = mpsc::sync_channel(64);
        let failed = Arc::new(AtomicBool::new(false));
        let bad = failed.clone();
        channel.connect_on_message_data(move |_, bytes| {
            match bytes {
                Some(bytes) if !bytes.is_empty() && bytes.len() <= MAX_CONTROL_BYTES => {
                    if tx.try_send(bytes.as_ref().to_vec()).is_err() {
                        bad.store(true, Ordering::Release);
                    }
                }
                _ => bad.store(true, Ordering::Release),
            }
        });
        let bad = failed.clone();
        channel.connect_on_error(move |_, _| bad.store(true, Ordering::Release));
        let bad = failed.clone();
        channel.connect("on-message-string", false, move |_| {
            bad.store(true, Ordering::Release);
            None
        });
        Self { channel, messages, failed }
    }
    pub fn is_open(&self) -> bool {
        self.channel.ready_state() == rtc::WebRTCDataChannelState::Open
    }
    pub fn check_health(&self) -> MediaResult<()> {
        if self.failed.load(Ordering::Acquire)
            || matches!(self.channel.ready_state(), rtc::WebRTCDataChannelState::Closing | rtc::WebRTCDataChannelState::Closed)
        { return Err("Control channel failed or closed".into()); }
        Ok(())
    }
    pub fn send(&self, data: &[u8]) -> MediaResult<()> {
        self.check_health()?;
        if !self.is_open() || data.is_empty() || data.len() > MAX_CONTROL_BYTES
            || self.channel.buffered_amount().saturating_add(data.len() as u64) > MAX_CONTROL_BUFFER
        { return Err("Control channel unavailable or buffer limit exceeded".into()); }
        let bytes = gst::glib::Bytes::from(data);
        self.channel.send_data_full(Some(&bytes)).map_err(|e| e.to_string())
    }
    pub fn receive(&self) -> MediaResult<Option<Vec<u8>>> {
        self.check_health()?;
        match self.messages.try_recv() {
            Ok(bytes) => Ok(Some(bytes)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("Control channel receiver closed".into()),
        }
    }
}
impl Drop for ControlChannel {
    fn drop(&mut self) { self.channel.close(); }
}

fn validate_ice(ice: &IceCandidate, max_mline: u32) -> MediaResult<()> {
    if ice.mline > max_mline || ice.candidate.len() > ICE_LIMIT {
        return Err(format!("Invalid ICE bounds: mline={}, bytes={}", ice.mline, ice.candidate.len()));
    }
    // Empty (or NULL normalized to empty) explicitly marks gathering complete.
    if ice.candidate.contains(['\r', '\n', '\0']) {
        return Err("Invalid ICE candidate characters".into());
    }
    Ok(())
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ice_completion_and_bounds() {
        assert!(validate_ice(&IceCandidate { mline: 0, candidate: String::new() }, 0).is_ok());
        assert!(validate_ice(&IceCandidate { mline: 1, candidate: String::new() }, 0).is_err());
        assert!(validate_ice(&IceCandidate { mline: 0, candidate: "x".repeat(ICE_LIMIT + 1) }, 0).is_err());
        assert!(validate_ice(&IceCandidate { mline: 0, candidate: "bad\nline".into() }, 0).is_err());
    }
    #[test]
    fn config_bounds() {
        assert!(VideoConfig::default().validate().is_ok());
        assert!(VideoConfig { width: 0, ..VideoConfig::default() }.validate().is_err());
        assert!(VideoConfig { width: 641, ..VideoConfig::default() }.validate().is_err());
        assert!(VideoConfig { fps: 61, ..VideoConfig::default() }.validate().is_err());
    }
}
