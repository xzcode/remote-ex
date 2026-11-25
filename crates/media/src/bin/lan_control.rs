//! Encrypted, explicit one-shot control session integration test.
//! Inert backend and synthetic video only; never injects OS input.
use gstreamer_webrtc::WebRTCPeerConnectionState;
use prost::Message as ProstMessage;
use remote_ex_core::{Permissions,SessionGate};
use remote_ex_input::{Backend,Dispatcher,DisplayArea,InputResult};
use remote_ex_media::{ControlChannel,Description,DescriptionKind,Event,IceCandidate,MediaResult,Peer,Source,VideoConfig};
use remote_ex_protocol::{InputEnvelope,Key,PointerMove,VERSION,input_envelope::Event as InputEvent};
use remote_ex_security::{DeviceIdentity,PublicKey,Role,pairing::{PendingPairing,new_session_id},session::{VIEW,INPUT}};
use remote_ex_signaling::{Candidate,Message,Wire,decode_hex,hex,MAX_CANDIDATES,secure::{NoiseIdentity,SecureWire}};
use std::{io::{self,Write},net::SocketAddr,time::{Duration,Instant}};
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
    if peer.connection_state()!=WebRTCPeerConnectionState::Connected||!channel.is_open(){return Err("Control disconnected".into());}Ok(())
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
fn serve<B:Backend,F:FnMut(u32)->MediaResult<Vec<u8>>>(peer:&Peer,channel:&ControlChannel,mut d:Dispatcher<B>,mut frame:F)->MediaResult<(u32,u32)>{
    let started=Instant::now();let mut last=started;let mut next=started;let mut frames=0;let mut events=0;
    let result=(||{loop{
        connected(peer,channel)?;
        if started.elapsed()>=Duration::from_secs(12)||last.elapsed()>Duration::from_secs(2){return Err("Control deadline or heartbeat lost".into());}
        for _ in 0..64{
            let Some(data)=channel.receive()? else{break};
            if data==HEARTBEAT{last=Instant::now();}else if data==STOP{return Ok(());}else{d.dispatch(&data)?;events+=1;}
        }
        if Instant::now()>=next{peer.push_rgba(frame(frames)?)?;frames+=1;next=Instant::now()+Duration::from_millis(100);}
        std::thread::sleep(Duration::from_millis(5));
    }})();
    d.close()?;result?;channel.send(ACK)?;Ok((frames,events))
}
fn host(mut wire:SecureWire,id:DeviceIdentity,expected:PublicKey)->MediaResult<()>{
    let Message::Request{session,controller,nonce,requested}=recv(&mut wire,None)? else{return Err("Expected request".into())};
    if controller!=expected||requested!=GRANT{return Err("Wrong identity/grant".into());}
    if recv(&mut wire,Some(session))?!=(Message::ViewProfile{session,synthetic:true}){return Err("Test profile required".into());}
    let p=PendingPairing::new(Role::Host,session,&id,expected,GRANT).map_err(error)?;
    let hn=p.nonce();let p=p.approve_local(GRANT).map_err(error)?;
    send(&mut wire,Message::Accept{session,host:id.public_key(),nonce:hn,granted:GRANT})?;
    send(&mut wire,Message::ViewProfile{session,synthetic:true})?;
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
    if recv(&mut wire,Some(session))?!=(Message::Ready{session}){return Err("Controller not ready".into());}
    send(&mut wire,Message::Ready{session})?;out("CONNECTED TEST CONTROL")?;
    let backend=Inert::default();let log=backend.events.clone();
    let d=Dispatcher::new(gate,backend,DisplayArea{x:0,y:0,width:960,height:540})?;
    let (frames,events)=serve(&peer,&channel,d,|n|Ok(frame(n)))?;
    if log.borrow().as_slice()!=["move","key:4:true","key:4:false"]{return Err("Inert input/release verification failed".into());}
    if recv(&mut wire,Some(session))?!=(Message::Finished{session}){return Err("Missing finish confirmation".into());}
    out(&format!("PASS HOST CONTROL: sent_frames={frames}, dispatched_events={events}, released"))
}
fn controller(mut wire:SecureWire,id:DeviceIdentity,expected:PublicKey)->MediaResult<()>{
    let session=new_session_id();let p=PendingPairing::new(Role::Controller,session,&id,expected,GRANT).map_err(error)?;
    let nonce=p.nonce();send(&mut wire,Message::Request{session,controller:id.public_key(),nonce,requested:GRANT})?;
    send(&mut wire,Message::ViewProfile{session,synthetic:true})?;
    let Message::Accept{host,nonce:hn,granted,..}=recv(&mut wire,Some(session))? else{return Err("Expected acceptance".into())};
    if host!=expected||granted!=GRANT{return Err("Wrong host/grant".into());}
    if recv(&mut wire,Some(session))?!=(Message::ViewProfile{session,synthetic:true}){return Err("Test profile mismatch".into());}
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
    send(&mut wire,Message::Ready{session})?;
    if recv(&mut wire,Some(session))?!=(Message::Ready{session}){return Err("Host not ready".into());}
    out("CONNECTED TEST CONTROL")?;
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
    channel.send(STOP)?;let until=Instant::now()+Duration::from_secs(3);
    loop{peer.check_health()?;if let Some(b)=channel.receive()?{if b==ACK{break;}else{return Err("Unexpected response".into());}}
        if Instant::now()>=until{return Err("Release ACK timeout".into());}std::thread::sleep(Duration::from_millis(10));}
    send(&mut wire,Message::Finished{session})?;out(&format!("PASS CONTROLLER CONTROL: decoded_frames={frames}, release_ack=true"))
}
fn run()->MediaResult<()>{
    let mut a=std::env::args().skip(1);
    let host=match a.next().as_deref(){Some("--listen")=>true,Some("--connect")=>false,_=>return Err("Use --listen or --connect".into())};
    let address:SocketAddr=a.next().ok_or("Missing address")?.parse().map_err(error)?;
    if a.next().as_deref()!=Some("--test")||a.next().is_some()||!remote_ex_signaling::local_address(address){return Err("Explicit test profile and private address required".into());}
    let id=DeviceIdentity::generate();let noise=NoiseIdentity::generate().map_err(error)?;
    let mut bundle=id.public_key().to_vec();bundle.extend_from_slice(&noise.public_key());
    out(&format!("KEY {}",hex(&bundle)))?;out("PEER_KEY (verify complete key bundle)")?;
    let b=decode_hex::<64>(&input()?).map_err(error)?;
    let expected:PublicKey=b[..32].try_into().map_err(error)?;let pn:[u8;32]=b[32..].try_into().map_err(error)?;
    out("CONSENT (type ALLOW TEST CONTROL)")?;if input()? !="ALLOW TEST CONTROL"{return Err("Consent denied".into());}
    let wire=if host{let l=Wire::bind(address).map_err(error)?;out(&format!("LISTENING {}",l.local_addr().map_err(error)?))?;Wire::accept_listener(l).map_err(error)?}else{Wire::connect(address).map_err(error)?};
    let wire=SecureWire::handshake(wire,noise,pn,!host).map_err(error)?;
    if host{crate::host(wire,id,expected)}else{controller(wire,id,expected)}
}
fn main(){if let Err(e)=run(){eprintln!("FAIL: {e}");std::process::exit(1);}}
