//! Noise NN + mutual Ed25519 authentication of the Noise handshake hash.
//! The server only routes ciphertext. Both peers must independently verify
//! the expected Ed25519 public keys and explicitly approve the same grant.
//! No SDP, ICE, media or application data before both signatures verify.
use crate::{Connection,Result};
use remote_ex_rendezvous_protocol::{Client,Server,Signal};
use remote_ex_security::{DeviceIdentity,Role,route_binding::RouteBinding};
use remote_ex_signaling::hex;
use snow::{Builder,TransportState};
use std::time::Duration;
use zeroize::Zeroizing;
const PATTERN:&str=remote_ex_security::profiles::NOISE_PATTERN;
const MAX_CLEAR:usize=remote_ex_security::profiles::MAX_SEALED_CLEAR;
fn fail<T>(r:std::result::Result<T,snow::Error>)->Result<T>{
    r.map_err(|_|"Routed Noise authentication failed".into())
}
fn unhex(s:&str,max:usize)->Result<Vec<u8>>{
    if s.len()%2!=0||s.len()/2>max||!s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){
        return Err("Invalid encrypted record".into());
    }
    s.as_bytes().chunks_exact(2).map(|b|{
        let n=|c:u8|if c.is_ascii_digit(){c-b'0'}else{c-b'a'+10};
        Ok((n(b[0])<<4)|n(b[1]))
    }).collect()
}
async fn relay(conn:&mut Connection,session:&str,payload:Signal)->Result<()>{
    conn.send(Client::Signal{session:session.into(),payload}).await
}
async fn recv(conn:&mut Connection,session:&str)->Result<Signal>{recv_limit(conn,session,Duration::from_secs(15)).await}
async fn recv_limit(conn:&mut Connection,session:&str,limit:Duration)->Result<Signal>{
    match conn.receive(limit).await?{
        Server::Signal{session:id,payload} if id==session=>Ok(payload),
        _=>Err("Unexpected routed record or closed session".into()),
    }
}
async fn hs_recv(conn:&mut Connection,session:&str,expected:u8)->Result<Vec<u8>>{
    match recv(conn,session).await?{
        Signal::NoiseHandshake{step,packet} if step==expected=>unhex(&packet,48),
        _=>Err("Unexpected handshake record".into()),
    }
}
pub struct RouteCipher{state:TransportState,session:String,failed:bool}
impl RouteCipher{
    pub async fn establish(conn:&mut Connection,binding:&RouteBinding,id:&DeviceIdentity,role:Role)->Result<Self>{
        let session=hex(&binding.session);
        if id.public_key()!=binding.key(role){return Err("Wrong local route key".into());}
        let context=binding.context().map_err(|_|"Invalid route binding")?;
        let builder=Builder::new(PATTERN.parse().map_err(|_|"Noise configuration failed")?);
        let builder=fail(builder.prologue(&context))?;
        let mut hs=if role==Role::Controller{fail(builder.build_initiator())?}else{fail(builder.build_responder())?};
        let mut out=[0u8;128];let mut plain=[0u8;128];
        if role==Role::Controller{
            let n=fail(hs.write_message(&[],&mut out))?;
            relay(conn,&session,Signal::NoiseHandshake{step:1,packet:hex(&out[..n])}).await?;
            let incoming=hs_recv(conn,&session,2).await?;
            if fail(hs.read_message(&incoming,&mut plain))?!=0{return Err("Unexpected Noise payload".into());}
        }else{
            let incoming=hs_recv(conn,&session,1).await?;
            if fail(hs.read_message(&incoming,&mut plain))?!=0{return Err("Unexpected Noise payload".into());}
            let n=fail(hs.write_message(&[],&mut out))?;
            relay(conn,&session,Signal::NoiseHandshake{step:2,packet:hex(&out[..n])}).await?;
        }
        if !hs.is_handshake_finished(){return Err("Noise handshake incomplete".into());}
        let hash:[u8;32]=hs.get_handshake_hash().try_into().map_err(|_|"Invalid handshake hash")?;
        let signature=binding.sign(id,role,&hash).map_err(|_|"Cannot sign route binding")?;
        let mut channel=Self{state:fail(hs.into_transport_mode())?,session,failed:false};
        // Authentication records contain only the domain-bound signature,
        // not any SDP or candidate data. Both peers send then verify.
        channel.seal(conn,&signature).await?;
        let peer_signature=channel.open(conn).await?;
        let peer_sig:&[u8;64]=peer_signature.as_slice().try_into().map_err(|_|"Invalid route signature length")?;
        let other=if role==Role::Controller{Role::Host}else{Role::Controller};
        binding.verify(other,&hash,peer_sig).map_err(|_|"Peer route signature mismatch")?;
        Ok(channel)
    }
    async fn seal(&mut self,conn:&mut Connection,data:&[u8])->Result<()>{
        if self.failed||data.is_empty()||data.len()>MAX_CLEAR{return Err("Encrypted route unavailable or oversized record".into());}
        let mut bytes=vec![0;data.len()+16];
        let result=async{
            let n=fail(self.state.write_message(data,&mut bytes))?;
            relay(conn,&self.session,Signal::Sealed{packet:hex(&bytes[..n])}).await
        }.await;
        if result.is_err(){self.failed=true;}result
    }
    async fn open(&mut self,conn:&mut Connection)->Result<Zeroizing<Vec<u8>>>{self.open_limit(conn,Duration::from_secs(15)).await}
    async fn open_limit(&mut self,conn:&mut Connection,limit:Duration)->Result<Zeroizing<Vec<u8>>>{
        if self.failed{return Err("Encrypted route closed".into());}
        let result=async{
            let Signal::Sealed{packet}=recv_limit(conn,&self.session,limit).await? else{return Err("Plaintext downgrade refused".into());};
            let bytes=unhex(&packet,MAX_CLEAR+16)?;
            if bytes.len()<16{return Err("Truncated encrypted record".into());}
            let mut clear=Zeroizing::new(vec![0;MAX_CLEAR]);
            let n=fail(self.state.read_message(&bytes,&mut clear))?;
            clear.truncate(n);Ok(clear)
        }.await;
        if result.is_err(){self.failed=true;}result
    }
    pub async fn send(&mut self,conn:&mut Connection,payload:Signal)->Result<()>{
        if matches!(payload,Signal::NoiseHandshake{..}|Signal::Sealed{..}){return Err("Nested encryption record refused".into());}
        payload.validate().map_err(str::to_owned)?;
        let clear=Zeroizing::new(serde_json::to_vec(&payload).map_err(|_|"Invalid inner signal")?);
        self.seal(conn,&clear).await
    }
    pub async fn receive(&mut self,conn:&mut Connection)->Result<Signal>{
        self.receive_limit(conn,Duration::from_secs(15)).await
    }
    /// Bounded wait for a fresh local OS source picker, never indefinite.
    pub async fn receive_limit(&mut self,conn:&mut Connection,limit:Duration)->Result<Signal>{
        let clear=self.open_limit(conn,limit.min(Duration::from_secs(45))).await?;
        let result=(||{
            let payload:Signal=serde_json::from_slice(&clear).map_err(|_|"Invalid inner signal")?;
            if matches!(payload,Signal::NoiseHandshake{..}|Signal::Sealed{..}){return Err("Nested encryption record refused".into());}
            payload.validate().map_err(str::to_owned)?;Ok(payload)
        })();
        if result.is_err(){self.failed=true;}result
    }
}
