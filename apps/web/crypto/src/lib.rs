#![forbid(unsafe_code)]
//! Isolated synthetic-only Noise compatibility spike. Never opens a socket,
//! accepts an arbitrary application payload, grants input or registers devices.
//! Identity signatures are supplied by the local WebCrypto fixture; both
//! expected keys are known from creation. Only fixed profile 0 is accepted.
use remote_ex_security::{Role,route_binding::RouteBinding,profiles::NOISE_PATTERN};
use snow::{Builder,HandshakeState,TransportState};
use wasm_bindgen::prelude::*;
use zeroize::Zeroizing;
const FIXTURE:&[u8]=b"Remote EX/browser/fixture-only/v1";
type Result<T>=std::result::Result<T,String>;
fn exact<const N:usize>(bytes:&[u8])->Result<[u8;N]>{bytes.try_into().map_err(|_|"Invalid fixture field length".into())}
fn noise<T>(v:std::result::Result<T,snow::Error>)->Result<T>{v.map_err(|_|"Fixture Noise authentication failed".into())}
struct Peer{
    binding:RouteBinding,role:Role,handshake:Option<HandshakeState>,transport:Option<TransportState>,
    hash:Option<[u8;32]>,step:u8,proof_sent:bool,proof_received:bool,failed:bool,
}
impl Peer{
    fn new(role:Role,session:&[u8],controller:&[u8],host:&[u8],cn:&[u8],hn:&[u8])->Result<Self>{
        let binding=RouteBinding{session:exact(session)?,controller:exact(controller)?,host:exact(host)?,
            controller_nonce:exact(cn)?,host_nonce:exact(hn)?,requested:3,granted:3,profile:0,relay_only:false};
        let context=binding.context().map_err(|_|"Invalid fixture binding")?;
        if !remote_ex_security::valid_public_key(&binding.controller)||!remote_ex_security::valid_public_key(&binding.host){return Err("Invalid fixture public key".into());}
        let b=Builder::new(NOISE_PATTERN.parse().map_err(|_|"Noise pattern unavailable")?);
        let b=noise(b.prologue(&context))?;
        let hs=if role==Role::Controller{noise(b.build_initiator())?}else{noise(b.build_responder())?};
        Ok(Self{binding,role,handshake:Some(hs),transport:None,hash:None,step:0,proof_sent:false,proof_received:false,failed:false})
    }
    fn run<T>(&mut self,f:impl FnOnce(&mut Self)->Result<T>)->Result<T>{
        if self.failed{return Err("Fixture closed".into());}
        let r=f(self);
        if r.is_err(){self.close();}
        r
    }
    fn close(&mut self){self.failed=true;self.handshake=None;self.transport=None;self.hash=None;self.proof_sent=false;self.proof_received=false;}
    fn finish(&mut self)->Result<()>{
        let hs=self.handshake.take().ok_or("No fixture handshake")?;
        if !hs.is_handshake_finished(){return Err("Incomplete fixture handshake".into());}
        self.hash=Some(exact(hs.get_handshake_hash())?);
        self.transport=Some(noise(hs.into_transport_mode())?);Ok(())
    }
    fn write(&mut self)->Result<Vec<u8>>{self.run(|s|{
        let expected=match (s.role,s.step){(Role::Controller,0)=>32,(Role::Host,1)=>48,_=>return Err("Unexpected fixture write".into())};
        let mut out=[0u8;64];let n=noise(s.handshake.as_mut().ok_or("Handshake closed")?.write_message(&[],&mut out))?;
        if n!=expected{return Err("Invalid fixture handshake length".into());}
        s.step+=1;if s.step==2{s.finish()?;}Ok(out[..n].to_vec())
    })}
    fn read(&mut self,packet:&[u8])->Result<()>{self.run(|s|{
        let expected=match (s.role,s.step){(Role::Host,0)=>32,(Role::Controller,1)=>48,_=>return Err("Unexpected fixture read".into())};
        if packet.len()!=expected{return Err("Invalid fixture packet length".into());}
        let mut out=Zeroizing::new([0u8;64]);
        if noise(s.handshake.as_mut().ok_or("Handshake closed")?.read_message(packet,&mut out[..]))?!=0{return Err("Unexpected fixture payload".into());}
        s.step+=1;if s.step==2{s.finish()?;}Ok(())
    })}
    fn signature_message(&self)->Result<Vec<u8>>{
        if self.failed{return Err("Fixture closed".into());}
        let hash=self.hash.ok_or("Handshake not complete")?;
        let mut message=self.binding.context().map_err(|_|"Invalid fixture context")?;
        message.push(self.role as u8);message.extend_from_slice(&hash);Ok(message)
    }
    fn send_proof(&mut self,signature:&[u8])->Result<Vec<u8>>{self.run(|s|{
        if s.proof_sent{return Err("Fixture proof already sent".into());}
        let sig=exact::<64>(signature)?;
        s.binding.verify(s.role,&s.hash.ok_or("Handshake not complete")?,&sig).map_err(|_|"Wrong fixture signing key")?;
        let mut out=[0u8;80];let n=noise(s.transport.as_mut().ok_or("Transport unavailable")?.write_message(&sig,&mut out))?;
        if n!=80{return Err("Invalid fixture proof size".into());}s.proof_sent=true;Ok(out.to_vec())
    })}
    fn receive_proof(&mut self,packet:&[u8])->Result<()>{self.run(|s|{
        if !s.proof_sent||s.proof_received||packet.len()!=80{return Err("Unexpected fixture proof".into());}
        let mut plain=Zeroizing::new([0u8;80]);
        let n=noise(s.transport.as_mut().ok_or("Transport unavailable")?.read_message(packet,&mut plain[..]))?;
        if n!=64{return Err("Invalid fixture proof".into());}
        let other=if s.role==Role::Controller{Role::Host}else{Role::Controller};
        s.binding.verify(other,&s.hash.ok_or("Handshake not complete")?,&exact(&plain[..n])?).map_err(|_|"Peer fixture signature mismatch")?;
        s.proof_received=true;Ok(())
    })}
    fn ready(&self)->bool{!self.failed&&self.proof_sent&&self.proof_received&&self.transport.is_some()}
    fn send_fixture(&mut self)->Result<Vec<u8>>{self.run(|s|{
        if !s.ready(){return Err("Mutual fixture proof required".into());}
        let mut out=vec![0;FIXTURE.len()+16];
        let n=noise(s.transport.as_mut().ok_or("Transport unavailable")?.write_message(FIXTURE,&mut out))?;
        if n!=out.len(){return Err("Invalid fixture ciphertext length".into());}Ok(out)
    })}
    fn receive_fixture(&mut self,packet:&[u8])->Result<()>{self.run(|s|{
        if !s.ready()||packet.len()!=FIXTURE.len()+16{return Err("Invalid fixture record".into());}
        let mut out=Zeroizing::new(vec![0;packet.len()]);
        let n=noise(s.transport.as_mut().ok_or("Transport unavailable")?.read_message(packet,&mut out))?;
        if &out[..n]!=FIXTURE{return Err("Unexpected fixture plaintext".into());}Ok(())
    })}
}
#[wasm_bindgen]
pub struct FixturePeer{inner:Peer}
fn js<T>(r:Result<T>)->std::result::Result<T,JsValue>{r.map_err(|e|JsValue::from_str(&e))}
#[wasm_bindgen]
impl FixturePeer{
    #[wasm_bindgen(constructor)]
    pub fn new(controller_role:bool,session:&[u8],controller:&[u8],host:&[u8],cn:&[u8],hn:&[u8])->std::result::Result<FixturePeer,JsValue>{
        js(Peer::new(if controller_role{Role::Controller}else{Role::Host},session,controller,host,cn,hn).map(|inner|Self{inner}))
    }
    pub fn write_handshake(&mut self)->std::result::Result<Vec<u8>,JsValue>{js(self.inner.write())}
    pub fn read_handshake(&mut self,packet:&[u8])->std::result::Result<(),JsValue>{js(self.inner.read(packet))}
    pub fn signature_message(&self)->std::result::Result<Vec<u8>,JsValue>{js(self.inner.signature_message())}
    pub fn send_proof(&mut self,signature:&[u8])->std::result::Result<Vec<u8>,JsValue>{js(self.inner.send_proof(signature))}
    pub fn receive_proof(&mut self,packet:&[u8])->std::result::Result<(),JsValue>{js(self.inner.receive_proof(packet))}
    pub fn ready(&self)->bool{self.inner.ready()}
    pub fn send_fixture(&mut self)->std::result::Result<Vec<u8>,JsValue>{js(self.inner.send_fixture())}
    pub fn receive_fixture(&mut self,packet:&[u8])->std::result::Result<(),JsValue>{js(self.inner.receive_fixture(packet))}
    pub fn close(&mut self){self.inner.close();}
}
#[cfg(test)]
mod tests{
    use super::*;
    use remote_ex_security::DeviceIdentity;
    fn peers()->(Peer,Peer,DeviceIdentity,DeviceIdentity){
        let c=DeviceIdentity::generate();let h=DeviceIdentity::generate();
        let create=|r|Peer::new(r,&[7;16],&c.public_key(),&h.public_key(),&[8;32],&[9;32]).unwrap();
        (create(Role::Controller),create(Role::Host),c,h)
    }
    #[test]fn mutual_proof_gates_fixture_and_replay_closes(){
        let(mut c,mut h,ci,hi)=peers();
        h.read(&c.write().unwrap()).unwrap();c.read(&h.write().unwrap()).unwrap();
        assert_eq!(c.hash,h.hash);assert!(!c.ready()&&!h.ready());
        let a=c.binding.sign(&ci,Role::Controller,&c.hash.unwrap()).unwrap();
        let b=h.binding.sign(&hi,Role::Host,&h.hash.unwrap()).unwrap();
        let ap=c.send_proof(&a).unwrap();let bp=h.send_proof(&b).unwrap();
        c.receive_proof(&bp).unwrap();h.receive_proof(&ap).unwrap();
        assert!(c.ready()&&h.ready());
        let packet=c.send_fixture().unwrap();h.receive_fixture(&packet).unwrap();
        assert!(h.receive_fixture(&packet).is_err());assert!(!h.ready());
        let(mut c,_,_,_)=peers();assert!(c.send_fixture().is_err());assert!(c.write().is_err());
    }
    #[test]fn changed_context_and_invalid_proof_are_rejected(){
        let c_id=DeviceIdentity::generate();let h_id=DeviceIdentity::generate();
        let mut c=Peer::new(Role::Controller,&[7;16],&c_id.public_key(),&h_id.public_key(),&[8;32],&[9;32]).unwrap();
        let mut h=Peer::new(Role::Host,&[6;16],&c_id.public_key(),&h_id.public_key(),&[8;32],&[9;32]).unwrap();
        h.read(&c.write().unwrap()).unwrap();
        assert!(c.read(&h.write().unwrap()).is_err());assert!(!c.ready());
        let(mut c,mut h,ci,_)=peers();
        h.read(&c.write().unwrap()).unwrap();c.read(&h.write().unwrap()).unwrap();
        let mut sig=c.binding.sign(&ci,Role::Controller,&c.hash.unwrap()).unwrap();sig[0]^=1;
        assert!(c.send_proof(&sig).is_err());assert!(!c.ready());
    }
}

#[cfg(all(not(target_arch="wasm32"),feature="fixture-cli"))]
mod native_fixture;
#[cfg(all(not(target_arch="wasm32"),feature="fixture-cli"))]
pub use native_fixture::run_native_fixture;
