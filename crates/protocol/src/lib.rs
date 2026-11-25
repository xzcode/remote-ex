#![forbid(unsafe_code)]
//! Bounded protobuf input messages. The transport must authenticate the peer
//! and channel before passing bytes to this module.
use prost::Message;

pub const VERSION: u32 = 1;
pub const MAX_INPUT_BYTES: usize = 1024;
pub type SessionId = [u8; 16];

#[derive(Clone, PartialEq, Message)]
pub struct InputEnvelope {
    #[prost(uint32, tag = "1")]
    pub version: u32,
    #[prost(bytes = "vec", tag = "2")]
    pub session_id: Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub sequence: u64,
    #[prost(oneof = "input_envelope::Event", tags = "10, 11, 12, 13, 14")]
    pub event: Option<input_envelope::Event>,
}

pub mod input_envelope {
    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Event {
        #[prost(message, tag = "10")]
        PointerMove(super::PointerMove),
        #[prost(message, tag = "11")]
        PointerButton(super::PointerButton),
        #[prost(message, tag = "12")]
        Key(super::Key),
        #[prost(message, tag = "13")]
        Scroll(super::Scroll),
        #[prost(message, tag = "14")]
        ReleaseAll(super::ReleaseAll),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct PointerMove {
    #[prost(uint32, tag = "1")]
    pub x: u32,
    #[prost(uint32, tag = "2")]
    pub y: u32,
}
#[derive(Clone, PartialEq, Message)]
pub struct PointerButton {
    #[prost(uint32, tag = "1")]
    pub button: u32,
    #[prost(bool, tag = "2")]
    pub down: bool,
}
#[derive(Clone, PartialEq, Message)]
pub struct Key {
    #[prost(uint32, tag = "1")]
    pub usage: u32,
    #[prost(bool, tag = "2")]
    pub down: bool,
}
#[derive(Clone, PartialEq, Message)]
pub struct Scroll {
    #[prost(sint32, tag = "1")]
    pub x: i32,
    #[prost(sint32, tag = "2")]
    pub y: i32,
}
#[derive(Clone, PartialEq, Message)]
pub struct ReleaseAll {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Move { x: u16, y: u16 },
    Button { button: u8, down: bool },
    Key { usage: u16, down: bool },
    Scroll { x: i32, y: i32 },
    ReleaseAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedInput {
    pub session: SessionId,
    pub sequence: u64,
    pub input: Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    TooLarge, Malformed, Version, Session, Sequence, InvalidEvent,
}

pub fn decode_input(bytes: &[u8]) -> Result<ValidatedInput, DecodeError> {
    if bytes.len() > MAX_INPUT_BYTES { return Err(DecodeError::TooLarge); }
    let msg = InputEnvelope::decode(bytes).map_err(|_| DecodeError::Malformed)?;
    if msg.version != VERSION { return Err(DecodeError::Version); }
    let session: SessionId = msg.session_id.as_slice().try_into().map_err(|_| DecodeError::Session)?;
    if session == [0; 16] { return Err(DecodeError::Session); }
    if msg.sequence == 0 { return Err(DecodeError::Sequence); }
    use input_envelope::Event;
    let input = match msg.event.ok_or(DecodeError::InvalidEvent)? {
        Event::PointerMove(p) => Input::Move {
            x: u16::try_from(p.x).map_err(|_| DecodeError::InvalidEvent)?,
            y: u16::try_from(p.y).map_err(|_| DecodeError::InvalidEvent)?,
        },
        Event::PointerButton(p) if (1..=5).contains(&p.button) => Input::Button {
            button: p.button as u8, down: p.down,
        },
        Event::Key(k) if (4..=231).contains(&k.usage) => Input::Key {
            usage: k.usage as u16, down: k.down,
        },
        Event::Scroll(s) if (-1200..=1200).contains(&s.x) && (-1200..=1200).contains(&s.y) =>
            Input::Scroll { x: s.x, y: s.y },
        Event::ReleaseAll(_) => Input::ReleaseAll,
        _ => return Err(DecodeError::InvalidEvent),
    };
    Ok(ValidatedInput { session, sequence: msg.sequence, input })
}

#[cfg(test)]
mod tests {
    use super::*;
    use input_envelope::Event;
    fn msg(event: Event) -> InputEnvelope {
        InputEnvelope { version: VERSION, session_id: vec![7; 16], sequence: 1, event: Some(event) }
    }
    #[test]
    fn valid_move() {
        let bytes = msg(Event::PointerMove(PointerMove { x: 65535, y: 0 })).encode_to_vec();
        assert_eq!(decode_input(&bytes).unwrap().input, Input::Move { x: 65535, y: 0 });
    }
    #[test]
    fn invalid_bounds_and_identity() {
        assert_eq!(decode_input(&vec![0; MAX_INPUT_BYTES + 1]), Err(DecodeError::TooLarge));
        let mut m = msg(Event::PointerMove(PointerMove { x: 65536, y: 0 }));
        assert_eq!(decode_input(&m.encode_to_vec()), Err(DecodeError::InvalidEvent));
        m.session_id = vec![0; 16];
        assert_eq!(decode_input(&m.encode_to_vec()), Err(DecodeError::Session));
        m.version = 2;
        assert_eq!(decode_input(&m.encode_to_vec()), Err(DecodeError::Version));
    }
    #[test]
    fn rejects_missing_event_and_sequence() {
        let mut m = msg(Event::ReleaseAll(ReleaseAll {}));
        m.sequence = 0;
        assert_eq!(decode_input(&m.encode_to_vec()), Err(DecodeError::Sequence));
        m.sequence = 1;
        m.event = None;
        assert_eq!(decode_input(&m.encode_to_vec()), Err(DecodeError::InvalidEvent));
    }
}
