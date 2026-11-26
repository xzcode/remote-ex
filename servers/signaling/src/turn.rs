//! coturn REST credentials. Shared secret stays in the server process.
//! Issue only once per participant in a locally approved encrypted route.
use crate::protocol::{ice_url,IceConfig,TurnCredential};
use base64::{engine::general_purpose::STANDARD,Engine};
use hmac::{Hmac,Mac};
use sha1::Sha1;
use std::{path::Path,time::{SystemTime,UNIX_EPOCH}};
use crate::relay_pool::{RelayPool,read_secret};
use zeroize::Zeroizing;
const TTL:u64=180;
const EXTENDED_TTL:u64=540;
pub struct IceIssuer { stun:Option<String>,turn:Option<(String,Zeroizing<Vec<u8>>)>,pool:Option<RelayPool> }
impl IceIssuer {
    pub fn new(stun:Option<String>,turn:Option<(String,Vec<u8>)>)->Result<Self,&'static str>{
        if let Some(s)=&stun{ice_url(s,&["stun"])?;}
        let turn=if let Some((url,secret))=turn{
            let secret=Zeroizing::new(secret);
            ice_url(&url,&["turn","turns"])?;
            if secret.len()<32||secret.len()>256||!secret.iter().all(u8::is_ascii_graphic){return Err("TURN secret must be 32-256 printable non-space bytes");}
            Some((url,secret))
        }else{None};
        Ok(Self{stun,turn,pool:None})
    }
    pub fn from_env()->Result<Self,Box<dyn std::error::Error>>{
        let stun=std::env::var("REMOTE_EX_STUN_URL").ok();
        let url=std::env::var("REMOTE_EX_TURN_URL").ok();
        let secret_path=std::env::var("REMOTE_EX_TURN_SECRET_FILE").ok();
        let pool=std::env::var("REMOTE_EX_RELAY_POOL_FILE").ok();
        let region=std::env::var("REMOTE_EX_RELAY_REGION").ok();
        if let Some(path)=pool{
            if url.is_some()||secret_path.is_some(){return Err("TURN pool and single TURN configuration are mutually exclusive".into());}
            let mut issuer=Self::new(stun,None)?;
            issuer.pool=Some(RelayPool::load(Path::new(&path),region.as_deref())?);
            return Ok(issuer);
        }
        if region.is_some(){return Err("Relay region requires a TURN pool file".into());}
        let turn=match(url,secret_path){
            (None,None)=>None,
            (Some(url),Some(path))=>Some((url,read_secret(Path::new(&path))?.to_vec())),
            _=>return Err("TURN URL and secret file must both be configured".into()),
        };
        Ok(Self::new(stun,turn)?)
    }
    pub fn relay_count(&self)->usize{self.pool.as_ref().map_or(if self.turn.is_some(){1}else{0},RelayPool::count)}
    pub fn eligible_relay_count(&self)->usize{self.pool.as_ref().map_or(if self.turn.is_some(){1}else{0},RelayPool::eligible_count)}
    pub fn issue_for_route(&self,device:&str,session:&str,extended:bool)->Result<IceConfig,&'static str>{
        let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"system clock invalid")?.as_secs();
        self.issue_at(device,session,now,if extended{EXTENDED_TTL}else{TTL})
    }
    fn issue_at(&self,device:&str,session:&str,now:u64,ttl:u64)->Result<IceConfig,&'static str>{
        // Inputs originate only from authenticated, canonical hub identifiers.
        if device.len()!=64||session.len()!=32||!device.bytes().chain(session.bytes()).all(|b|b.is_ascii_hexdigit()){
            return Err("invalid TURN scope");
        }
        let selected=self.pool.as_ref().map(|pool|pool.select(session)).or_else(||self.turn.as_ref().map(|(url,secret)|(url.as_str(),secret.as_slice())));
        let turn=if let Some((url,secret))=selected{
            let expires_at=now.checked_add(ttl).ok_or("expiry overflow")?;
            let username=format!("{expires_at}:{device}-{session}");
            let mut mac=Hmac::<Sha1>::new_from_slice(secret).map_err(|_|"TURN issuer error")?;
            mac.update(username.as_bytes());
            let password=STANDARD.encode(mac.finalize().into_bytes());
            Some(TurnCredential{url:url.to_owned(),username,password,expires_at})
        }else{None};
        let config=IceConfig{stun:self.stun.clone(),turn};config.validate()?;Ok(config)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn pool_issues_one_same_relay_and_distinct_scoped_credentials(){
        let dir=tempfile::tempdir().unwrap();
        let a=dir.path().join("a.secret");let b=dir.path().join("b.secret");
        std::fs::write(&a,"a".repeat(40)).unwrap();std::fs::write(&b,"b".repeat(40)).unwrap();
        let file=dir.path().join("pool.json");
        std::fs::write(&file,serde_json::to_vec(&serde_json::json!({"schema":1,"relays":[
            {"id":"a","region":"asia","priority":1,"url":"turns://a.example.net:5349","secret_file":a.to_str().unwrap()},
            {"id":"b","region":"asia","priority":1,"url":"turns://b.example.net:5349","secret_file":b.to_str().unwrap()}
        ]})).unwrap()).unwrap();
        let mut issuer=IceIssuer::new(None,None).unwrap();
        issuer.pool=Some(RelayPool::load(&file,Some("asia")).unwrap());
        let mut seen=std::collections::HashSet::new();
        for i in 1..=100{
            let session=format!("{i:032x}");
            let x=issuer.issue_at(&"11".repeat(32),&session,1000,TTL).unwrap().turn.unwrap();
            let y=issuer.issue_at(&"22".repeat(32),&session,1000,TTL).unwrap().turn.unwrap();
            assert_eq!(x.url,y.url);assert_ne!(x.username,y.username);assert_ne!(x.password,y.password);
            let secret=if x.url=="turns://a.example.net:5349"{b'a'}else{assert_eq!(x.url,"turns://b.example.net:5349");b'b'};
            for credential in [&x,&y]{
                let mut mac=Hmac::<Sha1>::new_from_slice(&[secret;40]).unwrap();mac.update(credential.username.as_bytes());
                assert_eq!(credential.password,STANDARD.encode(mac.finalize().into_bytes()));
                assert_eq!(credential.expires_at,1180);
            }seen.insert(x.url);
        }
        assert_eq!(seen.len(),2);
    }
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
