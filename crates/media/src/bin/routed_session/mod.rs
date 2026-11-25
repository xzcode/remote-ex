//! Shared one-shot routed session owner. Synthetic mode never captures or
//! injects; native modes require a separate executable and local consent.
use gstreamer_webrtc::WebRTCPeerConnectionState;
use prost::Message as _;
use remote_ex_core::{Permissions, SessionGate};
use remote_ex_input::{Backend, Dispatcher, DisplayArea, InputResult};
use remote_ex_media::{ControlChannel, Description, DescriptionKind, Event, IceCandidate, MediaResult, Peer, Source, VideoConfig};
use remote_ex_protocol::{InputEnvelope, Key, PointerMove, VERSION, input_envelope::Event as InputEvent};
use remote_ex_rendezvous_client::{Connection, Endpoint, secure_route::RouteCipher};
use remote_ex_rendezvous_protocol::{Client, Server, Signal, IceItem};
use remote_ex_security::{DeviceIdentity, PublicKey, Role, VerifiedSession, fingerprint, pairing::{OutgoingPairing, PendingPairing}, session::{VIEW, INPUT}, route_binding::RouteBinding};
use remote_ex_signaling::{decode_hex, hex};
use std::{io::{self, Read, Write, IsTerminal},sync::atomic::{AtomicBool,Ordering}, time::{Duration, Instant}};
#[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
mod viewer;
#[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
mod native;
mod exchange;
mod audio;
mod clipboard_sync;
static CANCEL:AtomicBool=AtomicBool::new(false);
fn canceled()->bool{CANCEL.load(Ordering::Acquire)}
#[derive(Clone,Copy,PartialEq,Eq)]
enum Profile{Test,View,Control,Exchange(exchange::Mode),Audio(bool),ClipboardSync(bool)}
impl Profile{
    fn code(self)->u8{match self{Self::Test=>0,Self::View=>1,Self::Control=>2,Self::Exchange(mode)=>mode.code(),Self::Audio(test)=>if test{9}else{10},Self::ClipboardSync(test)=>if test{13}else{14}}}
    fn grant(self)->u8{if self==Self::View||matches!(self,Self::Exchange(_)|Self::Audio(_)|Self::ClipboardSync(_)){VIEW}else{VIEW|INPUT}}
    fn consent(self,host:bool)->&'static str{match(self,host){
        (Self::Test,_)=>"ALLOW ROUTED TEST",(Self::View,true)=>"ALLOW ROUTED SCREEN",
        (Self::View,false)=>"ALLOW ROUTED VIEW",(Self::Control,true)=>"ALLOW ROUTED SCREEN AND INPUT",
        (Self::Control,false)=>"ALLOW ROUTED INPUT",
        (Self::Exchange(mode),_)=>mode.consent(host),
        (Self::Audio(true),_)=>"ALLOW TEST AUDIO",
        (Self::Audio(false),true)=>"ALLOW SEND SYSTEM AUDIO",
        (Self::Audio(false),false)=>"ALLOW PLAY SYSTEM AUDIO",
        (Self::ClipboardSync(true),_)=>"ALLOW TEST CLIPBOARD SYNC",
        (Self::ClipboardSync(false),_)=>"ALLOW LIVE CLIPBOARD TEXT SYNC",
    }}
}
const HEARTBEAT:&[u8]=b"remote-ex-routed/heartbeat/v1";
const SWITCH_VIEW:&[u8]=b"remote-ex-view/next/v1";
const VIEW_LAYOUT_ACK:u8=0xD1;
#[derive(Clone,Copy,Debug)]
struct Geometry{x:u32,y:u32,width:u32,height:u32}
impl Geometry{
    fn from_area(area:DisplayArea,config:VideoConfig)->MediaResult<Self>{
        area.validate()?;
        let(w,h)=if u64::from(area.width)*u64::from(config.height)>=u64::from(area.height)*u64::from(config.width){
            (config.width,(u64::from(area.height)*u64::from(config.width)/u64::from(area.width))as u32)
        }else{((u64::from(area.width)*u64::from(config.height)/u64::from(area.height))as u32,config.height)};
        if w<2||h<2{return Err("Unsupported display geometry".into());}
        Ok(Self{x:(config.width-w)/2,y:(config.height-h)/2,width:w,height:h})
    }
    fn signal(self)->Signal{Signal::Geometry{x:self.x,y:self.y,width:self.width,height:self.height}}
}
fn poll_release(channel:&ControlChannel)->MediaResult<bool>{
    match channel.receive()?{None=>Ok(false),Some(b) if b==ACK=>Ok(true),_=>Err("Unexpected control response".into())}
}
const READY:&[u8]=b"remote-ex-routed-test/ready/v1";
const READY_ACK:&[u8]=b"remote-ex-routed-test/ready-ack/v1";
const STOP:&[u8]=b"remote-ex-routed-test/stop/v1";
const ACK:&[u8]=b"remote-ex-routed-test/released/v1";

fn error<T:std::fmt::Debug>(e:T)->String{format!("{e:?}")}
fn out(s:&str)->MediaResult<()>{println!("{s}");io::stdout().flush().map_err(error)}
fn input()->MediaResult<String>{let mut s=String::new();io::stdin().read_line(&mut s).map_err(error)?;Ok(s.trim().into())}
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
        if canceled()||Instant::now()>=deadline{return Err("Control transport timeout or canceled".into());}
        std::thread::sleep(Duration::from_millis(10));}
}
fn connected(peer:&Peer,channel:&ControlChannel)->MediaResult<()>{
    peer.check_health()?;channel.check_health()?;
    if peer.connection_state()!=WebRTCPeerConnectionState::Connected||!channel.is_open(){return Err("Control disconnected".into());}Ok(())
}

fn frame(n:u32,config:VideoConfig)->Vec<u8>{let mut p=vec![0;config.rgba_bytes()];for(i,px)in p.chunks_exact_mut(4).enumerate(){px[0]=((i as u32%config.width+n*7)%256)as u8;px[1]=((i as u32/config.width)%256)as u8;px[2]=120;px[3]=255;}p}
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
async fn recv(conn:&mut Connection)->MediaResult<Server>{
    conn.receive(Duration::from_secs(20)).await
}
async fn ice(peer:&Peer,conn:&mut Connection,cipher:&mut RouteCipher,relay_only:bool)->MediaResult<()>{
    let deadline=Instant::now()+Duration::from_secs(18);
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
    if relay_only { require_relay(&items)?; }
    cipher.send(conn,Signal::IceBatch{items}).await?;
    let Signal::IceBatch{items}=cipher.receive(conn).await? else{return Err("Expected ICE batch".into());};
    if relay_only { require_relay(&items)?; }
    for item in items{peer.add_ice(&IceCandidate{mline:item.mline,candidate:item.candidate})?;}
    Ok(())
}
fn require_relay(items:&[IceItem])->MediaResult<()>{
    let mut count=0;
    for item in items.iter().filter(|i|!i.candidate.is_empty()){
        let parts:Vec<_>=item.candidate.split_ascii_whitespace().collect();
        if !parts.windows(2).any(|p|p==["typ","relay"]){return Err("Non-relay ICE candidate in forced relay mode".into());}
        count+=1;
    }
    if count==0{return Err("No TURN relay candidates".into());}
    Ok(())
}
fn gate(proof:&VerifiedSession)->MediaResult<SessionGate>{
    let permissions=Permissions{view:true,input:proof.transcript().granted&INPUT!=0};
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

async fn host(conn:&mut Connection,id:&DeviceIdentity,expected:PublicKey,relay_only:bool,profile:Profile,display:Option<u32>,extra_displays:Vec<u32>,quality:&str,exchange_path:Option<&std::path::Path>,volume:u8,resume:&exchange::ResumeOptions)->MediaResult<()>{
    let grant=profile.grant();
    let Server::Incoming{session,controller,controller_key,requested,controller_nonce}=recv(conn).await?
        else{return Err("Expected incoming device request".into());};
    if controller!=hex(&fingerprint(&expected)) || decode_hex::<32>(&controller_key).map_err(error)?!=expected || requested!=grant{
        conn.send(Client::Decision{session,grant:None,host_nonce:None}).await?;
        return Err("Unknown peer or unexpected test grant".into());
    }
    let sid=decode_hex::<16>(&session).map_err(error)?;
    let nonce=decode_hex::<32>(&controller_nonce).map_err(error)?;
    let pending=PendingPairing::new(Role::Host,sid,id,expected,grant).map_err(error)?;
    let hn=pending.nonce();
    let host_nonce=hex(&hn);
    let approved=pending.approve_local(grant).map_err(error)?;
    conn.send(Client::Decision{session:session.clone(),grant:Some(grant),host_nonce:Some(host_nonce)}).await?;
    let binding=RouteBinding{session:sid,controller:expected,host:id.public_key(),controller_nonce:nonce,host_nonce:hn,requested:grant,granted:grant,profile:profile.code(),relay_only};
    let mut cipher=RouteCipher::establish(conn,&binding,id,Role::Host).await?;

    let config=negotiate_quality(conn,&mut cipher,quality).await?;
    let ice_config=conn.request_ice(&session).await?;
    let mut peer=Peer::sender(config,Source::NativeRgba)?;
    peer.configure_ice(&ice_config,relay_only)?;drop(ice_config);
    peer.start()?;
    // Never show a synthetic desktop pattern as the initial native frame.
    if matches!(profile,Profile::View|Profile::Control){let mut black=vec![0;config.rgba_bytes()];for p in black.chunks_exact_mut(4){p[3]=255;}peer.push_rgba(black)?;}else{peer.push_rgba(frame(0,config))?;}
    let channel=peer.create_control_channel()?;
    let offer=offer(&peer)?;
    peer.set_description(true,&offer)?;
    cipher.send(conn,Signal::Offer{sdp:offer.sdp.clone()}).await?;
    let Signal::Answer{sdp}=cipher.receive(conn).await? else{return Err("Expected answer".into());};
    if !valid_sdp(&sdp){return Err("Unexpected answer profile".into());}
    let answer=Description{kind:DescriptionKind::Answer,sdp};
    let signed=approved.sign(id,nonce,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    cipher.send(conn,Signal::Proof{signature:hex(&signed.signature())}).await?;
    let Signal::Proof{signature}=cipher.receive(conn).await? else{return Err("Expected peer proof".into());};
    let proof=signed.finish(&decode_hex::<64>(&signature).map_err(error)?,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    peer.set_description(false,&answer)?;
    ice(&peer,conn,&mut cipher,relay_only).await?;
    ready(&peer,&channel,&answer.sdp,true)?;
    if let Profile::ClipboardSync(test)=profile{
        wait_data(&peer,&channel,conn,READY).await?;channel.send(READY_ACK)?;
        clipboard_sync::run(&peer,&channel,conn,true,test).await?;
        channel.send(clipboard_sync::HOST_DONE)?;
        finish(conn,&session).await?;return out("PASS ROUTED HOST: clipboard sync complete");
    }
    if let Profile::Audio(test)=profile{
        wait_data(&peer,&channel,conn,READY).await?;channel.send(READY_ACK)?;
        audio::send(&peer,&channel,conn,test).await?;channel.send(audio::HOST_DONE)?;
        finish(conn,&session).await?;return out("PASS ROUTED HOST: audio complete");
    }
    if let Profile::Exchange(mode)=profile{
        wait_data(&peer,&channel,conn,READY).await?;channel.send(READY_ACK)?;
        let scope=if mode.sending(true){remote_ex_desktop_features::checkpoint::Scope{sender:id.public_key(),receiver:expected}}else{remote_ex_desktop_features::checkpoint::Scope{sender:expected,receiver:id.public_key()}};
        exchange::run(&peer,&channel,conn,&session,mode,true,exchange_path,scope,resume).await?;
        channel.send(exchange::HOST_DONE)?;
        finish(conn,&session).await?;return out("PASS ROUTED HOST: transfer verified");
    }
    if profile!=Profile::Test{
        #[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
        {return native::host(&peer,&channel,conn,&mut cipher,&session,gate(&proof)?,profile,display.ok_or("Select a display")?,extra_displays).await;}
        #[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
        {let _=(display,extra_displays);return Err("Native routed mode unavailable".into());}
    }
    let backend=Inert::default();let log=backend.events.clone();
    let mut dispatcher=Dispatcher::new(gate(&proof)?,backend,DisplayArea{x:0,y:0,width:config.width,height:config.height})?;
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
                peer.push_rgba(frame(frames,config))?;frames+=1;
                next=Instant::now()+Duration::from_millis(1000/u64::from(config.fps));
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
async fn controller(mut owned:Connection,id:&DeviceIdentity,expected:PublicKey,relay_only:bool,profile:Profile,quality:&str,exchange_path:Option<&std::path::Path>,volume:u8,resume:&exchange::ResumeOptions)->MediaResult<()>{
    let conn=&mut owned;
    let grant=profile.grant();
    let target=hex(&fingerprint(&expected));
    let request=OutgoingPairing::new(id,expected,grant).map_err(error)?;
    let cn=request.nonce();
    conn.send(Client::Request{target:target.clone(),requested:grant,controller_nonce:hex(&cn)}).await?;
    let Server::Requested{session,target:returned}=recv(conn).await? else{return Err("Device request failed".into());};
    if returned!=target{return Err("Server substituted target ID".into());}
    let sid=decode_hex::<16>(&session).map_err(error)?;
    let pending=request.bind_session(sid).map_err(error)?;
    let Server::Decision{session:decision,grant:Some(granted),host_key,host_nonce:Some(nonce)}=recv(conn).await?
        else{return Err("Peer declined routed test".into());};
    if decision!=session||granted!=grant||decode_hex::<32>(&host_key).map_err(error)?!=expected{
        return Err("Mismatched host key or grant".into());
    }
    let approved=pending.approve_local(granted).map_err(error)?;
    let binding=RouteBinding{session:sid,controller:id.public_key(),host:expected,controller_nonce:cn,host_nonce:decode_hex::<32>(&nonce).map_err(error)?,requested:grant,granted,profile:profile.code(),relay_only};
    let mut cipher=RouteCipher::establish(conn,&binding,id,Role::Controller).await?;
    let config=negotiate_quality(conn,&mut cipher,quality).await?;
    let ice_config=conn.request_ice(&session).await?;
    let mut peer=Peer::receiver(config)?;
    peer.configure_ice(&ice_config,relay_only)?;drop(ice_config);peer.start()?;
    let channel=peer.create_control_channel()?;
    let Signal::Offer{sdp}=cipher.receive(conn).await? else{return Err("Expected offer".into());};
    if !valid_sdp(&sdp){return Err("Unexpected offer profile".into());}
    let offer=Description{kind:DescriptionKind::Offer,sdp};
    // Needed to create the answer; only synthetic video and no privileged
    // operations are possible in this executable.
    peer.set_description(false,&offer)?;
    let answer=peer.create_description(DescriptionKind::Answer)?;
    peer.set_description(true,&answer)?;
    cipher.send(conn,Signal::Answer{sdp:answer.sdp.clone()}).await?;
    let signed=approved.sign(id,decode_hex::<32>(&nonce).map_err(error)?,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    let Signal::Proof{signature}=cipher.receive(conn).await? else{return Err("Expected host proof".into());};
    let local=signed.signature();
    let _proof=signed.finish(&decode_hex::<64>(&signature).map_err(error)?,offer.sdp.as_bytes(),answer.sdp.as_bytes()).map_err(error)?;
    cipher.send(conn,Signal::Proof{signature:hex(&local)}).await?;
    ice(&peer,conn,&mut cipher,relay_only).await?;
    ready(&peer,&channel,&offer.sdp,false)?;
    if let Profile::ClipboardSync(test)=profile{
        channel.send(READY)?;wait_data(&peer,&channel,conn,READY_ACK).await?;
        clipboard_sync::run(&peer,&channel,conn,false,test).await?;
        wait_data(&peer,&channel,conn,clipboard_sync::HOST_DONE).await?;
        conn.send(Client::Close{session:session.clone()}).await?;finish(conn,&session).await?;
        return out("PASS ROUTED CONTROLLER: clipboard sync complete");
    }
    if let Profile::Audio(test)=profile{
        channel.send(READY)?;wait_data(&peer,&channel,conn,READY_ACK).await?;
        audio::play(&peer,&channel,conn,test,volume).await?;
        wait_data(&peer,&channel,conn,audio::HOST_DONE).await?;
        conn.send(Client::Close{session:session.clone()}).await?;finish(conn,&session).await?;
        return out("PASS ROUTED CONTROLLER: audio complete");
    }
    if let Profile::Exchange(mode)=profile{
        channel.send(READY)?;wait_data(&peer,&channel,conn,READY_ACK).await?;
        let scope=if mode.sending(false){remote_ex_desktop_features::checkpoint::Scope{sender:id.public_key(),receiver:expected}}else{remote_ex_desktop_features::checkpoint::Scope{sender:expected,receiver:id.public_key()}};
        exchange::run(&peer,&channel,conn,&session,mode,false,exchange_path,scope,resume).await?;
        wait_data(&peer,&channel,conn,exchange::HOST_DONE).await?;
        conn.send(Client::Close{session:session.clone()}).await?;finish(conn,&session).await?;
        return out("PASS ROUTED CONTROLLER: transfer verified");
    }
    if profile!=Profile::Test{
        #[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
        {
            let Signal::Geometry{x,y,width,height}=cipher.receive(conn).await? else{return Err("Missing authorized display geometry".into());};
            let geometry=Geometry{x,y,width,height};
            if x.checked_add(width).is_none_or(|n|n>config.width)||y.checked_add(height).is_none_or(|n|n>config.height){return Err("Geometry exceeds negotiated canvas".into());}
            let screens=if profile==Profile::View{
                match cipher.receive(conn).await?{Signal::ViewLayout{count} if (1..=4).contains(&count)=>count,_=>return Err("Missing approved display list".into())}
            }else{1};
            channel.send(READY)?;
            wait_data(&peer,&channel,conn,READY_ACK).await?;
            return native::controller(&peer,&channel,owned,session,sid,geometry,profile,screens).await;
        }
        #[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
        {return Err("Native routed mode unavailable".into());}
    }
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
            if info.width!=config.width||info.height!=config.height{return Err("Unexpected frame dimensions".into());}
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
async fn negotiate_quality(conn:&mut Connection,cipher:&mut RouteCipher,name:&str)->MediaResult<VideoConfig>{
    let q=remote_ex_desktop_features::quality::preset(name)?;
    cipher.send(conn,Signal::Quality{preset:name.into()}).await?;
    match cipher.receive(conn).await?{Signal::Quality{preset} if preset==name=>{},_=>return Err("Both peers must approve the same quality preset".into())}
    VideoConfig{width:q.width,height:q.height,fps:q.fps,bitrate:q.bitrate}.validate()
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
pub async fn run(native_allowed:bool)->MediaResult<()>{
    let mut a=std::env::args().skip(1);
    let host=match a.next().as_deref(){Some("--host")=>true,Some("--controller")=>false,_=>return Err("Choose --host or --controller".into())};
    let url=a.next().ok_or("Missing WSS URL")?;
    let pin=decode_hex::<32>(&a.next().ok_or("Missing server ID")?).map_err(error)?;
    let endpoint=Endpoint::secure(&url,pin)?;
    let mut ca=None;let mut relay_only=false;let mut profile=Profile::Test;let mut display=None;let mut supervised=false;let mut persistent=false;let mut defer_test_registration=false;let mut quality="compat".to_owned();let mut exchange_path=None;let mut volume=75u8;let mut extra_displays=Vec::new();let mut identity_slot=None;let mut resume=exchange::ResumeOptions::default();let mut shared_controller=false;
    while let Some(arg)=a.next(){match arg.as_str(){
        "--ca" if ca.is_none()=>ca=Some(a.next().ok_or("Missing CA path")?),
        "--relay-only" if !relay_only=>relay_only=true,
        "--quality"=>{quality=a.next().ok_or("Missing quality preset")?;remote_ex_desktop_features::quality::preset(&quality)?;},
        "--desktop-session" if !supervised=>supervised=true,
        "--defer-test-registration" if !native_allowed&&!defer_test_registration=>defer_test_registration=true,
        "--persistent" if !persistent=>persistent=true,
        "--shared-controller" if !shared_controller=>shared_controller=true,
        "--identity-slot" if identity_slot.is_none()=>{
            let n=a.next().ok_or("Missing identity slot")?.parse::<u8>().map_err(error)?;
            remote_ex_security::identity_store::credential_name(n)?;identity_slot=Some(n);
        },
        "--clipboard-sync" if native_allowed&&profile==Profile::Test=>profile=Profile::ClipboardSync(false),
        "--clipboard-sync-test" if !native_allowed&&profile==Profile::Test=>profile=Profile::ClipboardSync(true),
        "--audio" if native_allowed&&profile==Profile::Test=>profile=Profile::Audio(false),
        "--audio-test" if !native_allowed&&profile==Profile::Test=>profile=Profile::Audio(true),
        "--volume"=>{volume=a.next().ok_or("Missing volume")?.parse().map_err(error)?;if volume>100{return Err("Volume out of range".into());}},
        "--exchange" if profile==Profile::Test=>{
            let mode=exchange::Mode::parse(&a.next().ok_or("Missing exchange kind")?,&a.next().ok_or("Missing sender role")?)?;
            profile=Profile::Exchange(mode);
            if matches!(mode.kind,remote_ex_desktop_features::exchange::Kind::File|remote_ex_desktop_features::exchange::Kind::Directory){exchange_path=Some(std::path::PathBuf::from(a.next().ok_or("Select explicit local file path")?));}
        },
        "--resume-from" if resume.from.is_none()=>resume.from=Some(std::path::PathBuf::from(a.next().ok_or("Select existing local checkpoint")?)),
        "--save-checkpoint" if resume.save_to.is_none()=>resume.save_to=Some(std::path::PathBuf::from(a.next().ok_or("Select unused local checkpoint path")?)),
        "--view" if native_allowed&&profile==Profile::Test=>{profile=Profile::View;if host{display=Some(a.next().ok_or("Select display ID")?.parse::<u32>().map_err(error)?);}},
        "--view-extra" if native_allowed&&host&&profile==Profile::View=>{
            let d=a.next().ok_or("Missing additional display ID")?.parse::<u32>().map_err(error)?;
            if extra_displays.len()>=3||display==Some(d)||extra_displays.contains(&d){return Err("Select up to four distinct displays".into());}extra_displays.push(d);
        },
        "--control" if native_allowed&&profile==Profile::Test=>{profile=Profile::Control;if host{display=Some(a.next().ok_or("Select display ID")?.parse::<u32>().map_err(error)?);}},
        _=>return Err("Unexpected argument".into()),
    }}
    if shared_controller&&(host||!matches!(profile,Profile::Test|Profile::View|Profile::Control)||persistent&&identity_slot.unwrap_or(0)!=0){return Err("Shared identity is only for outgoing view/control/test using slot one identity".into());}
    if identity_slot.is_some()&&!persistent{return Err("Identity slot requires explicit persistent storage opt-in".into());}
    if defer_test_registration&&(!supervised||persistent||profile!=Profile::Test){return Err("Test registration barrier requires synthetic temporary supervised mode".into());}
    if native_allowed&&profile==Profile::Test{return Err("Choose a separate view, control or exchange profile".into());}
    if let Profile::ClipboardSync(test)=profile{
        if !test&&!cfg!(all(feature="native-control",any(target_os="windows",target_os="macos"))){return Err("Native clipboard unavailable".into());}
        quality="compat".into();
    }
    if let Profile::Audio(test)=profile{
        if !test&&host&&!cfg!(target_os="windows"){return Err("System audio capture currently requires Windows WASAPI loopback; microphone fallback is disabled".into());}
        quality="compat".into();
    }
    if resume.from.is_some()||resume.save_to.is_some(){
        if !matches!(profile,Profile::Exchange(mode) if mode.kind==remote_ex_desktop_features::exchange::Kind::File&&!mode.sending(host)){
            return Err("Checkpoints are only allowed on the explicitly authorized file receiver".into());
        }
        let target=exchange_path.as_ref().ok_or("Choose new local destination")?;
        for path in [&resume.from,&resume.save_to].into_iter().flatten(){
            if !path.is_absolute()||path.file_name().is_none()||path==target{return Err("Choose a separate absolute checkpoint path".into());}
        }
        if resume.save_to.as_ref().is_some_and(|p|p.exists()||!p.parent().is_some_and(|d|d.is_dir()))||resume.from.as_ref().is_some_and(|p|!p.is_file())||resume.from.is_some()&&resume.from==resume.save_to{
            return Err("Checkpoint input must exist; output must be a separate unused path".into());
        }
    }
    if let Profile::Exchange(mode)=profile{
        if !matches!(mode.kind,remote_ex_desktop_features::exchange::Kind::File|remote_ex_desktop_features::exchange::Kind::Directory)&&!cfg!(all(feature="native-control",any(target_os="windows",target_os="macos"))){return Err("Native clipboard unavailable".into());}
        if !native_allowed&&!matches!(mode.kind,remote_ex_desktop_features::exchange::Kind::File|remote_ex_desktop_features::exchange::Kind::Directory){return Err("Diagnostic cannot access clipboard".into());}
        quality="compat".into();
    }
    if matches!(profile,Profile::View|Profile::Control)&&!cfg!(all(feature="native-control",any(target_os="windows",target_os="macos"))){return Err("Native mode not compiled for this platform".into());}
    if supervised&&(io::stdin().is_terminal()||io::stdout().is_terminal()){return Err("Desktop mode requires supervisor pipes".into());}
    if profile!=Profile::Test&&!supervised&&(!io::stdin().is_terminal()||!io::stdout().is_terminal()){return Err("Visible local consent required".into());}
    ctrlc::set_handler(||CANCEL.store(true,Ordering::Release)).map_err(error)?;
    let id=if persistent{
        if !supervised{
            out("Type STORE IDENTITY to use the system credential store")?;
            if input()? !="STORE IDENTITY"{return Err("Persistent identity declined".into());}
        }
        #[cfg(all(feature="native-control",any(target_os="windows",target_os="macos")))]
        {remote_ex_security::identity_store::native::open_slot(identity_slot.unwrap_or(0))?}
        #[cfg(not(all(feature="native-control",any(target_os="windows",target_os="macos"))))]
        {return Err("Persistent identity unavailable".into());}
    }else{DeviceIdentity::generate()};
    out(&format!("KEY {}",hex(&id.public_key())))?;
    // The isolated CI lab explicitly enrolls these ephemeral keys before
    // opening the TLS endpoint. This barrier exists only in the inert binary.
    if defer_test_registration && (input()?!="REGISTER TEST"||canceled()){
        return Err("Test enrollment canceled".into());
    }
    let mut conn=match (shared_controller,ca){
        (true,Some(path))=>Connection::register_controller_with_roots(endpoint,&id,roots(&path)?).await?,
        (true,None)=>Connection::register_controller(endpoint,&id).await?,
        (false,Some(path))=>Connection::register_with_roots(endpoint,&id,roots(&path)?).await?,
        (false,None)=>Connection::register(endpoint,&id).await?,
    };
    out(&format!("ONLINE {}",conn.device_id()))?;
    out("PEER_KEY (independently verify the peer's entire public key)")?;
    let expected=decode_hex::<32>(&input()?).map_err(error)?;
    if expected==[0;32]||expected==id.public_key(){return Err("Invalid peer key".into());}
    if let Profile::Exchange(mode)=profile{out(&format!("TRANSFER {} {:?}",if mode.sending(host){"SEND"}else{"RECEIVE"},mode.kind))?;}
    out(&format!("CONSENT (type {}; one-shot)",profile.consent(host)))?;
    if input()? !=profile.consent(host)||canceled(){return Err("Local consent denied".into());}
    audio::VOLUME.store(volume,Ordering::Release);
    if supervised{
        let allow_pause=matches!(profile,Profile::Exchange(m) if m.kind==remote_ex_desktop_features::exchange::Kind::File&&m.sending(host));
        let allow_volume=profile==Profile::Audio(false)&&!host;
        std::thread::spawn(move||{
            use std::io::BufRead;
            let mut pipe=io::stdin().lock();
            if allow_pause||allow_volume{
                loop{
                    let mut line=Vec::new();
                    match (&mut pipe).take(16).read_until(b'\n',&mut line){
                        Ok(n) if n>0=>{
                            // Python/Windows text pipes emit CRLF. Require exactly
                            // one complete line; no whitespace or arbitrary command.
                            let Some(command)=line.strip_suffix(b"\n")else{break;};
                            let command=command.strip_suffix(b"\r").unwrap_or(command);
                            match command{
                                b"PAUSE" if allow_pause=>exchange::PAUSED.store(true,Ordering::Release),
                                b"RESUME" if allow_pause=>exchange::PAUSED.store(false,Ordering::Release),
                                cmd if allow_volume&&cmd.starts_with(b"VOLUME ")=>{
                                    let value=std::str::from_utf8(&cmd[7..]).ok().filter(|s|!s.is_empty()&&s.len()<=3&&s.bytes().all(|b|b.is_ascii_digit())).and_then(|s|s.parse::<u8>().ok());
                                    if let Some(v)=value.filter(|v|*v<=100){audio::VOLUME.store(v,Ordering::Release);}else{break;}
                                },
                                _=>break,
                            }
                        },
                        _=>break,
                    }
                }
            }else{let mut b=[0u8;1];let _=pipe.read(&mut b);}
            CANCEL.store(true,Ordering::Release);
        });
    }
    if host{
        let result=self::host(&mut conn,&id,expected,relay_only,profile,display,extra_displays,&quality,exchange_path.as_deref(),volume,&resume).await;
        let close=conn.close().await;result?;close
    }else{controller(conn,&id,expected,relay_only,profile,&quality,exchange_path.as_deref(),volume,&resume).await}
}
