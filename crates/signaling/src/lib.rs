#![forbid(unsafe_code)]
//! Bounded, versioned LAN diagnostic signaling. TCP framing is not encryption
//! or authentication. Only public keys, random challenges, SDP, ICE and
//! signatures are exchanged; the session owner verifies both signatures over
//! the exact SDP before allowing any privileged media or data.
//! This is not a production listener, TLS service, registration or TURN relay.
use remote_ex_security::{PublicKey, session::{INPUT, VIEW}};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    time::{Duration, Instant},
};

pub const VERSION: u16 = 1;
pub const MAX_FRAME: usize = 256 * 1024;
pub const MAX_SDP: usize = 64 * 1024;
pub const MAX_CANDIDATES: usize = 128;
pub const MAX_CANDIDATE: usize = 4096;
const KNOWN_GRANTS: u8 = VIEW | INPUT;
const IO_BUDGET: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub mline: u32,
    pub candidate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Message {
    Request { session: [u8; 16], controller: PublicKey, nonce: [u8; 32], requested: u8 },
    Accept { session: [u8; 16], host: PublicKey, nonce: [u8; 32], granted: u8 },
    Offer { session: [u8; 16], sdp: String },
    Answer { session: [u8; 16], sdp: String },
    Proof { session: [u8; 16], signature: String },
    Candidates { session: [u8; 16], items: Vec<Candidate> },
    Finished { session: [u8; 16] },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope { version: u16, message: Message }

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}
fn flags_valid(flags: u8) -> bool { flags & VIEW != 0 && flags & !KNOWN_GRANTS == 0 }

impl Message {
    pub fn session(&self) -> [u8; 16] {
        match self {
            Self::Request { session, .. } | Self::Accept { session, .. }
            | Self::Offer { session, .. } | Self::Answer { session, .. }
            | Self::Proof { session, .. } | Self::Candidates { session, .. }
            | Self::Finished { session } => *session,
        }
    }
    pub fn validate(&self) -> io::Result<()> {
        if self.session() == [0; 16] { return Err(invalid("Invalid session ID")); }
        match self {
            Self::Request { controller, nonce, requested, .. }
                if *controller == [0; 32] || *nonce == [0; 32] || !flags_valid(*requested)
                => Err(invalid("Invalid connection request")),
            Self::Accept { host, nonce, granted, .. }
                if *host == [0; 32] || *nonce == [0; 32] || !flags_valid(*granted)
                => Err(invalid("Invalid connection acceptance")),
            Self::Offer { sdp, .. } | Self::Answer { sdp, .. }
                if sdp.is_empty() || sdp.len() > MAX_SDP || sdp.contains('\0')
                => Err(invalid("Invalid SDP")),
            Self::Proof { signature, .. } => decode_hex::<64>(signature).map(|_| ()),
            Self::Candidates { items, .. } => {
                if items.len() > MAX_CANDIDATES || items.iter().any(|c|
                    c.mline > 1 || c.candidate.len() > MAX_CANDIDATE
                    || c.candidate.contains(['\r', '\n', '\0']))
                { return Err(invalid("Invalid candidate list")); }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        result.push(DIGITS[(b >> 4) as usize] as char);
        result.push(DIGITS[(b & 15) as usize] as char);
    }
    result
}
pub fn decode_hex<const N: usize>(s: &str) -> io::Result<[u8; N]> {
    if s.len() != N * 2 || !s.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(invalid("Invalid hexadecimal length or digits"));
    }
    let mut result = [0u8; N];
    for (i, pair) in s.as_bytes().chunks_exact(2).enumerate() {
        let digit = |b: u8| if b.is_ascii_digit() { b - b'0' } else { b.to_ascii_lowercase() - b'a' + 10 };
        result[i] = (digit(pair[0]) << 4) | digit(pair[1]);
    }
    Ok(result)
}

fn encode(message: Message) -> io::Result<Vec<u8>> {
    message.validate()?;
    let bytes = serde_json::to_vec(&Envelope { version: VERSION, message })
        .map_err(|_| invalid("Cannot encode signal"))?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME { return Err(invalid("Frame exceeds bounds")); }
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> io::Result<Message> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME { return Err(invalid("Frame exceeds bounds")); }
    let env: Envelope = serde_json::from_slice(bytes).map_err(|_| invalid("Invalid signaling JSON"))?;
    if env.version != VERSION { return Err(invalid("Signaling version mismatch")); }
    env.message.validate()?;
    Ok(env.message)
}
fn frame_length(header: [u8; 4]) -> io::Result<usize> {
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_FRAME { return Err(invalid("Invalid signaling frame length")); }
    Ok(len)
}
fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline.checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Signaling deadline"))
}
fn read_bounded(stream: &mut TcpStream, mut bytes: &mut [u8], deadline: Instant) -> io::Result<()> {
    while !bytes.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(bytes) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Signaling peer closed")),
            Ok(n) => bytes = &mut bytes[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
fn write_bounded(stream: &mut TcpStream, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
    while !bytes.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        match stream.write(bytes) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "Signaling peer closed")),
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Diagnostic use: no public or wildcard address. A private address is not
/// proof of trust. The operator must independently verify the peer key.
pub fn local_address(address: SocketAddr) -> bool {
    match address.ip() {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
        IpAddr::V6(ip) => ip.is_loopback(),
    }
}

pub struct Wire { stream: TcpStream, deadline: Instant }

impl Wire {
    pub fn connect(address: SocketAddr) -> io::Result<Self> {
        if !local_address(address) || address.port() == 0 {
            return Err(invalid("Use a specific private or loopback address"));
        }
        Self::new(TcpStream::connect_timeout(&address, IO_BUDGET)?)
    }
    pub fn accept(address: SocketAddr) -> io::Result<Self> {
        if !local_address(address) || address.port() == 0 {
            return Err(invalid("Use a specific private or loopback bind address"));
        }
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            remaining(deadline)?;
            match listener.accept() {
                Ok((stream, peer)) if local_address(peer) => return Self::new(stream),
                Ok(_) => return Err(invalid("Non-local peer refused")),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => return Err(e),
            }
        }
    }
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        if !local_address(stream.peer_addr()?) || !local_address(stream.local_addr()?) {
            return Err(invalid("Non-local endpoint"));
        }
        stream.set_nodelay(true)?;
        Ok(Self { stream, deadline: Instant::now() + Duration::from_secs(110) })
    }
    pub fn send(&mut self, message: Message) -> io::Result<()> {
        let bytes = encode(message)?;
        let deadline = self.deadline.min(Instant::now() + IO_BUDGET);
        write_bounded(&mut self.stream, &(bytes.len() as u32).to_be_bytes(), deadline)?;
        write_bounded(&mut self.stream, &bytes, deadline)
    }
    pub fn recv(&mut self, expected_session: Option<[u8; 16]>) -> io::Result<Message> {
        let deadline = self.deadline.min(Instant::now() + IO_BUDGET);
        let mut header = [0; 4];
        read_bounded(&mut self.stream, &mut header, deadline)?;
        let mut bytes = vec![0; frame_length(header)?];
        read_bounded(&mut self.stream, &mut bytes, deadline)?;
        let message = decode(&bytes)?;
        if expected_session.is_some_and(|id| message.session() != id) {
            return Err(invalid("Wrong signaling session"));
        }
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: [u8; 16] = [1;16];

    #[test]
    fn frames_roundtrip_and_validate() {
        let message = Message::Proof { session: ID, signature: hex(&[2;64]) };
        assert_eq!(decode(&encode(message.clone()).unwrap()).unwrap(), message);
        assert!(decode(b"{\"version\":99,\"message\":{\"type\":\"finished\",\"session\":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]}}").is_err());
        assert!(encode(Message::Request { session: ID, controller: [3;32], nonce: [4;32], requested: 128 }).is_err());
        assert!(frame_length(((MAX_FRAME + 1) as u32).to_be_bytes()).is_err());
        assert!(decode_hex::<2>("zzzz").is_err());
        assert_eq!(decode_hex::<2>("AbCd").unwrap(), [0xab,0xcd]);
    }

    #[test]
    fn actual_tcp_framing_and_wrong_session() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let host = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut wire = Wire::new(stream).unwrap();
            assert!(matches!(wire.recv(Some(ID)).unwrap(), Message::Finished { .. }));
            wire.send(Message::Finished { session: [2;16] }).unwrap();
        });
        let mut wire = Wire::connect(address).unwrap();
        wire.send(Message::Finished { session: ID }).unwrap();
        assert!(wire.recv(Some(ID)).is_err());
        host.join().unwrap();
    }

    #[test]
    fn blocks_wildcard_and_public_addresses() {
        assert!(local_address("127.0.0.1:12".parse().unwrap()));
        assert!(local_address("192.168.1.2:12".parse().unwrap()));
        assert!(!local_address("0.0.0.0:12".parse().unwrap()));
        assert!(!local_address("8.8.8.8:12".parse().unwrap()));
    }
}
