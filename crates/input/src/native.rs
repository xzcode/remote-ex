//! Windows/macOS ordinary desktop input backend using Enigo.
//! Only the primary display is supported until multi-monitor and DPI geometry
//! can be verified. Not connected to any remote session or UI yet.
use crate::{Backend, DisplayArea, InputResult};
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

pub struct NativeBackend { enigo: Enigo }
impl NativeBackend {
    pub fn new() -> InputResult<Self> {
        Ok(Self { enigo: Enigo::new(&Settings::default()).map_err(|e| e.to_string())? })
    }
    pub fn primary_area(&self) -> InputResult<DisplayArea> {
        let (width,height) = self.enigo.main_display().map_err(|e| e.to_string())?;
        if width <= 0 || height <= 0 { return Err("No usable primary display".into()); }
        DisplayArea { x: 0, y: 0, width: width as u32, height: height as u32 }.validate()
    }
}
fn direction(down: bool) -> Direction { if down { Direction::Press } else { Direction::Release } }
fn mouse_button(id: u8) -> InputResult<Button> {
    match id {
        1 => Ok(Button::Left), 2 => Ok(Button::Middle), 3 => Ok(Button::Right),
        4 => Ok(Button::Back), 5 => Ok(Button::Forward),
        _ => Err("Unsupported mouse button".into()),
    }
}
impl Backend for NativeBackend {
    fn check_display(&mut self, area: DisplayArea) -> InputResult<()> {
        if area != self.primary_area()? { return Err("Primary display geometry changed or is unsupported".into()); }
        Ok(())
    }
    fn move_to(&mut self, x: i32, y: i32) -> InputResult<()> {
        self.enigo.move_mouse(x,y,Coordinate::Abs).map_err(|e| e.to_string())
    }
    fn position(&mut self) -> InputResult<(i32,i32)> {
        self.enigo.location().map_err(|e| e.to_string())
    }
    fn key(&mut self, usage: u16, down: bool) -> InputResult<()> {
        let keycode = physical_key(usage).ok_or("Unsupported physical USB HID keyboard usage")?;
        self.enigo.raw(keycode,direction(down)).map_err(|e| e.to_string())
    }
    fn button(&mut self, id: u8, down: bool) -> InputResult<()> {
        self.enigo.button(mouse_button(id)?,direction(down)).map_err(|e| e.to_string())
    }
    fn scroll_steps(&mut self, x: i32, y: i32) -> InputResult<()> {
        if x != 0 { self.enigo.scroll(x,Axis::Horizontal).map_err(|e| e.to_string())?; }
        if y != 0 { self.enigo.scroll(y,Axis::Vertical).map_err(|e| e.to_string())?; }
        Ok(())
    }
}

// USB HID usages, physical key positions, not character/layout translation.
// Unmapped keys fail closed; no substitution with arbitrary key codes.
#[cfg(target_os = "windows")]
fn physical_key(usage: u16) -> Option<u16> {
    use enigo::EXT;
    const LETTERS: [u16;26] = [0x1e,0x30,0x2e,0x20,0x12,0x21,0x22,0x23,0x17,0x24,0x25,0x26,0x32,0x31,0x18,0x19,0x10,0x13,0x1f,0x14,0x16,0x2f,0x11,0x2d,0x15,0x2c];
    Some(match usage {
        4..=29 => LETTERS[(usage-4) as usize],
        30..=38 => usage-28, 39 => 0x0b,
        40 => 0x1c,41 => 0x01,42 => 0x0e,43 => 0x0f,44 => 0x39,
        45 => 0x0c,46 => 0x0d,47 => 0x1a,48 => 0x1b,49 => 0x2b,
        51 => 0x27,52 => 0x28,53 => 0x29,54 => 0x33,55 => 0x34,56 => 0x35,57 => 0x3a,
        58..=67 => usage-58+0x3b,68 => 0x57,69 => 0x58,
        73 => 0x52|EXT,74 => 0x47|EXT,75 => 0x49|EXT,76 => 0x53|EXT,
        77 => 0x4f|EXT,78 => 0x51|EXT,79 => 0x4d|EXT,80 => 0x4b|EXT,81 => 0x50|EXT,82 => 0x48|EXT,
        224 => 0x1d,225 => 0x2a,226 => 0x38,227 => 0x5b|EXT,
        228 => 0x1d|EXT,229 => 0x36,230 => 0x38|EXT,231 => 0x5c|EXT,
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
fn physical_key(usage: u16) -> Option<u16> {
    const LETTERS: [u16;26] = [0,11,8,2,14,3,5,4,34,38,40,37,46,45,31,35,12,15,1,17,32,9,13,7,16,6];
    const DIGITS: [u16;10] = [18,19,20,21,23,22,26,28,25,29];
    const FUNCTIONS: [u16;12] = [122,120,99,118,96,97,98,100,101,109,103,111];
    Some(match usage {
        4..=29 => LETTERS[(usage-4) as usize],
        30..=39 => DIGITS[(usage-30) as usize],
        40 => 36,41 => 53,42 => 51,43 => 48,44 => 49,
        45 => 27,46 => 24,47 => 33,48 => 30,49 => 42,
        51 => 41,52 => 39,53 => 50,54 => 43,55 => 47,56 => 44,57 => 57,
        58..=69 => FUNCTIONS[(usage-58) as usize],
        74 => 115,75 => 116,76 => 117,77 => 119,78 => 121,79 => 124,80 => 123,81 => 125,82 => 126,
        224 => 59,225 => 56,226 => 58,227 => 55,228 => 62,229 => 60,230 => 61,231 => 54,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_mapping_has_explicit_bounds() {
        assert!(physical_key(4).is_some());
        assert!(physical_key(58).is_some());
        assert!(physical_key(231).is_some());
        assert!(physical_key(0).is_none());
        assert!(physical_key(232).is_none());
    }
}
