//! Server routing protocol v1 (development). IDs are full key fingerprints,
//! not secrets or proof of permission. Endpoint authentication remains mandatory.
use serde::{Deserialize, Serialize};
use remote_ex_security::session::{VIEW, INPUT};
use remote_ex_signaling::decode_hex;

pub const VERSION: u16 = 1;
pub const MAX_MESSAGE: usize = 96 * 1024;
pub const MAX_SDP: usize = 48 * 1024;
pub const MAX_ICE: usize = 4096;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientFrame { pub version: u16, pub message: Client }
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerFrame { pub version: u16, pub message: Server }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Client {
    Register { public_key: String, signature: String },
    Ping,
    Request { target: String, requested: u8, controller_nonce: String },
    Decision { session: String, grant: Option<u8>, host_nonce: Option<String> },
    Signal { session: String, payload: Signal },
    IceConfig { session: String },
    Close { session: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Signal {
    Offer { sdp: String },
    Answer { sdp: String },
    Proof { signature: String },
    Ice { mline: u32, candidate: String },
    IceBatch { items: Vec<IceItem> },
    NoiseHandshake { step: u8, packet: String },
    Sealed { packet: String },
    Geometry { x:u32,y:u32,width:u32,height:u32 },
}

/// Bounded non-trickle candidate exchange after both session proofs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IceItem { pub mline: u32, pub candidate: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Server {
    Challenge { server: String, nonce: String },
    Registered { device_id: String },
    Pong,
    Requested { session: String, target: String },
    Incoming { session: String, controller: String, controller_key: String, requested: u8, controller_nonce: String },
    Decision { session: String, grant: Option<u8>, host_key: String, host_nonce: Option<String> },
    Signal { session: String, payload: Signal },
    IceConfig { session: String, config: IceConfig },
    Closed { session: String, reason: String },
    Error { code: String },
}
pub fn grant_valid(flags: u8) -> bool {
    flags & VIEW != 0 && flags & !(VIEW | INPUT) == 0
}
fn canonical<const N: usize>(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && decode_hex::<N>(s).is_ok_and(|bytes| bytes.iter().any(|&b| b != 0))
}
impl Client {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Register { public_key, signature } if canonical::<32>(public_key)
                && decode_hex::<64>(signature).is_ok() => Ok(()),
            Self::Ping => Ok(()),
            Self::Request { target, requested, controller_nonce } if canonical::<32>(target)
                && canonical::<32>(controller_nonce) && grant_valid(*requested) => Ok(()),
            Self::Decision { session, grant, host_nonce } if canonical::<16>(session)
                && grant.is_none_or(grant_valid)
                && grant.is_some() == host_nonce.is_some()
                && host_nonce.as_ref().is_none_or(|nonce| canonical::<32>(nonce)) => Ok(()),
            Self::Signal { session, payload } if canonical::<16>(session) => payload.validate(),
            Self::IceConfig { session } | Self::Close { session } if canonical::<16>(session) => Ok(()),
            _ => Err("invalid message"),
        }
    }
}
impl Signal {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Offer { sdp } | Self::Answer { sdp }
                if !sdp.is_empty() && sdp.len() <= MAX_SDP && !sdp.contains('\0') => Ok(()),
            Self::Proof { signature } if decode_hex::<64>(signature).is_ok() => Ok(()),
            Self::Ice { mline, candidate } if *mline <= 1 && candidate.len() <= MAX_ICE
                && !candidate.contains(['\r','\n','\0']) => Ok(()),
            Self::IceBatch { items } if !items.is_empty() && items.len() <= 128
                && items.iter().map(|i| i.candidate.len()).sum::<usize>() <= 16 * 1024
                && items.iter().any(|i| !i.candidate.is_empty())
                && items.iter().all(|i| i.mline <= 1 && i.candidate.len() <= MAX_ICE
                    && !i.candidate.contains(['\r','\n','\0'])) => Ok(()),
            Self::Geometry{x,y,width,height} if *width>=2&&*height>=2
                &&x.checked_add(*width).is_some_and(|n|n<=960)&&y.checked_add(*height).is_some_and(|n|n<=540)=>Ok(()),
            Self::NoiseHandshake { step:1, packet } if packet.len()==64 && hex_packet(packet) => Ok(()),
            Self::NoiseHandshake { step:2, packet } if packet.len()==96 && hex_packet(packet) => Ok(()),
            Self::Sealed { packet } if (32..=65568).contains(&packet.len()) && hex_packet(packet) => Ok(()),
            _ => Err("invalid signal"),
        }
    }
}

impl Server {
    /// Reject malformed server messages before exposing any to session owners.
    /// Validation is not proof of peer identity or capture/control permission.
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Challenge { server, nonce } if canonical::<32>(server) && canonical::<32>(nonce) => Ok(()),
            Self::Registered { device_id } if canonical::<32>(device_id) => Ok(()),
            Self::Pong => Ok(()),
            Self::Requested { session, target } if canonical::<16>(session) && canonical::<32>(target) => Ok(()),
            Self::Incoming { session, controller, controller_key, requested, controller_nonce }
                if canonical::<16>(session) && canonical::<32>(controller)
                && canonical::<32>(controller_key) && canonical::<32>(controller_nonce)
                && grant_valid(*requested) => Ok(()),
            Self::Decision { session, grant, host_key, host_nonce }
                if canonical::<16>(session) && canonical::<32>(host_key) && grant.is_none_or(grant_valid)
                && grant.is_some() == host_nonce.is_some()
                && host_nonce.as_ref().is_none_or(|n| canonical::<32>(n)) => Ok(()),
            Self::Signal { session, payload } if canonical::<16>(session) => payload.validate(),
            Self::IceConfig { session, config } if canonical::<16>(session) => config.validate(),
            Self::Closed { session, reason } if canonical::<16>(session) && short_code(reason) => Ok(()),
            Self::Error { code } if short_code(code) => Ok(()),
            _ => Err("invalid server message"),
        }
    }
}
fn short_code(s: &str)->bool{
    !s.is_empty() && s.len()<=64 && s.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'_')
}

fn hex_packet(s:&str)->bool{s.len()%2==0&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}

// Credentials are short-lived secrets. Avoid Debug output of passwords.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnCredential { pub url:String, pub username:String, pub password:String, pub expires_at:u64 }
impl std::fmt::Debug for TurnCredential {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{
        f.debug_struct("TurnCredential").field("url",&self.url).field("expires_at",&self.expires_at).finish_non_exhaustive()
    }
}
#[derive(Clone,Default,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IceConfig { pub stun:Option<String>, pub turn:Option<TurnCredential> }

/// Restrict server addresses to explicitly configured host:port URIs. No
/// embedded credentials, paths, fragments, query, or implicit public service.
pub fn ice_url(value:&str,schemes:&[&str])->Result<url::Url,&'static str>{
    if value.len()>512||value.chars().any(char::is_control){return Err("invalid ICE server URL");}
    let u=url::Url::parse(value).map_err(|_|"invalid ICE server URL")?;
    if !schemes.contains(&u.scheme())||u.host().is_none()||u.port().is_none_or(|p|p==0)
        ||!u.username().is_empty()||u.password().is_some()
        ||!matches!(u.path(),""|"/")||u.query().is_some()||u.fragment().is_some(){
        return Err("invalid ICE server URL");
    }
    Ok(u)
}
impl IceConfig {
    pub fn validate(&self)->Result<(),&'static str>{
        if let Some(stun)=&self.stun{ice_url(stun,&["stun"])?;}
        if let Some(turn)=&self.turn{
            ice_url(&turn.url,&["turn","turns"])?;
            if turn.username.is_empty()||turn.username.len()>160||!turn.username.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b':'|b'-'|b'_'))
                ||turn.password.len()!=28||!turn.password.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'+'|b'/'|b'='))
                ||turn.expires_at==0{return Err("invalid TURN credential");}
        }
        Ok(())
    }
}
#[cfg(test)]
mod ice_tests {
    use super::*;
    #[test]
    fn urls_reject_credentials_paths_and_implicit_ports(){
        assert!(ice_url("stun://127.0.0.1:3478",&["stun"]).is_ok());
        assert!(ice_url("turns://relay.example.com:5349",&["turns"]).is_ok());
        assert!(ice_url("turn://user:pass@relay.example.com:3478",&["turn"]).is_err());
        assert!(ice_url("stun://example.com",&["stun"]).is_err());
        assert!(ice_url("turn://example.com:3478?transport=udp",&["turn"]).is_err());
    }
}
