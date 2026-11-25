//! coturn REST credentials. Shared secret stays in the server process.
//! Issue only once per participant in a locally approved encrypted route.
use crate::protocol::{ice_url,IceConfig,TurnCredential};
use base64::{engine::general_purpose::STANDARD,Engine};
use hmac::{Hmac,Mac};
use sha1::Sha1;
use std::{io::Read,time::{SystemTime,UNIX_EPOCH}};
use zeroize::Zeroizing;
const TTL:u64=180;
const EXTENDED_TTL:u64=540;
pub struct IceIssuer { stun:Option<String>,turn:Option<(String,Zeroizing<Vec<u8>>)> }
impl IceIssuer {
    pub fn new(stun:Option<String>,turn:Option<(String,Vec<u8>)>)->Result<Self,&'static str>{
        if let Some(s)=&stun{ice_url(s,&["stun"])?;}
        let turn=if let Some((url,secret))=turn{
            let secret=Zeroizing::new(secret);
            ice_url(&url,&["turn","turns"])?;
            if secret.len()<32||secret.len()>256||!secret.iter().all(u8::is_ascii_graphic){return Err("TURN secret must be 32-256 printable non-space bytes");}
            Some((url,secret))
        }else{None};
        Ok(Self{stun,turn})
    }
    pub fn from_env()->Result<Self,Box<dyn std::error::Error>>{
        let stun=std::env::var("REMOTE_EX_STUN_URL").ok();
        let turn=match(std::env::var("REMOTE_EX_TURN_URL").ok(),std::env::var("REMOTE_EX_TURN_SECRET_FILE").ok()){
            (None,None)=>None,
            (Some(url),Some(path))=>{
                let mut secret=Zeroizing::new(Vec::new());
                std::fs::File::open(path)?.take(258).read_to_end(&mut secret)?;
                while matches!(secret.last(),Some(b'\n'|b'\r')){secret.pop();}
                Some((url,secret.to_vec()))
            }
            _=>return Err("TURN URL and secret file must both be configured".into()),
        };
        Ok(Self::new(stun,turn)?)
    }
    pub fn issue_for_route(&self,device:&str,session:&str,extended:bool)->Result<IceConfig,&'static str>{
        let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"system clock invalid")?.as_secs();
        self.issue_at(device,session,now,if extended{EXTENDED_TTL}else{TTL})
    }
    fn issue_at(&self,device:&str,session:&str,now:u64,ttl:u64)->Result<IceConfig,&'static str>{
        // Inputs originate only from authenticated, canonical hub identifiers.
        if device.len()!=64||session.len()!=32||!device.bytes().chain(session.bytes()).all(|b|b.is_ascii_hexdigit()){
            return Err("invalid TURN scope");
        }
        let turn=if let Some((url,secret))=&self.turn{
            let expires_at=now.checked_add(ttl).ok_or("expiry overflow")?;
            let username=format!("{expires_at}:{device}-{session}");
            let mut mac=Hmac::<Sha1>::new_from_slice(secret).map_err(|_|"TURN issuer error")?;
            mac.update(username.as_bytes());
            let password=STANDARD.encode(mac.finalize().into_bytes());
            Some(TurnCredential{url:url.clone(),username,password,expires_at})
        }else{None};
        let config=IceConfig{stun:self.stun.clone(),turn};config.validate()?;Ok(config)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn rest_credentials_have_expiry_scope_and_hmac(){
        let secret=vec![b'x';32];
        let issuer=IceIssuer::new(None,Some(("turn://relay.example.com:3478".into(),secret.clone()))).unwrap();
        let c=issuer.issue_at(&"01".repeat(32),&"02".repeat(16),1_700_000_000,TTL).unwrap().turn.unwrap();
        assert_eq!(c.expires_at,1_700_000_180);
        let extended=issuer.issue_at(&"01".repeat(32),&"02".repeat(16),1_700_000_000,EXTENDED_TTL).unwrap().turn.unwrap();assert_eq!(extended.expires_at,1_700_000_540);
        assert!(c.username.starts_with("1700000180:"));
        let mut mac=Hmac::<Sha1>::new_from_slice(&secret).unwrap();mac.update(c.username.as_bytes());
        assert_eq!(c.password,STANDARD.encode(mac.finalize().into_bytes()));
        assert!(issuer.issue_at("invalid",&"02".repeat(16),1,TTL).is_err());
    }
}
