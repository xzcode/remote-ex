//! Narrow synthetic-only subset of v1 Signal JSON. Host is the offerer.
//! Ordered records: host Offer, controller Answer, host Proof, controller
//! Proof. Both signatures over the exact original bytes must pass the
//! production verifier. No general-purpose encrypted messages or ICE API.
use super::Result;
use remote_ex_security::{Role,SessionTranscript,hash_sdp,verify_session,route_binding::RouteBinding,profiles::MAX_SEALED_CLEAR};
use serde::{Deserialize,Serialize};
#[derive(Serialize,Deserialize)]
#[serde(tag="type",rename_all="snake_case",deny_unknown_fields)]
enum Record{Offer{sdp:String},Answer{sdp:String},Proof{signature:String}}
#[derive(Default)]
pub(super) struct SdpState{step:u8,offer:Option<String>,answer:Option<String>,host_proof:Option<[u8;64]>,verified:bool}
fn encode(record:&Record)->Result<Vec<u8>>{
    let data=serde_json::to_vec(record).map_err(|_|"Cannot encode SDP fixture")?;
    if data.len()>MAX_SEALED_CLEAR{return Err("SDP fixture exceeds record limit".into());}Ok(data)
}
fn decode(data:&[u8])->Result<Record>{
    if data.is_empty()||data.len()>MAX_SEALED_CLEAR{return Err("Invalid SDP fixture size".into());}
    serde_json::from_slice(data).map_err(|_|"Invalid or unsupported SDP fixture record".into())
}
fn valid(sdp:&str)->bool{
    !sdp.is_empty()&&sdp.len()<=MAX_SEALED_CLEAR&&!sdp.contains('\0')
        &&sdp.starts_with("v=0\r\n")&&sdp.lines().filter(|l|l.starts_with("m=")).count()<=2
        &&sdp.lines().filter(|l|l.starts_with("m=video ")).count()==1
        &&sdp.lines().filter(|l|l.starts_with("m=application ")).count()<=1
        &&sdp.lines().all(|l|!l.starts_with("m=")||l.starts_with("m=video ")||l.starts_with("m=application "))
}
fn hex(bytes:&[u8])->String{bytes.iter().map(|b|format!("{b:02x}")).collect()}
fn signature(s:&str)->Result<[u8;64]>{
    if s.len()!=128||!s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err("Invalid proof hex".into());}
    let mut out=[0;64];
    for(i,b)in out.iter_mut().enumerate(){*b=u8::from_str_radix(&s[2*i..2*i+2],16).map_err(|_|"Invalid proof hex")?;}Ok(out)
}
impl SdpState{
    pub fn started(&self)->bool{self.step!=0}
    pub fn verified(&self)->bool{self.verified&&self.step==4}
    pub fn send_description(&mut self,role:Role,sdp:&str)->Result<Vec<u8>>{
        if !valid(sdp){return Err("Invalid generated video-only SDP".into());}
        let record=match(role,self.step){
            (Role::Host,0)=>{self.offer=Some(sdp.to_owned());Record::Offer{sdp:sdp.into()}},
            (Role::Controller,1)=>{self.answer=Some(sdp.to_owned());Record::Answer{sdp:sdp.into()}},
            _=>return Err("Unexpected outgoing SDP fixture state".into()),
        };
        let data=encode(&record)?;self.step+=1;Ok(data)
    }
    pub fn receive_description(&mut self,role:Role,data:&[u8])->Result<String>{
        let sdp=match(role,self.step,decode(data)?){
            (Role::Controller,0,Record::Offer{sdp}) if valid(&sdp)=>{self.offer=Some(sdp.clone());sdp},
            (Role::Host,1,Record::Answer{sdp}) if valid(&sdp)=>{self.answer=Some(sdp.clone());sdp},
            _=>return Err("Unexpected incoming SDP fixture state".into()),
        };self.step+=1;Ok(sdp)
    }
    fn verify_pair(&self,b:&RouteBinding,controller:&[u8;64],host:&[u8;64])->Result<()>{
        let o=self.offer.as_ref().ok_or("Missing exact fixture offer")?.as_bytes();
        let a=self.answer.as_ref().ok_or("Missing exact fixture answer")?.as_bytes();
        let t=SessionTranscript{session_id:b.session,controller_key:b.controller,host_key:b.host,controller_nonce:b.controller_nonce,host_nonce:b.host_nonce,
            requested:b.requested,granted:b.granted,offer_sha256:hash_sdp(o).map_err(|_|"Invalid fixture offer")?,answer_sha256:hash_sdp(a).map_err(|_|"Invalid fixture answer")?};
        verify_session(t,&b.controller,&b.host,o,a,controller,host).map_err(|_|"Mutual exact SDP signature mismatch")?;Ok(())
    }
    pub fn send_proof(&mut self,role:Role,b:&RouteBinding,sig:[u8;64])->Result<Vec<u8>>{
        match(role,self.step){
            (Role::Host,2)=>{self.host_proof=Some(sig);},
            (Role::Controller,3)=>{self.verify_pair(b,&sig,&self.host_proof.ok_or("Missing host SDP proof")?)?;self.verified=true;},
            _=>return Err("Unexpected outgoing SDP proof state".into()),
        }
        let data=encode(&Record::Proof{signature:hex(&sig)})?;self.step+=1;Ok(data)
    }
    pub fn receive_proof(&mut self,role:Role,b:&RouteBinding,data:&[u8])->Result<()>{
        let Record::Proof{signature:value}=decode(data)?else{return Err("Expected SDP proof record".into());};
        let sig=signature(&value)?;
        match(role,self.step){
            (Role::Controller,2)=>{self.host_proof=Some(sig);},
            (Role::Host,3)=>{self.verify_pair(b,&sig,&self.host_proof.ok_or("Missing host SDP proof")?)?;self.verified=true;},
            _=>return Err("Unexpected incoming SDP proof state".into()),
        }self.step+=1;Ok(())
    }
}
#[cfg(test)]mod tests{
    use super::*;
    use remote_ex_security::DeviceIdentity;
    #[test]fn proof_order_and_exact_byte_binding(){
        let c=DeviceIdentity::generate();let h=DeviceIdentity::generate();
        let b=RouteBinding{session:[7;16],controller:c.public_key(),host:h.public_key(),controller_nonce:[8;32],host_nonce:[9;32],requested:3,granted:3,profile:0,relay_only:false};
        let o="v=0\r\ns=synthetic offer\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n";
        let a="v=0\r\ns=synthetic answer\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n";
        let t=SessionTranscript{session_id:b.session,controller_key:b.controller,host_key:b.host,controller_nonce:b.controller_nonce,host_nonce:b.host_nonce,requested:3,granted:3,offer_sha256:hash_sdp(o.as_bytes()).unwrap(),answer_sha256:hash_sdp(a.as_bytes()).unwrap()};
        let cs=c.sign_session(Role::Controller,&t).unwrap();let hs=h.sign_session(Role::Host,&t).unwrap();
        let mut x=SdpState::default();let mut y=SdpState::default();
        assert!(x.send_proof(Role::Controller,&b,cs).is_err());
        let p=y.send_description(Role::Host,o).unwrap();assert_eq!(x.receive_description(Role::Controller,&p).unwrap(),o);
        let p=x.send_description(Role::Controller,a).unwrap();assert_eq!(y.receive_description(Role::Host,&p).unwrap(),a);
        let p=y.send_proof(Role::Host,&b,hs).unwrap();x.receive_proof(Role::Controller,&b,&p).unwrap();assert!(!x.verified()&&!y.verified());
        let mut wrong=cs;wrong[0]^=1;assert!(x.send_proof(Role::Controller,&b,wrong).is_err());
        let p=x.send_proof(Role::Controller,&b,cs).unwrap();y.receive_proof(Role::Host,&b,&p).unwrap();assert!(x.verified()&&y.verified());
        assert!(x.receive_proof(Role::Controller,&b,&p).is_err());
        assert!(!valid("v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 96\r\n"));
    }
}
