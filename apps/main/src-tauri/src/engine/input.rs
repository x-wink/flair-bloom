use enigo::{Enigo, Key, Keyboard, Settings};
use std::sync::atomic::{AtomicUsize, Ordering};

pub static SIM_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn simulate_keypress(vk: u32) {
    let Some(key) = vk_to_enigo(vk) else { return };
    SIM_COUNT.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
        let _ = enigo.key(key, enigo::Direction::Click);
    }
    SIM_COUNT.fetch_sub(1, Ordering::SeqCst);
}

pub fn rdev_key_to_vk(key: rdev::Key) -> u32 {
    use rdev::Key::*;
    match key {
        KeyA => 0x41,
        KeyB => 0x42,
        KeyC => 0x43,
        KeyD => 0x44,
        KeyE => 0x45,
        KeyF => 0x46,
        KeyG => 0x47,
        KeyH => 0x48,
        KeyI => 0x49,
        KeyJ => 0x4A,
        KeyK => 0x4B,
        KeyL => 0x4C,
        KeyM => 0x4D,
        KeyN => 0x4E,
        KeyO => 0x4F,
        KeyP => 0x50,
        KeyQ => 0x51,
        KeyR => 0x52,
        KeyS => 0x53,
        KeyT => 0x54,
        KeyU => 0x55,
        KeyV => 0x56,
        KeyW => 0x57,
        KeyX => 0x58,
        KeyY => 0x59,
        KeyZ => 0x5A,
        Num0 => 0x30,
        Num1 => 0x31,
        Num2 => 0x32,
        Num3 => 0x33,
        Num4 => 0x34,
        Num5 => 0x35,
        Num6 => 0x36,
        Num7 => 0x37,
        Num8 => 0x38,
        Num9 => 0x39,
        F1 => 0x70,
        F2 => 0x71,
        F3 => 0x72,
        F4 => 0x73,
        F5 => 0x74,
        F6 => 0x75,
        F7 => 0x76,
        F8 => 0x77,
        F9 => 0x78,
        F10 => 0x79,
        F11 => 0x7A,
        F12 => 0x7B,
        Space => 0x20,
        Return => 0x0D,
        Escape => 0x1B,
        Backspace => 0x08,
        Tab => 0x09,
        ShiftLeft => 0xA0,
        ShiftRight => 0xA1,
        ControlLeft => 0xA2,
        ControlRight => 0xA3,
        Alt => 0x12,
        AltGr => 0x12,
        CapsLock => 0x14,
        UpArrow => 0x26,
        DownArrow => 0x28,
        LeftArrow => 0x25,
        RightArrow => 0x27,
        Insert => 0x2D,
        Delete => 0x2E,
        Home => 0x24,
        End => 0x23,
        PageUp => 0x21,
        PageDown => 0x22,
        _ => 0,
    }
}

fn vk_to_enigo(vk: u32) -> Option<Key> {
    match vk {
        0x41..=0x5A => {
            let c = char::from_u32(vk)?.to_ascii_lowercase();
            Some(Key::Unicode(c))
        }
        0x30..=0x39 => {
            let c = char::from_u32(vk)?;
            Some(Key::Unicode(c))
        }
        0x70 => Some(Key::F1),
        0x71 => Some(Key::F2),
        0x72 => Some(Key::F3),
        0x73 => Some(Key::F4),
        0x74 => Some(Key::F5),
        0x75 => Some(Key::F6),
        0x76 => Some(Key::F7),
        0x77 => Some(Key::F8),
        0x78 => Some(Key::F9),
        0x79 => Some(Key::F10),
        0x7A => Some(Key::F11),
        0x7B => Some(Key::F12),
        0x20 => Some(Key::Space),
        0x0D => Some(Key::Return),
        0x1B => Some(Key::Escape),
        0x08 => Some(Key::Backspace),
        0x09 => Some(Key::Tab),
        0x10 => Some(Key::Shift),
        0x11 => Some(Key::Control),
        0x12 => Some(Key::Alt),
        0x14 => Some(Key::CapsLock),
        0x26 => Some(Key::UpArrow),
        0x28 => Some(Key::DownArrow),
        0x25 => Some(Key::LeftArrow),
        0x27 => Some(Key::RightArrow),
        0x2D => Some(Key::Insert),
        0x2E => Some(Key::Delete),
        0x24 => Some(Key::Home),
        0x23 => Some(Key::End),
        0x21 => Some(Key::PageUp),
        0x22 => Some(Key::PageDown),
        _ => None,
    }
}
