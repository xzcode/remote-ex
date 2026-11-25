//! Server routing protocol v1 (development). IDs are full key fingerprints,
//! not secrets or proof of permission. Endpoint authentication remains mandatory.
use serde::{Deserialize, Serialize};
use remote_ex_security::session::{VIEW, INPUT};
use remote_ex_signaling::decode_hex;

pub const VERSION: u16 = 1;
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
    Request { target: String, requested: u8 },
    Decision { session: String, grant: Option<u8> },
    Signal { session: String, payload: Signal },
    Close { session: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Signal {
    Offer { sdp: String },
    Answer { sdp: String },
    Proof { signature: String },
    Ice { mline: u32, candidate: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Server {
    Challenge { server: String, nonce: String },
    Registered { device_id: String },
    Pong,
    Requested { session: String, target: String },
    Incoming { session: String, controller: String, controller_key: String, requested: u8 },
    Decision { session: String, grant: Option<u8>, host_key: String },
    Signal { session: String, payload: Signal },
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
            Self::Request { target, requested } if canonical::<32>(target)
                && grant_valid(*requested) => Ok(()),
            Self::Decision { session, grant } if canonical::<16>(session)
                && grant.is_none_or(grant_valid) => Ok(()),
            Self::Signal { session, payload } if canonical::<16>(session) => payload.validate(),
            Self::Close { session } if canonical::<16>(session) => Ok(()),
            _ => Err("invalid message"),
        }
    }
}
impl Signal {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Offer { sdp } | Self::Answer { sdp }
                if !sdp.is_empty() && sdp.len() <= MAX_SDP && !sdp.contains('\0') => Ok(()),
            Self::Proof { signature } if decode_hex::<64>(signature).is_ok() => Ok(()),
            Self::Ice { mline, candidate } if *mline <= 1 && candidate.len() <= MAX_ICE
                && !candidate.contains(['\r','\n','\0']) => Ok(()),
            _ => Err("invalid signal"),
        }
    }
}
