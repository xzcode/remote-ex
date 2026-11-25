#![forbid(unsafe_code)]
//! Serial, permission-gated input dispatcher. Native backends are not exposed
//! via a listener, CLI, Tauri command or the current view-only session.
use remote_ex_core::{Action, SessionGate};
use remote_ex_protocol::Input;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

#[cfg(all(feature = "native", any(target_os = "windows", target_os = "macos")))]
pub mod native;

pub type InputResult<T> = Result<T, String>;

/// OS-coordinate rectangle for the explicitly authorized display. The future
/// session owner must verify DPI and capture-to-input geometry on the actual OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl DisplayArea {
    pub fn validate(self) -> InputResult<Self> {
        if self.width == 0 || self.height == 0
            || self.width > 32768 || self.height > 32768
            || i64::from(self.x) + i64::from(self.width) - 1 > i64::from(i32::MAX)
            || i64::from(self.y) + i64::from(self.height) - 1 > i64::from(i32::MAX)
        { return Err("Invalid authorized display bounds".into()); }
        Ok(self)
    }
    fn contains(self, x: i32, y: i32) -> bool {
        i64::from(x) >= i64::from(self.x) && i64::from(x) < i64::from(self.x) + i64::from(self.width)
            && i64::from(y) >= i64::from(self.y) && i64::from(y) < i64::from(self.y) + i64::from(self.height)
    }
    pub fn map(self, x: u16, y: u16) -> InputResult<(i32, i32)> {
        self.validate()?;
        let convert = |value: u16, origin: i32, extent: u32| -> i32 {
            (i64::from(origin) + (i64::from(value) * i64::from(extent - 1) + 32767) / 65535) as i32
        };
        Ok((convert(x, self.x, self.width), convert(y, self.y, self.height)))
    }
}

/// No arbitrary shell, text command or privilege elevation. Implementations
/// must use the operating system's ordinary user-session permission mechanisms.
pub trait Backend {
    fn check_display(&mut self, area: DisplayArea) -> InputResult<()>;
    fn move_to(&mut self, x: i32, y: i32) -> InputResult<()>;
    fn position(&mut self) -> InputResult<(i32, i32)>;
    fn key(&mut self, usage: u16, down: bool) -> InputResult<()>;
    fn button(&mut self, button: u8, down: bool) -> InputResult<()>;
    fn scroll_steps(&mut self, x: i32, y: i32) -> InputResult<()>;
}

/// The owner must call this on one worker, only after verifying the connected
/// peer's transport and explicitly granting input separately from view access.
/// Rejecting a message or failing an OS action closes the gate and cleans up
/// actual pressed-state, including possibly partially executed actions.
pub struct Dispatcher<B: Backend> {
    gate: SessionGate,
    backend: B,
    area: DisplayArea,
    keys: BTreeSet<u16>,
    buttons: BTreeSet<u8>,
    pointer_placed: bool,
    scroll_remainder: (i32, i32),
    rate_start: Instant,
    rate_count: u16,
    closed: bool,
}
impl<B: Backend> Dispatcher<B> {
    pub fn new(gate: SessionGate, mut backend: B, area: DisplayArea) -> InputResult<Self> {
        if !gate.may_view() { return Err("Session is not verified and connected".into()); }
        let area = area.validate()?;
        backend.check_display(area)?;
        Ok(Self { gate, backend, area, keys: BTreeSet::new(), buttons: BTreeSet::new(),
            pointer_placed: false, scroll_remainder: (0,0), rate_start: Instant::now(), rate_count: 0, closed: false })
    }
    pub fn dispatch(&mut self, packet: &[u8]) -> InputResult<()> {
        if self.closed { return Err("Input dispatcher closed".into()); }
        let result = self.dispatch_inner(packet);
        if let Err(reason) = result {
            let cleanup = self.close();
            return Err(match cleanup {
                Ok(()) => reason,
                Err(e) => format!("{reason}; input release failed: {e}"),
            });
        }
        Ok(())
    }
    fn dispatch_inner(&mut self, packet: &[u8]) -> InputResult<()> {
        if self.rate_start.elapsed() >= Duration::from_secs(1) {
            self.rate_start = Instant::now(); self.rate_count = 0;
        }
        if self.rate_count >= 300 { return Err("Input rate limit exceeded".into()); }
        self.rate_count += 1;
        let action = self.gate.handle_input(packet).map_err(|e| format!("Input rejected: {e:?}"))?;
        match action {
            Action::Noop => Ok(()),
            Action::Release(_) => self.release_held(),
            Action::Inject(input) => {
                self.backend.check_display(self.area)?;
                match input {
                    Input::Move { x, y } => {
                        let (x, y) = self.area.map(x, y)?;
                        self.backend.move_to(x, y)?;
                        self.pointer_placed = true;
                        Ok(())
                    }
                    Input::Key { usage, down } => {
                        // Track before press: an OS error can be partial.
                        if down { self.keys.insert(usage); }
                        self.backend.key(usage, down)?;
                        if !down { self.keys.remove(&usage); }
                        Ok(())
                    }
                    Input::Button { button, down } => {
                        if down {
                            self.require_pointer()?;
                            self.buttons.insert(button);
                        }
                        self.backend.button(button, down)?;
                        if !down { self.buttons.remove(&button); }
                        Ok(())
                    }
                    Input::Scroll { x, y } => {
                        self.require_pointer()?;
                        // Protocol v1: 120 units = one wheel notch; positive
                        // values scroll right/down. Keep high-resolution remainder.
                        let sx = self.scroll_remainder.0 + x;
                        let sy = self.scroll_remainder.1 + y;
                        self.scroll_remainder = (sx % 120, sy % 120);
                        if sx / 120 == 0 && sy / 120 == 0 { return Ok(()); }
                        self.backend.scroll_steps(sx / 120, sy / 120)
                    }
                    Input::ReleaseAll => self.release_held(),
                }
            }
        }
    }
    fn require_pointer(&mut self) -> InputResult<()> {
        let (x, y) = self.backend.position()?;
        if !self.pointer_placed || !self.area.contains(x, y) {
            return Err("Pointer outside authorized display or not positioned".into());
        }
        Ok(())
    }
    fn release_held(&mut self) -> InputResult<()> {
        let mut failure = false;
        // Releases must not depend on current pointer position/display bounds.
        for key in self.keys.clone() {
            if self.backend.key(key, false).is_ok() { self.keys.remove(&key); } else { failure = true; }
        }
        for button in self.buttons.clone() {
            if self.backend.button(button, false).is_ok() { self.buttons.remove(&button); } else { failure = true; }
        }
        self.pointer_placed = false;
        self.scroll_remainder = (0,0);
        if failure { Err("Could not release every held input; retry cleanup".into()) } else { Ok(()) }
    }
    pub fn close(&mut self) -> InputResult<()> {
        self.closed = true;
        self.gate.close();
        self.release_held()
    }
}
impl<B: Backend> Drop for Dispatcher<B> {
    fn drop(&mut self) { let _ = self.close(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use remote_ex_core::Permissions;
    use remote_ex_protocol::{InputEnvelope, VERSION, Key, PointerMove, PointerButton, input_envelope::Event};
    use remote_ex_security::{DeviceIdentity, Role, pairing::PendingPairing, session::{VIEW, INPUT}};
    use std::{cell::RefCell, rc::Rc};
    const ID: [u8;16] = [7;16];
    const AREA: DisplayArea = DisplayArea { x: 0, y: 0, width: 1920, height: 1080 };
    fn gate(input: bool) -> SessionGate {
        let a = DeviceIdentity::generate();
        let b = DeviceIdentity::generate();
        let bits = VIEW | if input { INPUT } else { 0 };
        let pa = PendingPairing::new(Role::Controller, ID, &a, b.public_key(), bits).unwrap();
        let pb = PendingPairing::new(Role::Host, ID, &b, a.public_key(), bits).unwrap();
        let an = pa.nonce(); let bn = pb.nonce();
        let sa = pa.approve_local(bits).unwrap().sign(&a, bn, b"offer", b"answer").unwrap();
        let sb = pb.approve_local(bits).unwrap().sign(&b, an, b"offer", b"answer").unwrap();
        let proof = sa.finish(&sb.signature(), b"offer", b"answer").unwrap();
        let p = Permissions { view: true, input };
        let mut gate = SessionGate::new(ID, p).unwrap();
        gate.approve_local(p).unwrap(); gate.on_verified_transport(&proof).unwrap(); gate
    }
    fn packet(sequence: u64, event: Event) -> Vec<u8> {
        InputEnvelope { version: VERSION, session_id: ID.to_vec(), sequence, event: Some(event) }.encode_to_vec()
    }
    #[derive(Default)]
    struct Fake { log: Rc<RefCell<Vec<String>>>, position: (i32,i32), fail_press: bool }
    impl Backend for Fake {
        fn check_display(&mut self, _: DisplayArea) -> InputResult<()> { Ok(()) }
        fn move_to(&mut self, x: i32, y: i32) -> InputResult<()> { self.position = (x,y); Ok(()) }
        fn position(&mut self) -> InputResult<(i32,i32)> { Ok(self.position) }
        fn key(&mut self, usage: u16, down: bool) -> InputResult<()> {
            self.log.borrow_mut().push(format!("key:{usage}:{down}"));
            if down && self.fail_press { Err("partial OS failure".into()) } else { Ok(()) }
        }
        fn button(&mut self, button: u8, down: bool) -> InputResult<()> {
            self.log.borrow_mut().push(format!("button:{button}:{down}")); Ok(())
        }
        fn scroll_steps(&mut self, _: i32, _: i32) -> InputResult<()> { Ok(()) }
    }
    #[test]
    fn coordinates_are_inclusive_and_safe() {
        assert_eq!(AREA.map(0,0).unwrap(), (0,0));
        assert_eq!(AREA.map(65535,65535).unwrap(), (1919,1079));
        assert!(DisplayArea { width: 0, ..AREA }.validate().is_err());
        assert!(DisplayArea { x: i32::MAX, ..AREA }.validate().is_err());
    }
    #[test]
    fn view_only_and_wrong_sequence_never_inject() {
        let backend = Fake::default(); let log = backend.log.clone();
        let mut d = Dispatcher::new(gate(false), backend, AREA).unwrap();
        assert!(d.dispatch(&packet(1, Event::Key(Key { usage: 4, down: true }))).is_err());
        assert!(log.borrow().is_empty());
        let backend = Fake::default(); let log = backend.log.clone();
        let mut d = Dispatcher::new(gate(true), backend, AREA).unwrap();
        assert!(d.dispatch(&packet(2, Event::Key(Key { usage: 4, down: true }))).is_err());
        assert!(log.borrow().is_empty());
    }
    #[test]
    fn close_and_partial_failure_release_actual_held_inputs() {
        let backend = Fake::default(); let log = backend.log.clone();
        let mut d = Dispatcher::new(gate(true), backend, AREA).unwrap();
        d.dispatch(&packet(1, Event::PointerMove(PointerMove { x: 10, y: 20 }))).unwrap();
        d.dispatch(&packet(2, Event::PointerButton(PointerButton { button: 1, down: true }))).unwrap();
        d.dispatch(&packet(3, Event::Key(Key { usage: 4, down: true }))).unwrap();
        d.close().unwrap();
        assert_eq!(log.borrow().as_slice(), ["button:1:true", "key:4:true", "key:4:false", "button:1:false"]);
        let backend = Fake { fail_press: true, ..Fake::default() }; let log = backend.log.clone();
        let mut d = Dispatcher::new(gate(true), backend, AREA).unwrap();
        assert!(d.dispatch(&packet(1, Event::Key(Key { usage: 4, down: true }))).is_err());
        assert_eq!(log.borrow().as_slice(), ["key:4:true", "key:4:false"]);
    }
}
