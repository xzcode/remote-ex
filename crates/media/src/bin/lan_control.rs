//! Explicit, encrypted one-shot LAN control. Test mode is inert. Native mode
//! requires both visible local terminals, explicit consent, a primary display
//! and ordinary OS permissions. No unattended or privileged mode.
use gstreamer_webrtc::WebRTCPeerConnectionState;
use prost::Message as ProstMessage;
use remote_ex_core::{Permissions,SessionGate};
use remote_ex_input::{Backend,Dispatcher,DisplayArea,InputResult};
use remote_ex_media::{ControlChannel,Description,DescriptionKind,Event,IceCandidate,MediaResult,Peer,Source,VideoConfig};
use remote_ex_protocol::{InputEnvelope,Key,PointerMove,VERSION,input_envelope::Event as InputEvent};
use remote_ex_security::{DeviceIdentity,PublicKey,Role,pairing::{PendingPairing,new_session_id},session::{VIEW,INPUT}};
use remote_ex_signaling::{Candidate,Message,Wire,decode_hex,hex,MAX_CANDIDATES,secure::{NoiseIdentity,SecureWire}};
use std::{io::{self,Write,IsTerminal},net::SocketAddr,sync::atomic::{AtomicBool,Ordering},time::{Duration,Instant}};
#[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
#[path="control_window/mod.rs"]
mod control_window;
static CANCEL:AtomicBool=AtomicBool::new(false);
fn canceled()->bool{CANCEL.load(Ordering::Acquire)}
const GRANT:u8=VIEW|INPUT;
const CONFIG:VideoConfig=VideoConfig{width:960,height:540,fps:10,bitrate:1_500_000};
const HEARTBEAT:&[u8]=b"remote-ex-control/heartbeat/v1";
const STOP:&[u8]=b"remote-ex-control/stop/v1";
const ACK:&[u8]=b"remote-ex-control/released/v1";
fn error<T:std::fmt::Debug>(e:T)->String{format!("{e:?}")}
fn out(s:&str)->MediaResult<()>{println!("{s}");io::stdout().flush().map_err(error)}
fn input()->MediaResult<String>{let mut s=String::new();io::stdin().read_line(&mut s).map_err(error)?;Ok(s.trim().into())}
fn send(w:&mut SecureWire,m:Message)->MediaResult<()>{w.send(m).map_err(error)}
fn recv(w:&mut SecureWire,id:Option<[u8;16]>)->MediaResult<Message>{w.recv(id).map_err(error)}
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

fn valid_sdp(s:&str)->bool{s.lines().filter(|l|l.starts_with("m=")).count()==2&&s.lines().any(|l|l.starts_with("m=video "))&&s.lines().any(|l|l.starts_with("m=application "))}
fn offer(peer:&Peer)->MediaResult<Description>{
    let deadline=Instant::now()+Duration::from_secs(8);
    loop{let d=peer.create_description(DescriptionKind::Offer)?;if valid_sdp(&d.sdp){return Ok(d);}
        if Instant::now()>=deadline{return Err("Missing video/data SDP".into());}
        std::thread::sleep(Duration::from_millis(40));}
}
fn ready(peer:&Peer,channel:&ControlChannel,sdp:&str,sending:bool)->MediaResult<()>{
    let deadline=Instant::now()+Duration::from_secs(15);
    loop{peer.check_health()?;channel.check_health()?;
        if peer.connection_state()==WebRTCPeerConnectionState::Connected&&channel.is_open(){return peer.verify_video_peer(sdp,sending);}
        if Instant::now()>=deadline{return Err("Control transport timeout".into());}
        std::thread::sleep(Duration::from_millis(10));}
}
fn connected(peer:&Peer,channel:&ControlChannel)->MediaResult<()>{
    peer.check_health()?;channel.check_health()?;
    if canceled()||peer.connection_state()!=WebRTCPeerConnectionState::Connected||!channel.is_open(){return Err("Control disconnected".into());}Ok(())
}
fn frame(n:u32)->Vec<u8>{let mut p=vec![0;CONFIG.rgba_bytes()];for(i,px)in p.chunks_exact_mut(4).enumerate(){px[0]=((i as u32%CONFIG.width+n*7)%256)as u8;px[1]=((i as u32/CONFIG.width)%256)as u8;px[2]=120;px[3]=255;}p}
#[derive(Default)]
struct Inert{position:(i32,i32),events:std::rc::Rc<std::cell::RefCell<Vec<String>>>}
impl Backend for Inert{
    fn check_display(&mut self,_:DisplayArea)->InputResult<()>{Ok(())}
    fn move_to(&mut self,x:i32,y:i32)->InputResult<()>{self.position=(x,y);self.events.borrow_mut().push("move".into());Ok(())}
    fn position(&mut self)->InputResult<(i32,i32)>{Ok(self.position)}
    fn key(&mut self,k:u16,down:bool)->InputResult<()>{self.events.borrow_mut().push(format!("key:{k}:{down}"));Ok(())}
    fn button(&mut self,b:u8,down:bool)->InputResult<()>{self.events.borrow_mut().push(format!("button:{b}:{down}"));Ok(())}
    fn scroll_steps(&mut self,_:i32,_:i32)->InputResult<()>{Ok(())}
}
fn serve<B:Backend,F:FnMut(u32)->MediaResult<Vec<u8>>>(peer:&Peer,channel:&ControlChannel,mut d:Dispatcher<B>,mut frame:F,test:bool)->MediaResult<(u32,u32)>{
    let started=Instant::now();let mut last=started;let mut next=started;let mut frames=0;let mut events=0;
    let result=(||{loop{
        connected(peer,channel)?;
        if started.elapsed()>=Duration::from_secs(if test{12}else{30})||last.elapsed()>Duration::from_secs(2){return Err("Control deadline or heartbeat lost".into());}
        for _ in 0..64{
            let Some(data)=channel.receive()? else{break};
            if data==HEARTBEAT{last=Instant::now();}else if data==STOP{return Ok(());}else{d.dispatch(&data)?;events+=1;}
        }
        if Instant::now()>=next{
            let pixels=frame(frames)?;
            connected(peer,channel)?;
            if started.elapsed()>=Duration::from_secs(if test{12}else{30})||last.elapsed()>Duration::from_secs(2){return Err("Control deadline during capture".into());}
            peer.push_rgba(pixels)?;frames+=1;next=Instant::now()+Duration::from_millis(100);
        }
        std::thread::sleep(Duration::from_millis(5));
    }})();
    d.close()?;result?;channel.send(ACK)?;Ok((frames,events))
}
fn host(mut wire:SecureWire,id:DeviceIdentity,expected:PublicKey,test:bool,display:Option<u32>)->MediaResult<()>{
    let Message::Request{session,controller,nonce,requested}=recv(&mut wire,None)? else{return Err("Expected request".into())};
    if controller!=expected||requested!=GRANT{return Err("Wrong identity/grant".into());}
    if recv(&mut wire,Some(session))?!=(Message::ViewProfile{session,synthetic:test}){return Err("Control profile mismatch".into());}
    let p=PendingPairing::new(Role::Host,session,&id,expected,GRANT).map_err(error)?;
    let hn=p.nonce();let p=p.approve_local(GRANT).map_err(error)?;
    send(&mut wire,Message::Accept{session,host:id.public_key(),nonce:hn,granted:GRANT})?;
    send(&mut wire,Message::ViewProfile{session,synthetic:test})?;
    let mut peer=Peer::sender(CONFIG,Source::NativeRgba)?;peer.start()?;peer.push_rgba(frame(0))?;
    let channel=peer.create_control_channel()?;
    let offer=offer(&peer)?;peer.set_description(true,&offer)?;
    send(&mut wire,Message::Offer{session,sdp:offer.sdp.clone()})?;
    let Message::Answer{sdp,..}=recv(&mut wire,Some(session))? else{return Err("Expected answer".into())};
    if !valid_sdp(&sdp){return Err("Invalid answer profile".into());}
    let answer=Description{kind:DescriptionKind::Answer,sdp};
    let signed=p.sign(&id,nonce,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    send(&mut wire,Message::Proof{session,signature:hex(&signed.signature())})?;
    let Message::Proof{signature,..}=recv(&mut wire,Some(session))? else{return Err("Missing controller proof".into())};
    let proof=signed.finish(&decode_hex::<64>(&signature).map_err(error)?,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    peer.set_description(false,&answer)?;
    send(&mut wire,Message::Candidates{session,items:gather(&peer)?})?;
    receive_ice(&mut wire,&peer,session)?;
    ready(&peer,&channel,&answer.sdp,true)?;
    let permissions=Permissions{view:true,input:true};let mut gate=SessionGate::new(session,permissions).map_err(error)?;
    gate.approve_local(permissions).map_err(error)?;gate.on_verified_transport(&proof).map_err(error)?;
    let (frames,events)=if test{
        let area=DisplayArea{x:0,y:0,width:960,height:540};
        send(&mut wire,Geometry::from_area(area)?.message(session))?;
        exchange_ready(&mut wire,session)?;
        out("CONNECTED TEST CONTROL")?;
        let backend=Inert::default();let log=backend.events.clone();
        let d=Dispatcher::new(gate,backend,area)?;
        let result=serve(&peer,&channel,d,|n|Ok(frame(n)),true)?;
        if log.borrow().as_slice()!=["move","key:4:true","key:4:false"]{return Err("Inert input/release verification failed".into());}
        result
    }else{
        #[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
        {real_host(&peer,&channel,gate,&mut wire,session,display.ok_or("Missing display ID")?)?}
        #[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
        {let _=display;return Err("Native control unavailable".into());}
    };
    if recv(&mut wire,Some(session))?!=(Message::Finished{session}){return Err("Missing finish confirmation".into());}
    out(&format!("PASS HOST CONTROL: sent_frames={frames}, dispatched_events={events}, released"))
}
fn controller(mut wire:SecureWire,id:DeviceIdentity,expected:PublicKey,test:bool)->MediaResult<()>{
    let session=new_session_id();let p=PendingPairing::new(Role::Controller,session,&id,expected,GRANT).map_err(error)?;
    let nonce=p.nonce();send(&mut wire,Message::Request{session,controller:id.public_key(),nonce,requested:GRANT})?;
    send(&mut wire,Message::ViewProfile{session,synthetic:test})?;
    let Message::Accept{host,nonce:hn,granted,..}=recv(&mut wire,Some(session))? else{return Err("Expected acceptance".into())};
    if host!=expected||granted!=GRANT{return Err("Wrong host/grant".into());}
    if recv(&mut wire,Some(session))?!=(Message::ViewProfile{session,synthetic:test}){return Err("Test profile mismatch".into());}
    let p=p.approve_local(GRANT).map_err(error)?;
    let mut peer=Peer::receiver(CONFIG)?;peer.start()?;let channel=peer.create_control_channel()?;
    let Message::Offer{sdp,..}=recv(&mut wire,Some(session))? else{return Err("Expected offer".into())};
    if !valid_sdp(&sdp){return Err("Invalid offer profile".into());}
    let offer=Description{kind:DescriptionKind::Offer,sdp};peer.set_description(false,&offer)?;
    let answer=peer.create_description(DescriptionKind::Answer)?;peer.set_description(true,&answer)?;
    send(&mut wire,Message::Answer{session,sdp:answer.sdp.clone()})?;
    let signed=p.sign(&id,hn,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    let Message::Proof{signature,..}=recv(&mut wire,Some(session))? else{return Err("Missing host proof".into())};
    let local=signed.signature();
    let _proof=signed.finish(&decode_hex::<64>(&signature).map_err(error)?,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    send(&mut wire,Message::Proof{session,signature:hex(&local)})?;
    receive_ice(&mut wire,&peer,session)?;send(&mut wire,Message::Candidates{session,items:gather(&peer)?})?;
    ready(&peer,&channel,&offer.sdp,false)?;
    let Message::ControlGeometry{x,y,width,height,..}=recv(&mut wire,Some(session))? else{return Err("Missing control geometry".into())};
    let geometry=Geometry{x,y,width,height};
    send(&mut wire,Message::Ready{session})?;
    if recv(&mut wire,Some(session))?!=(Message::Ready{session}){return Err("Host not ready".into());}
    out(if test{"CONNECTED TEST CONTROL"}else{"CONNECTED CONTROL — Escape disconnects"})?;
    let frames=if test{
    channel.send(HEARTBEAT)?;
    for(sequence,event)in [(1,InputEvent::PointerMove(PointerMove{x:100,y:200})),(2,InputEvent::Key(Key{usage:4,down:true}))]{
        channel.send(&InputEnvelope{version:VERSION,session_id:session.to_vec(),sequence,event:Some(event)}.encode_to_vec())?;
    }
    let deadline=Instant::now()+Duration::from_secs(9);let mut frames=0;let mut heartbeat=Instant::now();
    while frames<30{connected(&peer,&channel)?;if Instant::now()>=deadline{return Err("Decoded video timeout".into());}
        if heartbeat.elapsed()>=Duration::from_millis(300){channel.send(HEARTBEAT)?;heartbeat=Instant::now();}
        if let Some(sample)=peer.pull_frame(Duration::from_millis(40))?{let f=Peer::frame_info(&sample)?;
            if f.width!=CONFIG.width||f.height!=CONFIG.height{return Err("Unexpected dimensions".into());}frames+=1;}
    }
    // Key intentionally still held in the inert backend; STOP must release it.
    frames
    }else{
        #[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
        {control_window::run(&peer,&channel,session,geometry)?}
        #[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
        {let _=geometry;return Err("Native control unavailable".into());}
    };
    channel.send(STOP)?;let until=Instant::now()+Duration::from_secs(3);
    loop{peer.check_health()?;if let Some(b)=channel.receive()?{if b==ACK{break;}else{return Err("Unexpected response".into());}}
        if Instant::now()>=until{return Err("Release ACK timeout".into());}std::thread::sleep(Duration::from_millis(10));}
    send(&mut wire,Message::Finished{session})?;out(&format!("PASS CONTROLLER CONTROL: decoded_frames={frames}, release_ack=true"))
}
fn run()->MediaResult<()>{
    let mut a=std::env::args().skip(1);
    let host=match a.next().as_deref(){Some("--listen")=>true,Some("--connect")=>false,_=>return Err("Use --listen or --connect".into())};
    let address:SocketAddr=a.next().ok_or("Missing address")?.parse().map_err(error)?;
    let(test,display)=match(host,a.next().as_deref()){
        (_,Some("--test"))=>(true,None),
        (true,Some("--control"))=>(false,Some(a.next().ok_or("Choose primary display ID")?.parse::<u32>().map_err(error)?)),
        (false,Some("--control"))=>(false,None),
        _=>return Err("Use --test or --control (host requires display ID)".into()),
    };
    if a.next().is_some()||!remote_ex_signaling::local_address(address){return Err("Specific private address required".into());}
    if !test{
        if !cfg!(all(feature="native-control",any(target_os="windows",target_os="macos"))){return Err("Native control not compiled for this platform".into());}
        if !io::stdin().is_terminal()||!io::stdout().is_terminal(){return Err("Real control requires visible interactive local terminals".into());}
    }
    ctrlc::set_handler(||CANCEL.store(true,Ordering::Release)).map_err(error)?;
    let id=DeviceIdentity::generate();let noise=NoiseIdentity::generate().map_err(error)?;
    let mut bundle=id.public_key().to_vec();bundle.extend_from_slice(&noise.public_key());
    out(&format!("KEY {}",hex(&bundle)))?;out("PEER_KEY (verify complete key bundle)")?;
    let b=decode_hex::<64>(&input()?).map_err(error)?;
    let expected:PublicKey=b[..32].try_into().map_err(error)?;let pn:[u8;32]=b[32..].try_into().map_err(error)?;
    if expected==[0;32]||pn==[0;32]||expected==id.public_key()||pn==noise.public_key(){return Err("Invalid peer keys".into());}
    let consent=if test{"ALLOW TEST CONTROL"}else if host{
        out(&format!("Share primary display {:?} AND permit the verified peer to use keyboard/mouse for up to 30 seconds. Keep terminal visible; Ctrl-C stops.",display))?;
        "ALLOW SCREEN AND INPUT"
    }else{
        out("Control the authorized primary display. Input only inside the focused viewer. Escape disconnects.")?;
        "ALLOW REMOTE INPUT"
    };
    out(&format!("CONSENT (type {consent})"))?;
    if input()?!=consent||canceled(){return Err("Consent denied".into());}
    let wire=if host{let l=Wire::bind(address).map_err(error)?;out(&format!("LISTENING {}",l.local_addr().map_err(error)?))?;Wire::accept_listener(l).map_err(error)?}else{Wire::connect(address).map_err(error)?};
    let wire=SecureWire::handshake(wire,noise,pn,!host).map_err(error)?;
    if host{crate::host(wire,id,expected,test,display)}else{controller(wire,id,expected,test)}
}
fn main(){if let Err(e)=run(){eprintln!("FAIL: {e}");std::process::exit(1);}}

#[derive(Clone,Copy,Debug)]
struct Geometry{x:u32,y:u32,width:u32,height:u32}
impl Geometry{
    fn from_area(area:DisplayArea)->MediaResult<Self>{
        area.validate()?;
        let(w,h)=if u64::from(area.width)*u64::from(CONFIG.height)>=u64::from(area.height)*u64::from(CONFIG.width){
            (CONFIG.width,(u64::from(area.height)*u64::from(CONFIG.width)/u64::from(area.width))as u32)
        }else{((u64::from(area.width)*u64::from(CONFIG.height)/u64::from(area.height))as u32,CONFIG.height)};
        if w<2||h<2{return Err("Unsupported display geometry".into());}
        Ok(Self{x:(CONFIG.width-w)/2,y:(CONFIG.height-h)/2,width:w,height:h})
    }
    fn message(self,session:[u8;16])->Message{Message::ControlGeometry{session,x:self.x,y:self.y,width:self.width,height:self.height}}
}
fn exchange_ready(wire:&mut SecureWire,session:[u8;16])->MediaResult<()>{
    if recv(wire,Some(session))?!=(Message::Ready{session}){return Err("Controller not ready".into());}
    send(wire,Message::Ready{session})
}
#[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
fn real_host(peer:&Peer,channel:&ControlChannel,gate:SessionGate,wire:&mut SecureWire,session:[u8;16],display:u32)->MediaResult<(u32,u32)>{
    use remote_ex_input::native::NativeBackend;
    use xcap::{Monitor,image::{DynamicImage,RgbaImage,Rgba,imageops::{self,FilterType}}};
    #[cfg(target_os="macos")]
    {
        #[link(name="CoreGraphics",kind="framework")]
        extern "C"{fn CGPreflightScreenCaptureAccess()->bool;}
        #[link(name="ApplicationServices",kind="framework")]
        extern "C"{fn AXIsProcessTrusted()->bool;}
        if !unsafe{CGPreflightScreenCaptureAccess()&&AXIsProcessTrusted()}{
            return Err("Grant Screen Recording and Accessibility in System Settings for this process".into());
        }
    }
    let backend=NativeBackend::new()?;let area=backend.primary_area()?;let geom=Geometry::from_area(area)?;
    let get_monitor=||->MediaResult<Monitor>{
        let m=Monitor::all().map_err(error)?.into_iter().find(|m|m.id().ok()==Some(display)).ok_or("Authorized display disconnected")?;
        if !m.is_primary().map_err(error)?||m.width().map_err(error)?!=area.width||m.height().map_err(error)?!=area.height{
            return Err("Only matching primary-display coordinates are supported".into());
        }Ok(m)
    };
    get_monitor()?;
    let d=Dispatcher::new(gate,backend,area)?;
    send(wire,geom.message(session))?;exchange_ready(wire,session)?;
    out("CONNECTED CONTROL — keyboard/mouse authorized, Ctrl-C stops")?;
    serve(peer,channel,d,|_|{
        let m=get_monitor()?;
        let image=m.capture_image().map_err(error)?;
        if image.width()==0||image.height()==0||u64::from(image.width())*u64::from(image.height())>32_000_000||
            u64::from(image.width())*u64::from(area.height)!=u64::from(image.height())*u64::from(area.width){
            return Err("Capture geometry changed or exceeds limits".into());
        }
        let image=DynamicImage::ImageRgba8(image).resize_exact(geom.width,geom.height,FilterType::Triangle).into_rgba8();
        let mut canvas=RgbaImage::from_pixel(CONFIG.width,CONFIG.height,Rgba([0,0,0,255]));
        imageops::replace(&mut canvas,&image,i64::from(geom.x),i64::from(geom.y));Ok(canvas.into_raw())
    },false)
}
