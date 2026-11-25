#![forbid(unsafe_code)]
//! Local authorization state machine, not a network listener or OS input injector.
use remote_ex_protocol::{decode_input, DecodeError, Input, SessionId};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State { Pending, Approved, Connected, Closed }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions { pub view: bool, pub input: bool }

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidState, InvalidGrant, PermissionDenied, WrongSession, Sequence, Decode(DecodeError),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Releases {
    pub keys: Vec<u16>,
    pub buttons: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action { Inject(Input), Release(Releases), Noop }

/// Callers must serialize transitions and OS injection on a single dispatcher.
/// Every returned release must be applied before tearing down the dispatcher.
/// Actual peer verification and OS permission checks are integration prerequisites.
pub struct SessionGate {
    id: SessionId,
    state: State,
    requested: Permissions,
    grant: Permissions,
    last_sequence: u64,
    keys: BTreeSet<u16>,
    buttons: BTreeSet<u8>,
}

impl SessionGate {
    pub fn new(id: SessionId, requested: Permissions) -> Result<Self, Error> {
        if id == [0; 16] || !requested.view { return Err(Error::InvalidGrant); }
        Ok(Self {
            id, state: State::Pending, requested, grant: Permissions { view: false, input: false },
            last_sequence: 0, keys: BTreeSet::new(), buttons: BTreeSet::new(),
        })
    }
    pub fn state(&self) -> State { self.state }
    /// Must only be called after explicit consent from the local user.
    pub fn approve_local(&mut self, grant: Permissions) -> Result<(), Error> {
        if self.state != State::Pending { return Err(Error::InvalidState); }
        if !grant.view || (grant.input && !self.requested.input) {
            return Err(Error::InvalidGrant);
        }
        self.grant = grant;
        self.state = State::Approved;
        Ok(())
    }
    /// Integration hook for the future authenticated transport. Never call this
    /// on receipt of an untrusted remote "connected" message.
    pub fn on_verified_transport(&mut self) -> Result<(), Error> {
        if self.state != State::Approved { return Err(Error::InvalidState); }
        self.state = State::Connected;
        Ok(())
    }
    pub fn may_view(&self) -> bool { self.state == State::Connected && self.grant.view }
    pub fn handle_input(&mut self, bytes: &[u8]) -> Result<Action, Error> {
        if self.state != State::Connected || !self.grant.input {
            return Err(Error::PermissionDenied);
        }
        let event = decode_input(bytes).map_err(Error::Decode)?;
        if event.session != self.id { return Err(Error::WrongSession); }
        if self.last_sequence.checked_add(1) != Some(event.sequence) {
            return Err(Error::Sequence);
        }
        self.last_sequence = event.sequence;
        match event.input {
            Input::Key { usage, down } => {
                let changed = if down { self.keys.insert(usage) } else { self.keys.remove(&usage) };
                Ok(if changed { Action::Inject(event.input) } else { Action::Noop })
            }
            Input::Button { button, down } => {
                let changed = if down { self.buttons.insert(button) } else { self.buttons.remove(&button) };
                Ok(if changed { Action::Inject(event.input) } else { Action::Noop })
            }
            Input::ReleaseAll => Ok(Action::Release(self.drain())),
            _ => Ok(Action::Inject(event.input)),
        }
    }
    pub fn revoke_input(&mut self) -> Releases {
        self.grant.input = false;
        self.drain()
    }
    pub fn close(&mut self) -> Releases {
        self.state = State::Closed;
        self.grant = Permissions { view: false, input: false };
        self.drain()
    }
    fn drain(&mut self) -> Releases {
        Releases {
            keys: std::mem::take(&mut self.keys).into_iter().collect(),
            buttons: std::mem::take(&mut self.buttons).into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_ex_protocol::{InputEnvelope, Key, VERSION, input_envelope::Event};
    use prost::Message;

    const ID: SessionId = [8; 16];
    fn key(id: SessionId, seq: u64, down: bool) -> Vec<u8> {
        InputEnvelope { version: VERSION, session_id: id.to_vec(), sequence: seq,
            event: Some(Event::Key(Key { usage: 4, down })) }.encode_to_vec()
    }
    fn gate(input: bool) -> SessionGate {
        SessionGate::new(ID, Permissions { view: true, input }).unwrap()
    }
    #[test]
    fn local_consent_and_transport_required() {
        let mut g = gate(true);
        assert_eq!(g.on_verified_transport(), Err(Error::InvalidState));
        assert_eq!(g.handle_input(&key(ID, 1, true)), Err(Error::PermissionDenied));
        g.approve_local(Permissions { view: true, input: true }).unwrap();
        assert!(!g.may_view());
        g.on_verified_transport().unwrap();
        assert!(g.may_view());
        assert_eq!(g.handle_input(&key(ID, 1, true)), Ok(Action::Inject(Input::Key { usage: 4, down: true })));
        assert_eq!(g.handle_input(&key(ID, 1, false)), Err(Error::Sequence));
        assert_eq!(g.handle_input(&key([9;16], 2, false)), Err(Error::WrongSession));
        assert_eq!(g.close().keys, vec![4]);
        assert!(!g.may_view());
        assert_eq!(g.handle_input(&key(ID, 2, false)), Err(Error::PermissionDenied));
    }
    #[test]
    fn no_permission_escalation_and_revoke() {
        let mut g = gate(false);
        assert_eq!(g.approve_local(Permissions { view: true, input: true }), Err(Error::InvalidGrant));
        g.approve_local(Permissions { view: true, input: false }).unwrap();
        g.on_verified_transport().unwrap();
        assert_eq!(g.handle_input(&key(ID, 1, true)), Err(Error::PermissionDenied));
        let mut g = gate(true);
        g.approve_local(Permissions { view: true, input: true }).unwrap();
        g.on_verified_transport().unwrap();
        g.handle_input(&key(ID, 1, true)).unwrap();
        assert_eq!(g.revoke_input().keys, vec![4]);
        assert_eq!(g.handle_input(&key(ID, 2, false)), Err(Error::PermissionDenied));
        assert!(g.close().keys.is_empty());
    }
}
