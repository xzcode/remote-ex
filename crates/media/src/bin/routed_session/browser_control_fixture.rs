//! Isolated CI acceptance for the real browser controller, using the exact
//! native profile-2 binding and input dispatcher but an inert backend.
//! Never captures a display, injects an OS event, or runs a command.
use super::*;
use remote_ex_protocol::{decode_input,Input};
use std::{cell::RefCell,rc::Rc};
const AREA:DisplayArea=DisplayArea{x:0,y:0,width:1024,height:768};

#[derive(Default)]
struct Evidence{
    position:(i32,i32),moves:u32,center:bool,
    key_held:bool,button_held:bool,key_down:u32,key_up:u32,button_down:u32,button_up:u32,
    scroll:u32,released:bool,release_packets:u32,modifier_held:bool,modifier_down:u32,modifier_up:u32,chord_key:bool,chord_verified:bool,
}
struct FixtureBackend(Rc<RefCell<Evidence>>);
impl Backend for FixtureBackend{
    fn check_display(&mut self,area:DisplayArea)->InputResult<()>{
        if area!=AREA{return Err("Unexpected inert display area".into());}Ok(())
    }
    fn move_to(&mut self,x:i32,y:i32)->InputResult<()>{
        if !(0..1024).contains(&x)||!(0..768).contains(&y){return Err("Inert pointer outside area".into());}
        let mut v=self.0.borrow_mut();v.position=(x,y);v.moves+=1;
        v.center|=(x-512).abs()<=8&&(y-384).abs()<=8;Ok(())
    }
    fn position(&mut self)->InputResult<(i32,i32)>{Ok(self.0.borrow().position)}
    fn key(&mut self,usage:u16,down:bool)->InputResult<()>{
        // Only the planned Ctrl+A and later physical A. No arbitrary
        // keystroke logging or OS state.
        let mut v=self.0.borrow_mut();
        if usage==224{
            if down{if v.modifier_held||v.key_held||v.modifier_down!=0{return Err("Unexpected inert modifier press".into());}v.modifier_down+=1;}
            else{if !v.modifier_held||v.key_held||!v.chord_key{return Err("Unexpected inert modifier release order".into());}v.modifier_up+=1;}
            v.modifier_held=down;return Ok(());
        }
        if usage!=4{return Err("Unexpected inert test key".into());}
        if down{v.key_down+=1;if v.modifier_held{v.chord_key=true;}}else{v.key_up+=1;}v.key_held=down;Ok(())
    }
    fn button(&mut self,button:u8,down:bool)->InputResult<()>{
        if button!=1{return Err("Unexpected inert test button".into());}
        let mut v=self.0.borrow_mut();
        if down{v.button_down+=1;}else{v.button_up+=1;}v.button_held=down;Ok(())
    }
    fn scroll_steps(&mut self,x:i32,y:i32)->InputResult<()>{
        if x!=0||!(1..=10).contains(&y){return Err("Unexpected inert test wheel".into());}
        self.0.borrow_mut().scroll+=1;Ok(())
    }
}

pub(super) async fn serve(peer:&Peer,channel:&ControlChannel,conn:&mut Connection,cipher:&mut RouteCipher,
    session:&str,gate:SessionGate,config:VideoConfig)->MediaResult<()>{
    if config.width!=960||config.height!=540{return Err("Fixture requires compat canvas".into());}
    let evidence=Rc::new(RefCell::new(Evidence::default()));
    let mut dispatcher=Dispatcher::new(gate,FixtureBackend(evidence.clone()),AREA)?;
    cipher.send(conn,Geometry::from_area(AREA,config)?.signal()).await?;
    wait_data(peer,channel,conn,READY).await?;
    peer.push_rgba(frame(0,config))?;
    channel.send(READY_ACK)?;
    out("CONNECTED BROWSER INERT CONTROL")?;
    let started=Instant::now();let mut last=started;let mut next=started;let mut frames=0u32;let mut packets=0u32;
    let result:MediaResult<()>=async{
        loop{
            if canceled()||started.elapsed()>Duration::from_secs(25){return Err("Fixture stopped before explicit browser close".into());}
            connected(peer,channel)?;
            if last.elapsed()>Duration::from_secs(3){return Err("Browser heartbeat lost".into());}
            for _ in 0..32{
                let Some(bytes)=channel.receive()? else{break;};
                if bytes==HEARTBEAT{last=Instant::now();continue;}
                if bytes==STOP{
                    let v=evidence.borrow();
                    if !v.released||!v.chord_verified||v.modifier_held||v.modifier_down!=1||v.modifier_up!=1||v.key_held||v.button_held||v.key_down!=2||v.key_up!=2||v.button_down!=1||v.button_up!=1||v.moves==0||!v.center||v.scroll==0||frames<10{
                        return Err("Browser input or explicit release evidence incomplete".into());
                    }
                    return Ok(());
                }
                packets+=1;if packets>128{return Err("Fixture input packet limit exceeded".into());}
                let event=decode_input(&bytes).map_err(error)?.input;
                let release=matches!(event,Input::ReleaseAll);
                let both_held={let v=evidence.borrow();v.key_held&&v.button_held};
                if evidence.borrow().released&&!release{return Err("Input continued after browser lost focus".into());}
                // Real Rust permission/sequence/geometry/rate gate.
                dispatcher.dispatch(&bytes)?;
                {
                    let mut v=evidence.borrow_mut();
                    if !v.chord_verified&&v.chord_key&&v.modifier_down==1&&v.modifier_up==1&&!v.modifier_held&&!v.key_held&&v.key_down==1&&v.key_up==1{
                        v.chord_verified=true;out("BROWSER_INERT_CHORD_VERIFIED")?;
                    }
                }
                if release{
                    let mut v=evidence.borrow_mut();v.release_packets+=1;
                    if v.key_held||v.button_held||v.modifier_held{return Err("Explicit input release failed".into());}
                    if both_held{v.released=true;out("BROWSER_INERT_RELEASE_VERIFIED")?;}
                }
            }
            if Instant::now()>=next{
                peer.push_rgba(frame(frames,config))?;frames+=1;
                next=Instant::now()+Duration::from_millis(1000/u64::from(config.fps));
            }
            tick(conn).await?;
        }
    }.await;
    dispatcher.close()?;
    result?;
    channel.send(ACK)?;
    finish(conn,session).await?;
    out("PASS BROWSER INERT CONTROL: authenticated input, mapped pointer, wheel, held-key and button release, route closed")
}

/// Dedicated executable with fixed arguments, loopback TLS only, fresh keys,
/// separate enrollment barrier and explicit one-shot consent. Not called by
/// the normal diagnostic or native desktop command-line paths.
pub(super) async fn run()->MediaResult<()>{
    if cfg!(any(feature="native-control",feature="native-x11-view",feature="local-screen")){
        return Err("Inert browser fixture cannot include native desktop features".into());
    }
    if io::stdin().is_terminal()||io::stdout().is_terminal(){return Err("Fixture requires supervisor pipes".into());}
    let args:Vec<_>=std::env::args().skip(1).collect();
    let [mode,url,pin,flag,ca]=args.as_slice()else{return Err("Expected --host LOOPBACK_WSS SERVER_ID --ca CA_PEM".into());};
    if mode!="--host"||flag!="--ca"{return Err("Invalid browser fixture arguments".into());}
    let port=url.strip_prefix("wss://127.0.0.1:").and_then(|s|s.strip_suffix("/v1/ws")).ok_or("Only numeric IPv4 loopback WSS permitted")?;
    if port.is_empty()||!port.bytes().all(|b|b.is_ascii_digit())||port.parse::<u16>().ok().filter(|p|*p!=0).is_none(){return Err("Invalid loopback fixture port".into());}
    let endpoint=Endpoint::secure(url,decode_hex::<32>(pin).map_err(error)?)?;
    ctrlc::set_handler(||CANCEL.store(true,Ordering::Release)).map_err(error)?;
    let id=DeviceIdentity::generate();
    out(&format!("KEY {}",hex(&id.public_key())))?;
    if input()?!="REGISTER TEST"||canceled(){return Err("Test enrollment canceled".into());}
    let mut conn=Connection::register_with_roots(endpoint,&id,roots(ca)?).await?;
    out(&format!("ONLINE {}",conn.device_id()))?;
    out("PEER_KEY (independently verify the peer's entire public key)")?;
    let expected=decode_hex::<32>(&input()?).map_err(error)?;
    if expected==[0;32]||expected==id.public_key(){return Err("Invalid peer key".into());}
    let profile=Profile::BrowserControlFixture;
    let consent=profile.consent(true);
    out(&format!("CONSENT (type {consent}; one-shot inert test)"))?;
    if input()?!=consent||canceled(){return Err("Inert test consent denied".into());}
    std::thread::spawn(||{let mut b=[0u8;1];let _=io::stdin().lock().read(&mut b);CANCEL.store(true,Ordering::Release);});
    let resume=exchange::ResumeOptions::default();
    let result=host(&mut conn,&id,expected,false,profile,None,Vec::new(),"compat",None,75,&resume,false,false,300,None,None).await;
    let close=conn.close().await;result?;close
}
