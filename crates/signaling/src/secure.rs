//! Authenticated encryption for an explicitly authorized one-shot LAN session.
//! The Noise KK handshake requires both peers' independently verified X25519
//! public keys. No TOFU, passwords, downgrade, or anonymous transport.
//! This module does not grant permission to capture or control a desktop.
use super::{decode, encode, invalid, read_bounded, write_bounded, Message, Wire};
use snow::{params::NoiseParams, Builder, TransportState};
use std::{io, time::{Duration, Instant}};
use zeroize::Zeroizing;

const PATTERN: &str = "Noise_KK_25519_ChaChaPoly_BLAKE2s";
const PROLOGUE: &[u8] = b"Remote EX/LAN/view/v1";
const MAX_NOISE: usize = 65535;
const MAX_PLAINTEXT: usize = MAX_NOISE - 16;
const DEADLINE: Duration = Duration::from_secs(5);

pub struct NoiseIdentity {
    secret: Zeroizing<Vec<u8>>,
    public: [u8; 32],
}
fn params() -> io::Result<NoiseParams> {
    PATTERN.parse().map_err(|_| invalid("Invalid Noise protocol"))
}
fn crypto<T>(result: Result<T, snow::Error>) -> io::Result<T> {
    result.map_err(|_| invalid("Encrypted signaling authentication or framing failed"))
}
impl NoiseIdentity {
    pub fn generate() -> io::Result<Self> {
        let keys = crypto(Builder::new(params()?).generate_keypair())?;
        let secret = Zeroizing::new(keys.private);
        let public = keys.public.try_into().map_err(|_| invalid("Invalid Noise public key"))?;
        Ok(Self { secret, public })
    }
    pub fn public_key(&self) -> [u8; 32] { self.public }
}

pub struct SecureWire {
    wire: Wire,
    state: TransportState,
    failed: bool,
}
fn read_packet(wire: &mut Wire) -> io::Result<Vec<u8>> {
    let deadline = wire.deadline.min(Instant::now() + DEADLINE);
    let mut len = [0; 2];
    read_bounded(&mut wire.stream, &mut len, deadline)?;
    let len = usize::from(u16::from_be_bytes(len));
    if len < 16 { return Err(invalid("Invalid Noise record length")); }
    let mut bytes = vec![0; len];
    read_bounded(&mut wire.stream, &mut bytes, deadline)?;
    Ok(bytes)
}
fn write_packet(wire: &mut Wire, packet: &[u8]) -> io::Result<()> {
    if !(16..=MAX_NOISE).contains(&packet.len()) { return Err(invalid("Invalid Noise record length")); }
    let deadline = wire.deadline.min(Instant::now() + DEADLINE);
    write_bounded(&mut wire.stream, &(packet.len() as u16).to_be_bytes(), deadline)?;
    write_bounded(&mut wire.stream, packet, deadline)
}

impl SecureWire {
    pub fn handshake(
        mut wire: Wire, identity: NoiseIdentity, expected_peer: [u8; 32], initiator: bool,
    ) -> io::Result<Self> {
        if expected_peer == [0;32] || expected_peer == identity.public {
            return Err(invalid("Invalid expected Noise peer"));
        }
        let builder = crypto(Builder::new(params()?).local_private_key(&identity.secret))?;
        let builder = crypto(builder.remote_public_key(&expected_peer))?;
        let builder = crypto(builder.prologue(PROLOGUE))?;
        let mut hs = if initiator { crypto(builder.build_initiator())? }
            else { crypto(builder.build_responder())? };
        let mut outgoing = vec![0; MAX_NOISE];
        let mut plaintext = Zeroizing::new(vec![0; MAX_NOISE]);
        if initiator {
            let len = crypto(hs.write_message(&[], &mut outgoing))?;
            write_packet(&mut wire, &outgoing[..len])?;
            let incoming = read_packet(&mut wire)?;
            if crypto(hs.read_message(&incoming, &mut plaintext))? != 0 {
                return Err(invalid("Unexpected handshake payload"));
            }
        } else {
            let incoming = read_packet(&mut wire)?;
            if crypto(hs.read_message(&incoming, &mut plaintext))? != 0 {
                return Err(invalid("Unexpected handshake payload"));
            }
            let len = crypto(hs.write_message(&[], &mut outgoing))?;
            write_packet(&mut wire, &outgoing[..len])?;
        }
        if !hs.is_handshake_finished() || hs.get_remote_static() != Some(expected_peer.as_slice()) {
            return Err(invalid("Unexpected Noise peer identity"));
        }
        Ok(Self { wire, state: crypto(hs.into_transport_mode())?, failed: false })
    }

    /// A failed read/write permanently poisons the channel. Do not retry a
    /// partial frame or reuse a cipher nonce after a transport failure.
    pub fn send(&mut self, message: Message) -> io::Result<()> {
        if self.failed { return Err(invalid("Encrypted signaling closed")); }
        let result = (|| {
            let bytes = Zeroizing::new(encode(message)?);
            if bytes.len() > MAX_PLAINTEXT { return Err(invalid("Encrypted signal exceeds record limit")); }
            let mut encrypted = vec![0; MAX_NOISE];
            let size = crypto(self.state.write_message(&bytes, &mut encrypted))?;
            write_packet(&mut self.wire, &encrypted[..size])
        })();
        if result.is_err() { self.failed = true; }
        result
    }
    pub fn recv(&mut self, expected_session: Option<[u8;16]>) -> io::Result<Message> {
        if self.failed { return Err(invalid("Encrypted signaling closed")); }
        let result = (|| {
            let packet = read_packet(&mut self.wire)?;
            let mut plaintext = Zeroizing::new(vec![0; MAX_NOISE]);
            let len = crypto(self.state.read_message(&packet, &mut plaintext))?;
            let message = decode(&plaintext[..len])?;
            if expected_session.is_some_and(|id| id != message.session()) {
                return Err(invalid("Wrong encrypted session"));
            }
            Ok(message)
        })();
        if result.is_err() { self.failed = true; }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn mutually_pinned_encrypted_roundtrip() {
        let a = NoiseIdentity::generate().unwrap();
        let b = NoiseIdentity::generate().unwrap();
        let ak = a.public_key();
        let bk = b.public_key();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let id = [1;16];
        let host = std::thread::spawn(move || {
            let wire = Wire::accept_listener(listener).unwrap();
            let mut secure = SecureWire::handshake(wire, b, ak, false).unwrap();
            assert_eq!(secure.recv(Some(id)).unwrap(), Message::Ready { session: id });
            secure.send(Message::Heartbeat { session: id, sequence: 1 }).unwrap();
        });
        let mut secure = SecureWire::handshake(Wire::connect(address).unwrap(), a, bk, true).unwrap();
        secure.send(Message::Ready { session: id }).unwrap();
        assert_eq!(secure.recv(Some(id)).unwrap(), Message::Heartbeat { session: id, sequence: 1 });
        host.join().unwrap();
    }

    #[test]
    fn wrong_peer_cannot_authenticate() {
        let a = NoiseIdentity::generate().unwrap();
        let b = NoiseIdentity::generate().unwrap();
        let wrong = NoiseIdentity::generate().unwrap().public_key();
        let ak = a.public_key();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let host = std::thread::spawn(move || {
            let wire = Wire::accept_listener(listener).unwrap();
            assert!(SecureWire::handshake(wire, b, ak, false).is_err());
        });
        assert!(SecureWire::handshake(Wire::connect(address).unwrap(), a, wrong, true).is_err());
        host.join().unwrap();
    }
}
