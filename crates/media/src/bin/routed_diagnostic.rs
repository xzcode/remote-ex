//! M2.3 WSS-routed synthetic media test. No native capture, native input,
//! unattended service, automatic acceptance or remote system command.
use gstreamer_webrtc::WebRTCPeerConnectionState;
use prost::Message as _;
use remote_ex_core::{Permissions, SessionGate};
use remote_ex_input::{Backend, Dispatcher, DisplayArea, InputResult};
use remote_ex_media::{ControlChannel, Description, DescriptionKind, Event, IceCandidate, MediaResult, Peer, Source, VideoConfig};
use remote_ex_protocol::{InputEnvelope, Key, PointerMove, VERSION, input_envelope::Event as InputEvent};
use remote_ex_rendezvous_client::{Connection, Endpoint};
use remote_ex_rendezvous_protocol::{Client, Server, Signal, IceItem};
use remote_ex_security::{DeviceIdentity, PublicKey, Role, VerifiedSession, fingerprint, pairing::{OutgoingPairing, PendingPairing}, session::{VIEW, INPUT}};
use remote_ex_signaling::{decode_hex, hex};
use std::{io::{self, Read, Write}, time::{Duration, Instant}};
const CONFIG: VideoConfig = VideoConfig{width:640,height:360,fps:10,bitrate:1_000_000};
const GRANT:u8=VIEW|INPUT;
const READY:&[u8]=b"remote-ex-routed-test/ready/v1";
const READY_ACK:&[u8]=b"remote-ex-routed-test/ready-ack/v1";
const STOP:&[u8]=b"remote-ex-routed-test/stop/v1";
const ACK:&[u8]=b"remote-ex-routed-test/released/v1";

fn error<T:std::fmt::Debug>(e:T)->String{format!("{e:?}")}
fn out(s:&str)->MediaResult<()>{println!("{s}");io::stdout().flush().map_err(error)}
fn input()->MediaResult<String>{let mut s=String::new();io::stdin().read_line(&mut s).map_err(error)?;Ok(s.trim().into())}
fn canceled()->bool{false} // bounded synthetic-only one-shot, no live OS input
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
    if peer.connection_state()!=WebRTCPeerConnectionState::Connected||!channel.is_open(){return Err("Control disconnected".into());}Ok(())
}

fn frame(n:u32)->Vec<u8>{let mut p=vec![0;CONFIG.rgba_bytes()];for(i,px)in p.chunks_exact_mut(4).enumerate(){px[0]=((i as u32%CONFIG.width+n*7)%256)as u8;px[1]=((i as u32/CONFIG.width)%256)as u8;px[2]=120;px[3]=255;}p}
#[derive(Default)]
struct Inert{position:(i32,i32),events:std::rc::Rc<std::cell::RefCell<Vec<String>>>}
impl Backend for Inert{
    fn check_display(&mut self,_:DisplayArea)->InputResult<()>{Ok(())}
    fn move_to(&mut self,x:i32,y:i32)->InputResult<()>{self.position=(x,y);self.events.borrow_mut().push("move".into());Ok(())}
    fn position(&mut self)->InputResult<(i32,i32)>{Ok(self.position)}
    fn key(&mut self,k:u16,down:bool)->InputResult<()>{
        self.events.borrow_mut().push(format!("key:{k}:{down}"));
        // Only emitted by the inert test backend, never logs real user input.
        if down { out("TEST_INPUT_HELD")?; }
        Ok(())
    }
    fn button(&mut self,b:u8,down:bool)->InputResult<()>{self.events.borrow_mut().push(format!("button:{b}:{down}"));Ok(())}
    fn scroll_steps(&mut self,_:i32,_:i32)->InputResult<()>{Ok(())}
}
fn serve<B:Backend,F:FnMut(u32)->MediaResult<Option<Vec<u8>>>>(peer:&Peer,channel:&ControlChannel,mut d:Dispatcher<B>,mut frame:F,test:bool)->MediaResult<(u32,u32)>{
    let started=Instant::now();let mut last=started;let mut next=started;let mut frames=0;let mut events=0;let mut last_frame=started;
    let result: MediaResult<()>=(||{loop{
        if canceled(){return Ok(());}
        connected(peer,channel)?;
        if started.elapsed()>=Duration::from_secs(if test{12}else{30})||last.elapsed()>Duration::from_secs(2){return Err("Control deadline or heartbeat lost".into());}
        for _ in 0..64{
            if canceled(){return Ok(());}
            connected(peer,channel)?;
            let Some(data)=channel.receive()? else{break};
            if data==HEARTBEAT{last=Instant::now();}else if data==STOP{return Ok(());}else{d.dispatch(&data)?;events+=1;}
        }
        if Instant::now()>=next{
            if let Some(pixels)=frame(frames)? {
                if canceled(){return Ok(());}
                connected(peer,channel)?;
                if started.elapsed()>=Duration::from_secs(if test{12}else{30})||last.elapsed()>Duration::from_secs(2){return Err("Control deadline during capture".into());}
                peer.push_rgba(pixels)?;frames+=1;last_frame=Instant::now();
            }
            next=Instant::now()+Duration::from_millis(25);
        }
        if last_frame.elapsed()>Duration::from_secs(2){
            return Err("Capture stalled; stopping input".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }})();
    d.close()?;
    out("INPUT_RELEASED")?;
    result?;
    // Both a remote STOP and local consent revocation end input before ACK.
    channel.send(ACK)?;Ok((frames,events))
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
        let result=serve(&peer,&channel,d,|n|Ok(Some(frame(n))),true)?;
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
fn poll_release(channel:&ControlChannel)->MediaResult<bool>{
    match channel.receive()?{
        None=>Ok(false),
        Some(b) if b==ACK=>Ok(true),
        _=>Err("Unexpected control response".into()),
    }
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
    let (frames,peer_released)=if test{
    channel.send(HEARTBEAT)?;
    for(sequence,event)in [(1,InputEvent::PointerMove(PointerMove{x:100,y:200})),(2,InputEvent::Key(Key{usage:4,down:true}))]{
        channel.send(&InputEnvelope{version:VERSION,session_id:session.to_vec(),sequence,event:Some(event)}.encode_to_vec())?;
    }
    let deadline=Instant::now()+Duration::from_secs(9);let mut frames=0;let mut heartbeat=Instant::now();
    let mut peer_released=false;
    while frames<30 && !canceled(){
        if poll_release(&channel)?{peer_released=true;break;}
        connected(&peer,&channel)?;
        if Instant::now()>=deadline{return Err("Decoded video timeout".into());}
        if heartbeat.elapsed()>=Duration::from_millis(300){channel.send(HEARTBEAT)?;heartbeat=Instant::now();}
        if let Some(sample)=peer.pull_frame(Duration::from_millis(40))?{let f=Peer::frame_info(&sample)?;
            if f.width!=CONFIG.width||f.height!=CONFIG.height{return Err("Unexpected dimensions".into());}frames+=1;}
    }
    // Key intentionally still held in the inert backend; STOP or host
    // revocation must release it. Early exit is only allowed on revoke/ACK.
    (frames,peer_released)
    }else{
        #[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
        {control_window::run(&peer,&channel,session,geometry)?}
        #[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
        {let _=geometry;return Err("Native control unavailable".into());}
    };
    if !peer_released {
        channel.send(STOP)?;
        let until=Instant::now()+Duration::from_secs(3);
        loop{
            peer.check_health()?;
            if poll_release(&channel)?{break;}
            if Instant::now()>=until{return Err("Release ACK timeout".into());}
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    out("INPUT_RELEASED")?;
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
    let supervised=match a.next().as_deref(){
        None=>false,Some("--desktop-session")=>true,_=>return Err("Unexpected argument".into())
    };
    if a.next().is_some()||!remote_ex_signaling::local_address(address){return Err("Specific private address required".into());}
    if supervised && (io::stdin().is_terminal()||io::stdout().is_terminal()){
        return Err("Supervised control requires local supervisor pipes".into());
    }
    if !test{
        if !cfg!(all(feature="native-control",any(target_os="windows",target_os="macos"))){return Err("Native control not compiled for this platform".into());}
        if !supervised && (!io::stdin().is_terminal()||!io::stdout().is_terminal()){return Err("Real control requires explicit local consent in a visible terminal or desktop UI".into());}
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
    if supervised {
        // A closed parent pipe or any additional byte revokes local consent.
        // Never process::exit from this thread; the input owner must release.
        std::thread::spawn(||{
            use std::io::Read;
            let mut b=[0u8;1];
            let _=io::stdin().lock().read(&mut b);
            CANCEL.store(true,Ordering::Release);
        });
    }
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
    let get_monitor=move ||->MediaResult<Monitor>{
        let m=Monitor::all().map_err(error)?.into_iter().find(|m|m.id().ok()==Some(display)).ok_or("Authorized display disconnected")?;
        if !m.is_primary().map_err(error)?||m.width().map_err(error)?!=area.width||m.height().map_err(error)?!=area.height{
            return Err("Only matching primary-display coordinates are supported".into());
        }Ok(m)
    };
    get_monitor()?;
    let d=Dispatcher::new(gate,backend,area)?;
    send(wire,geom.message(session))?;exchange_ready(wire,session)?;
    out("CONNECTED CONTROL — keyboard/mouse authorized, Ctrl-C stops")?;
    // A blocked native screenshot cannot hold the input dispatcher hostage.
    // Only the main owner has the OS input backend. The capture worker sends
    // bounded frames and never injects or releases input. Start after Ready.
    use std::sync::{Arc,mpsc::{sync_channel,TrySendError,TryRecvError}};
    let (tx,rx)=sync_channel::<MediaResult<Vec<u8>>>(1);
    let stop=Arc::new(AtomicBool::new(false));
    let worker_stop=stop.clone();
    std::thread::spawn(move || {
        while !canceled()&&!worker_stop.load(Ordering::Acquire){
            let next=Instant::now()+Duration::from_millis(100);
            let result=(||->MediaResult<Vec<u8>>{
                let m=get_monitor()?;
                let image=m.capture_image().map_err(error)?;
                if image.width()==0||image.height()==0||u64::from(image.width())*u64::from(image.height())>32_000_000||
                    u64::from(image.width())*u64::from(area.height)!=u64::from(image.height())*u64::from(area.width){
                    return Err("Capture geometry changed or exceeds limits".into());
                }
                let image=DynamicImage::ImageRgba8(image).resize_exact(geom.width,geom.height,FilterType::Triangle).into_rgba8();
                let mut canvas=RgbaImage::from_pixel(CONFIG.width,CONFIG.height,Rgba([0,0,0,255]));
                imageops::replace(&mut canvas,&image,i64::from(geom.x),i64::from(geom.y));
                Ok(canvas.into_raw())
            })();
            if canceled()||worker_stop.load(Ordering::Acquire){break;}
            let failed=result.is_err();
            match tx.try_send(result){
                Ok(())=>{},
                Err(TrySendError::Full(_)) if !failed=>{},
                _=>break,
            }
            if failed{break;}
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
        }
    });
    let result=serve(peer,channel,d,|_|match rx.try_recv(){
        Ok(frame)=>frame.map(Some),
        Err(TryRecvError::Empty)=>Ok(None),
        Err(TryRecvError::Disconnected)=>Err("Capture worker stopped".into()),
    },false);
    stop.store(true,Ordering::Release);
    result
}

async fn recv(conn:&mut Connection)->MediaResult<Server>{
    conn.receive(Duration::from_secs(20)).await
}
async fn send_signal(conn:&mut Connection,session:&str,payload:Signal)->MediaResult<()>{
    conn.send(Client::Signal{session:session.to_owned(),payload}).await
}
async fn signal(conn:&mut Connection,session:&str)->MediaResult<Signal>{
    match recv(conn).await?{
        Server::Signal{session:id,payload} if id==session=>Ok(payload),
        _=>Err("Unexpected routed signal or session closed".into()),
    }
}
async fn ice(peer:&Peer,conn:&mut Connection,session:&str)->MediaResult<()>{
    let deadline=Instant::now()+Duration::from_secs(8);
    let mut items=Vec::new();
    loop{
        for event in peer.drain_events()?{
            if let Event::Ice(c)=event{
                if items.len()>=128{return Err("ICE batch count exceeded".into());}
                items.push(IceItem{mline:c.mline,candidate:c.candidate});
            }
        }
        if peer.ice_complete(){
            tokio::time::sleep(Duration::from_millis(30)).await;
            for event in peer.drain_events()?{
                if let Event::Ice(c)=event{
                    if items.len()>=128{return Err("ICE batch count exceeded".into());}
                    items.push(IceItem{mline:c.mline,candidate:c.candidate});
                }
            }
            break;
        }
        if Instant::now()>=deadline{return Err("ICE gathering timeout".into());}
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    send_signal(conn,session,Signal::IceBatch{items}).await?;
    let Signal::IceBatch{items}=signal(conn,session).await? else{return Err("Expected ICE batch".into());};
    for item in items{peer.add_ice(&IceCandidate{mline:item.mline,candidate:item.candidate})?;}
    Ok(())
}
fn gate(proof:&VerifiedSession)->MediaResult<SessionGate>{
    let permissions=Permissions{view:true,input:true};
    let mut gate=SessionGate::new(proof.transcript().session_id,permissions).map_err(error)?;
    gate.approve_local(permissions).map_err(error)?;
    gate.on_verified_transport(proof).map_err(error)?;
    Ok(gate)
}

// Poll the authenticated data channel while also watching the rendezvous
// connection. A close/error stops this synthetic session; no reconnect.
async fn tick(conn:&mut Connection)->MediaResult<()>{
    tokio::select!{
        biased;
        event=conn.receive(Duration::from_secs(5))=>{
            let _=event?;
            Err("Routed session interrupted".into())
        }
        _=tokio::time::sleep(Duration::from_millis(10))=>Ok(()),
    }
}
async fn wait_data(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,expected:&[u8])->MediaResult<()>{
    let deadline=Instant::now()+Duration::from_secs(5);
    while Instant::now()<deadline{
        connected(peer,channel)?;
        if let Some(bytes)=channel.receive()?{
            return if bytes==expected{Ok(())}else{Err("Unexpected test data".into())};
        }
        tick(conn).await?;
    }
    Err("Test data timeout".into())
}
async fn finish(conn:&mut Connection,session:&str)->MediaResult<()>{
    match recv(conn).await?{
        Server::Closed{session:id,reason} if id==session && reason=="closed"=>Ok(()),
        _=>Err("Expected explicit route closure".into()),
    }
}

async fn host(conn:&mut Connection,id:&DeviceIdentity,expected:PublicKey)->MediaResult<()>{
    let Server::Incoming{session,controller,controller_key,requested,controller_nonce}=recv(conn).await?
        else{return Err("Expected incoming device request".into());};
    if controller!=hex(&fingerprint(&expected)) || decode_hex::<32>(&controller_key).map_err(error)?!=expected || requested!=GRANT{
        conn.send(Client::Decision{session,grant:None,host_nonce:None}).await?;
        return Err("Unknown peer or unexpected test grant".into());
    }
    let sid=decode_hex::<16>(&session).map_err(error)?;
    let nonce=decode_hex::<32>(&controller_nonce).map_err(error)?;
    let pending=PendingPairing::new(Role::Host,sid,id,expected,GRANT).map_err(error)?;
    let host_nonce=hex(&pending.nonce());
    let approved=pending.approve_local(GRANT).map_err(error)?;
    conn.send(Client::Decision{session:session.clone(),grant:Some(GRANT),host_nonce:Some(host_nonce)}).await?;

    let mut peer=Peer::sender(CONFIG,Source::NativeRgba)?;
    peer.start()?;peer.push_rgba(frame(0))?;
    let channel=peer.create_control_channel()?;
    let offer=offer(&peer)?;
    peer.set_description(true,&offer)?;
    send_signal(conn,&session,Signal::Offer{sdp:offer.sdp.clone()}).await?;
    let Signal::Answer{sdp}=signal(conn,&session).await? else{return Err("Expected answer".into());};
    if !valid_sdp(&sdp){return Err("Unexpected answer profile".into());}
    let answer=Description{kind:DescriptionKind::Answer,sdp};
    let signed=approved.sign(id,nonce,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    send_signal(conn,&session,Signal::Proof{signature:hex(&signed.signature())}).await?;
    let Signal::Proof{signature}=signal(conn,&session).await? else{return Err("Expected peer proof".into());};
    let proof=signed.finish(&decode_hex::<64>(&signature).map_err(error)?,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    peer.set_description(false,&answer)?;
    ice(&peer,conn,&session).await?;
    ready(&peer,&channel,&answer.sdp,true)?;
    let backend=Inert::default();let log=backend.events.clone();
    let mut dispatcher=Dispatcher::new(gate(&proof)?,backend,DisplayArea{x:0,y:0,width:CONFIG.width,height:CONFIG.height})?;
    wait_data(&peer,&channel,conn,READY).await?;
    channel.send(READY_ACK)?;
    out("CONNECTED ROUTED TEST")?;
    let result:MediaResult<u32>=async{
        let deadline=Instant::now()+Duration::from_secs(12);
        let mut frames=0;let mut events=0;let mut next=Instant::now();
        while Instant::now()<deadline{
            connected(&peer,&channel)?;
            for _ in 0..16{
                let Some(bytes)=channel.receive()? else{break;};
                if bytes==STOP{
                    if events!=2{return Err("Expected exactly two simulated input events".into());}
                    return Ok(frames);
                }
                if events>=2{return Err("Unexpected extra simulated input".into());}
                dispatcher.dispatch(&bytes)?;
                events+=1;
            }
            if Instant::now()>=next{
                peer.push_rgba(frame(frames))?;frames+=1;
                next=Instant::now()+Duration::from_millis(100);
            }
            tick(conn).await?;
        }
        Err("Routed test deadline".into())
    }.await;
    dispatcher.close()?;
    let frames=result?;
    if log.borrow().as_slice()!=["move","key:4:true","key:4:false"]{return Err("Inert release verification failed".into());}
    channel.send(ACK)?;
    finish(conn,&session).await?;
    out(&format!("PASS ROUTED HOST: sent_frames={frames}, input_released=true"))
}
async fn controller(conn:&mut Connection,id:&DeviceIdentity,expected:PublicKey)->MediaResult<()>{
    let target=hex(&fingerprint(&expected));
    let request=OutgoingPairing::new(id,expected,GRANT).map_err(error)?;
    conn.send(Client::Request{target:target.clone(),requested:GRANT,controller_nonce:hex(&request.nonce())}).await?;
    let Server::Requested{session,target:returned}=recv(conn).await? else{return Err("Device request failed".into());};
    if returned!=target{return Err("Server substituted target ID".into());}
    let sid=decode_hex::<16>(&session).map_err(error)?;
    let pending=request.bind_session(sid).map_err(error)?;
    let Server::Decision{session:decision,grant:Some(granted),host_key,host_nonce:Some(nonce)}=recv(conn).await?
        else{return Err("Peer declined routed test".into());};
    if decision!=session||granted!=GRANT||decode_hex::<32>(&host_key).map_err(error)?!=expected{
        return Err("Mismatched host key or grant".into());
    }
    let approved=pending.approve_local(granted).map_err(error)?;
    let mut peer=Peer::receiver(CONFIG)?;peer.start()?;
    let channel=peer.create_control_channel()?;
    let Signal::Offer{sdp}=signal(conn,&session).await? else{return Err("Expected offer".into());};
    if !valid_sdp(&sdp){return Err("Unexpected offer profile".into());}
    let offer=Description{kind:DescriptionKind::Offer,sdp};
    // Needed to create the answer; only synthetic video and no privileged
    // operations are possible in this executable.
    peer.set_description(false,&offer)?;
    let answer=peer.create_description(DescriptionKind::Answer)?;
    peer.set_description(true,&answer)?;
    send_signal(conn,&session,Signal::Answer{sdp:answer.sdp.clone()}).await?;
    let signed=approved.sign(id,decode_hex::<32>(&nonce).map_err(error)?,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    let Signal::Proof{signature}=signal(conn,&session).await? else{return Err("Expected host proof".into());};
    let local=signed.signature();
    let _proof=signed.finish(&decode_hex::<64>(&signature).map_err(error)?,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    send_signal(conn,&session,Signal::Proof{signature:hex(&local)}).await?;
    ice(&peer,conn,&session).await?;
    ready(&peer,&channel,&offer.sdp,false)?;
    channel.send(READY)?;
    wait_data(&peer,&channel,conn,READY_ACK).await?;
    out("CONNECTED ROUTED TEST")?;
    for(sequence,event)in [(1,InputEvent::PointerMove(PointerMove{x:100,y:200})),(2,InputEvent::Key(Key{usage:4,down:true}))]{
        channel.send(&InputEnvelope{version:VERSION,session_id:sid.to_vec(),sequence,event:Some(event)}.encode_to_vec())?;
    }
    let deadline=Instant::now()+Duration::from_secs(10);let mut frames=0;
    while frames<30{
        connected(&peer,&channel)?;
        if Instant::now()>=deadline{return Err("Routed decode timeout".into());}
        if let Some(sample)=peer.pull_frame(Duration::from_millis(25))?{
            let info=Peer::frame_info(&sample)?;
            if info.width!=CONFIG.width||info.height!=CONFIG.height{return Err("Unexpected frame dimensions".into());}
            frames+=1;
        }
        tick(conn).await?;
    }
    channel.send(STOP)?;
    wait_data(&peer,&channel,conn,ACK).await?;
    conn.send(Client::Close{session:session.clone()}).await?;
    finish(conn,&session).await?;
    out(&format!("PASS ROUTED CONTROLLER: decoded_frames={frames}, release_ack=true"))
}
fn roots(path:&str)->MediaResult<rustls::RootCertStore>{
    let file=std::fs::File::open(path).map_err(error)?;
    let mut data=Vec::new();
    file.take(256*1024+1).read_to_end(&mut data).map_err(error)?;
    if data.len()>256*1024{return Err("CA file exceeds limit".into());}
    let mut roots=rustls::RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut data.as_slice()){roots.add(cert.map_err(error)?).map_err(error)?;}
    if roots.is_empty(){return Err("Missing CA certificates".into());}
    Ok(roots)
}
async fn run()->MediaResult<()>{
    let mut a=std::env::args().skip(1);
    let host=match a.next().as_deref(){Some("--host")=>true,Some("--controller")=>false,_=>return Err("Choose --host or --controller".into())};
    let url=a.next().ok_or("Missing WSS URL")?;
    let pin=decode_hex::<32>(&a.next().ok_or("Missing server ID")?).map_err(error)?;
    let endpoint=Endpoint::secure(&url,pin)?;
    let ca=match a.next().as_deref(){None=>None,Some("--ca")=>Some(a.next().ok_or("Missing CA path")?),_=>return Err("Unexpected argument".into())};
    if a.next().is_some(){return Err("Unexpected argument".into());}
    let id=DeviceIdentity::generate();
    out(&format!("KEY {}",hex(&id.public_key())))?;
    let mut conn=if let Some(path)=ca{Connection::register_with_roots(endpoint,&id,roots(&path)?).await?}else{Connection::register(endpoint,&id).await?};
    out(&format!("ONLINE {}",conn.device_id()))?;
    out("PEER_KEY (independently verify the peer's entire public key)")?;
    let expected=decode_hex::<32>(&input()?).map_err(error)?;
    if expected==[0;32]||expected==id.public_key(){return Err("Invalid peer key".into());}
    out("CONSENT (type ALLOW ROUTED TEST — synthetic video and inert input only)")?;
    if input()? !="ALLOW ROUTED TEST"{return Err("Local consent denied".into());}
    let result=if host{self::host(&mut conn,&id,expected).await}else{controller(&mut conn,&id,expected).await};
    let close=conn.close().await;result?;close
}
#[tokio::main(flavor="current_thread")]
async fn main(){if let Err(e)=run().await{eprintln!("FAIL: {e}");std::process::exit(1);}}
