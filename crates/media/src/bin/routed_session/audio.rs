//! Explicit one-way audio preview. PCM travels inside the authenticated
//! WebRTC SCTP channel; independent of any screen or system-input grant.
use super::*;
use gstreamer as gst;
use gst::prelude::*;
use gstreamer_app as app;
pub(super) const PCM:usize=640; // 20 ms, 16 kHz mono signed 16-bit LE
const DATA:u8=0xA1;
const END:&[u8]=b"remote-ex-audio/end/v1";
const STOP_AUDIO:&[u8]=b"remote-ex-audio/stop/v1";
const DONE:&[u8]=b"remote-ex-audio/done/v1";
pub(super) const HOST_DONE:&[u8]=b"remote-ex-audio/host-done/v1";
pub(super) static VOLUME:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(75);
struct Pipeline(gst::Pipeline);
impl Drop for Pipeline{fn drop(&mut self){let _=self.0.set_state(gst::State::Null);}}
impl Pipeline{
    fn new(description:&str)->MediaResult<Self>{
        let p=gst::parse::launch(description).map_err(error)?.downcast::<gst::Pipeline>().map_err(|_|"Invalid audio pipeline")?;Ok(Self(p))
    }
    fn start(&self)->MediaResult<()>{self.0.set_state(gst::State::Playing).map_err(error)?;Ok(())}
    fn health(&self)->MediaResult<()>{
        if let Some(bus)=self.0.bus(){while let Some(m)=bus.pop(){if let gst::MessageView::Error(_)=m.view(){return Err("Audio device or pipeline failed".into());}}}
        Ok(())
    }
}
/// Created only after the signed profile and local consent are verified.
/// No microphone fallback and no helper is started by synthetic tests.
pub(super) enum Capture{
    Gst(Pipeline,app::AppSink),
    #[cfg(target_os="macos")]
    Mac(MacCapture),
}
impl Capture{
    pub(super) fn new(test:bool)->MediaResult<Self>{Self::with_limit(test,false)}
    pub(super) fn with_limit(test:bool,extended:bool)->MediaResult<Self>{
        if canceled(){return Err("Audio grant revoked".into());}
        #[cfg(target_os="macos")]
        if !test{return Ok(Self::Mac(MacCapture::new(extended)?));}
        let _=extended;
        let source=if test{"audiotestsrc is-live=true samplesperbuffer=320 wave=sine"}else{
            if !cfg!(target_os="windows"){return Err("System loopback capture unavailable".into());}
            "wasapi2src loopback=true low-latency=true"
        };
        let p=Pipeline::new(&format!("{source} ! audioconvert ! audioresample ! audio/x-raw,format=S16LE,rate=16000,channels=1,layout=interleaved ! audiobuffersplit output-buffer-duration=1/50 ! appsink name=pcm sync=false max-buffers=4 drop=true"))?;
        let sink=p.0.by_name("pcm").ok_or("Missing audio sink")?.downcast::<app::AppSink>().map_err(|_|"Invalid audio sink")?;
        p.start()?;Ok(Self::Gst(p,sink))
    }
    pub(super) fn poll(&mut self)->MediaResult<Option<Vec<u8>>>{
        if canceled(){return Err("Audio grant revoked".into());}
        match self{
            Self::Gst(p,sink)=>{
                p.health()?;
                if let Some(sample)=sink.try_pull_sample(gst::ClockTime::ZERO){
                    let b=sample.buffer().ok_or("Missing audio sample")?;
                    let map=b.map_readable().map_err(|_|"Cannot map audio sample")?;
                    if map.size()!=PCM{return Err("Unexpected audio frame size".into());}
                    return Ok(Some(map.as_slice().to_vec()));
                }
                Ok(None)
            },
            #[cfg(target_os="macos")]
            Self::Mac(c)=>c.poll(),
        }
    }
}
#[cfg(target_os="macos")]
struct MacCapture{
    child:std::process::Child,
    consent:Option<std::process::ChildStdin>,
    samples:std::sync::mpsc::Receiver<Vec<u8>>,
}
#[cfg(target_os="macos")]
impl MacCapture{
    fn new(extended:bool)->MediaResult<Self>{
        use std::process::{Command,Stdio};
        let exe=std::env::current_exe().map_err(error)?;
        let helper=exe.parent().ok_or("Missing app directory")?.join("remote-ex-mac-audio");
        if !helper.is_file(){return Err("macOS system audio helper is missing from this development package".into());}
        let mut child=Command::new(helper).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_|"Cannot start macOS system audio helper")?;
        let Some(mut consent)=child.stdin.take()else{let _=child.kill();let _=child.wait();return Err("Missing local audio consent pipe".into());};
        let Some(mut output)=child.stdout.take()else{let _=child.kill();let _=child.wait();return Err("Missing audio pipe".into());};
        let command:&[u8]=if extended{b"CAPTURE SYSTEM AUDIO EXTENDED\n"}else{b"CAPTURE SYSTEM AUDIO\n"};
        if consent.write_all(command).and_then(|_|consent.flush()).is_err(){
            let _=child.kill();let _=child.wait();return Err("Could not deliver audio consent".into());
        }
        let(tx,samples)=std::sync::mpsc::sync_channel(4);
        std::thread::spawn(move||{
            loop{
                let mut frame=vec![0;PCM];
                if output.read_exact(&mut frame).is_err(){break;}
                match tx.try_send(frame){
                    Ok(())|Err(std::sync::mpsc::TrySendError::Full(_))=>{},
                    Err(std::sync::mpsc::TrySendError::Disconnected(_))=>break,
                }
            }
        });
        Ok(Self{child,consent:Some(consent),samples})
    }
    fn poll(&mut self)->MediaResult<Option<Vec<u8>>>{
        if self.child.try_wait().map_err(error)?.is_some(){return Err("macOS system audio capture stopped (check Screen Recording permission and macOS version)".into());}
        match self.samples.try_recv(){
            Ok(frame)=>Ok(Some(frame)),
            Err(std::sync::mpsc::TryRecvError::Empty)=>Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected)=>Err("macOS audio pipe stopped".into()),
        }
    }
}
#[cfg(target_os="macos")]
impl Drop for MacCapture{
    fn drop(&mut self){
        self.consent.take();let _=self.child.kill();let _=self.child.wait();
    }
}
pub(super) async fn send(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,test:bool)->MediaResult<()>{
    if canceled(){return Err("Audio grant revoked".into());}
    let mut capture=Capture::new(test)?;
    out("CONNECTED ROUTED AUDIO")?;
    let started=Instant::now();let mut count=0u32;let mut last=started;
    loop{
        connected(peer,channel)?;
        if canceled()||started.elapsed()>=Duration::from_secs(if test{3}else{30}){break;}
        if let Some(packet)=channel.receive()?{if packet==STOP_AUDIO{break;}return Err("Unexpected audio control".into());}
        if let Some(pcm)=capture.poll()?{
            count=count.checked_add(1).ok_or("Audio sequence overflow")?;
            let mut packet=Vec::with_capacity(PCM+5);packet.push(DATA);packet.extend_from_slice(&count.to_be_bytes());packet.extend_from_slice(&pcm);
            channel.send(&packet)?;last=Instant::now();
        }
        if last.elapsed()>Duration::from_secs(3){return Err("Audio capture stalled".into());}
        tick(conn).await?;
    }
    drop(capture);
    channel.send(END)?;
    // Receiver may send STOP while the sender reaches its deadline.
    let deadline=Instant::now()+Duration::from_secs(4);
    loop{connected(peer,channel)?;if let Some(packet)=channel.receive()?{if packet==DONE{break;}if packet!=STOP_AUDIO{return Err("Unexpected audio completion".into());}}
        if Instant::now()>=deadline{return Err("Audio stop not acknowledged".into());}tick(conn).await?;}
    if test&&count<50{return Err("Insufficient synthetic audio frames".into());}
    out(&format!("AUDIO_SENT {count}"))
}
pub(super) async fn play(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,test:bool,volume:u8)->MediaResult<()>{
    if canceled(){return Err("Audio grant revoked".into());}
    let sink=if test{"fakesink sync=false"}else{"autoaudiosink sync=false"};
    let p=Pipeline::new(&format!("appsrc name=pcm is-live=true format=time do-timestamp=true block=false max-buffers=8 leaky-type=downstream ! audio/x-raw,format=S16LE,rate=16000,channels=1,layout=interleaved ! audioconvert ! audioresample ! volume name=gain ! {sink}"))?;
    let appsrc=p.0.by_name("pcm").ok_or("Missing audio appsrc")?.downcast::<app::AppSrc>().map_err(|_|"Invalid audio appsrc")?;
    let gain=p.0.by_name("gain").ok_or("Missing volume")?;
    let mut actual_volume=volume;gain.set_property("volume",f64::from(actual_volume)/100.0);
    appsrc.set_caps(Some(&gst::Caps::builder("audio/x-raw").field("format","S16LE").field("rate",16000i32).field("channels",1i32).field("layout","interleaved").build()));
    p.start()?;out("CONNECTED ROUTED AUDIO")?;
    let deadline=Instant::now()+Duration::from_secs(35);let mut seq=0u32;let mut nonzero=false;let mut stopped=false;let mut last=Instant::now();
    loop{
        connected(peer,channel)?;p.health()?;
        if canceled()&&!stopped{channel.send(STOP_AUDIO)?;stopped=true;gain.set_property("volume",0.0f64);}
        let requested=VOLUME.load(Ordering::Acquire);
        if !stopped&&requested!=actual_volume{actual_volume=requested;gain.set_property("volume",f64::from(actual_volume)/100.0);out(&format!("AUDIO_VOLUME {actual_volume}"))?;}
        for _ in 0..16{
            let Some(packet)=channel.receive()? else{break;};
            if packet==END{
                drop(p);channel.send(DONE)?;
                if test&&(seq<50||!nonzero){return Err("Synthetic audio verification failed".into());}
                return out(&format!("AUDIO_RECEIVED {seq}"));
            }
            if packet.len()!=PCM+5||packet[0]!=DATA{return Err("Invalid audio packet".into());}
            let received=u32::from_be_bytes(packet[1..5].try_into().map_err(error)?);
            if seq.checked_add(1)!=Some(received){return Err("Audio sequence mismatch".into());}
            seq=received;nonzero|=packet[5..].iter().any(|b|*b!=0);last=Instant::now();
            if !stopped{appsrc.push_buffer(gst::Buffer::from_mut_slice(packet[5..].to_vec())).map_err(error)?;}
        }
        if Instant::now()>=deadline||last.elapsed()>Duration::from_secs(5){return Err("Audio stream timed out".into());}
        tick(conn).await?;
    }
}
