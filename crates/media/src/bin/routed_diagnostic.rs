//! M2.3 WSS-routed synthetic media test. No native capture, native input,
//! unattended service, automatic acceptance or remote system command.
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
async fn recv(conn:&mut Connection)->MediaResult<Server>{
    conn.receive(Duration::from_secs(20)).await
}
async fn ice(peer:&Peer,conn:&mut Connection,cipher:&mut RouteCipher)->MediaResult<()>{
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
    cipher.send(conn,Signal::IceBatch{items}).await?;
    let Signal::IceBatch{items}=cipher.receive(conn).await? else{return Err("Expected ICE batch".into());};
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
    let hn=pending.nonce();
    let host_nonce=hex(&hn);
    let approved=pending.approve_local(GRANT).map_err(error)?;
    conn.send(Client::Decision{session:session.clone(),grant:Some(GRANT),host_nonce:Some(host_nonce)}).await?;
    let binding=RouteBinding{session:sid,controller:expected,host:id.public_key(),controller_nonce:nonce,host_nonce:hn,requested:GRANT,granted:GRANT};
    let mut cipher=RouteCipher::establish(conn,&binding,id,Role::Host).await?;

    let mut peer=Peer::sender(CONFIG,Source::NativeRgba)?;
    peer.start()?;peer.push_rgba(frame(0))?;
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
    ice(&peer,conn,&mut cipher).await?;
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
    let cn=request.nonce();
    conn.send(Client::Request{target:target.clone(),requested:GRANT,controller_nonce:hex(&cn)}).await?;
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
    let binding=RouteBinding{session:sid,controller:id.public_key(),host:expected,controller_nonce:cn,host_nonce:decode_hex::<32>(&nonce).map_err(error)?,requested:GRANT,granted};
    let mut cipher=RouteCipher::establish(conn,&binding,id,Role::Controller).await?;
    let mut peer=Peer::receiver(CONFIG)?;peer.start()?;
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
    ice(&peer,conn,&mut cipher).await?;
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
