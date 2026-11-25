//! Compact, time-limited connection hint. NOT a password or authorization.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD,Engine};
use remote_ex_security::{fingerprint,PublicKey};
use remote_ex_signaling::{decode_hex,hex};
use serde::{Deserialize,Serialize};
use std::time::{SystemTime,UNIX_EPOCH};
const PREFIX:&str="REX1-";
const TTL:u64=300;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Invitation{
    pub url:String,pub server_id:String,pub device_id:String,pub public_key:String,
    pub expires_at:u64,pub mode:String,
}
fn now()->Result<u64,String>{SystemTime::now().duration_since(UNIX_EPOCH).map(|d|d.as_secs()).map_err(|_|"Invalid system clock".into())}
fn key(s:&str)->Result<PublicKey,String>{
    let k=decode_hex::<32>(s).map_err(|_|"Invalid full identity")?;
    if k==[0;32]{return Err("Invalid zero identity".into());}
    Ok(k)
}
impl Invitation{
    pub fn create(url:&str,server:&str,public_key:&str,mode:&str)->Result<Self,String>{
        let k=key(public_key)?;
        let t=now()?;
        let i=Self{url:url.into(),server_id:hex(&key(server)?),device_id:hex(&fingerprint(&k)),public_key:hex(&k),expires_at:t.checked_add(TTL).ok_or("Invalid clock")?,mode:mode.into()};
        i.validate_at(t)?;Ok(i)
    }
    pub fn encode(&self)->Result<String,String>{
        self.validate_at(now()?)?;
        let data=serde_json::to_vec(self).map_err(|_|"Cannot encode invitation")?;
        Ok(format!("{PREFIX}{}",URL_SAFE_NO_PAD.encode(data)))
    }
    pub fn decode(code:&str)->Result<Self,String>{Self::decode_at(code,now()?)}
    fn decode_at(code:&str,t:u64)->Result<Self,String>{
        if code.len()>2048{return Err("Invitation too long".into());}
        let body=code.strip_prefix(PREFIX).ok_or("Invalid invitation prefix")?;
        let data=URL_SAFE_NO_PAD.decode(body).map_err(|_|"Invalid invitation encoding")?;
        let i:Self=serde_json::from_slice(&data).map_err(|_|"Invalid invitation structure")?;
        i.validate_at(t)?;Ok(i)
    }
    fn validate_at(&self,t:u64)->Result<(),String>{
        if self.url.len()>512||self.url.chars().any(char::is_control){return Err("Invalid invitation URL".into());}
        let u=url::Url::parse(&self.url).map_err(|_|"Invalid invitation URL")?;
        if u.scheme()!="wss"||u.host().is_none()||u.path()!="/v1/ws"||!u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some(){return Err("Invitation requires WSS server URL".into());}
        key(&self.server_id)?;
        if hex(&fingerprint(&key(&self.public_key)?))!=self.device_id{return Err("Invitation identity mismatch".into());}
        if self.expires_at<=t||self.expires_at>t.saturating_add(TTL){return Err("Invitation expired or invalid lifetime".into());}
        if !matches!(self.mode.as_str(),"test"|"view"|"control"){return Err("Invalid invitation profile".into());}
        Ok(())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn invitation_roundtrip_and_rejects_tampering(){
        let id=remote_ex_security::DeviceIdentity::generate();
        let i=Invitation::create("wss://example.test/v1/ws",&"01".repeat(32),&hex(&id.public_key()),"view").unwrap();
        assert_eq!(Invitation::decode(&i.encode().unwrap()).unwrap(),i);
        assert!(Invitation::decode_at(&i.encode().unwrap(),i.expires_at).is_err());
        let mut bad=i;bad.device_id="02".repeat(32);assert!(bad.encode().is_err());
    }
}
