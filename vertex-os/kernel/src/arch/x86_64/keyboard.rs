//! PS/2 keyboard driver.
//!
//! Scancode set 1 is translated to ASCII and pushed into a small ring buffer.
//! The interrupt handler never prints: the shell drains the buffer from normal
//! context, which keeps the console lock out of interrupt paths.

use core::sync::atomic::{AtomicUsize, Ordering};

use super::cpu::inb;
use super::pic;
use crate::sync::SyncCell;

const KEYBOARD_DATA_PORT: u16 = 0x60;
const RX_RING: usize = 256;

struct KeyboardState {
    shift: bool,
    caps: bool,
    ctrl: bool,
    alt: bool,
    extended: bool,
}

static STATE: SyncCell<KeyboardState> = SyncCell::new(KeyboardState {
    shift: false,
    caps: false,
    ctrl: false,
    alt: false,
    extended: false,
});

/// A key press translated from PS/2 scancode set 1.
///
/// The graphical front end needs navigation keys that have no ASCII byte, so
/// the ring buffer stores events instead of characters. [`pop_char`] keeps the
/// byte-only view used by the text shell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Char(u8),
    Enter,
    Escape,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Delete,
    Home,
    End,
    F1,
    F2,
    F3,
}

static RX: SyncCell<[Key; RX_RING]> = SyncCell::new([Key::Char(0); RX_RING]);
static RX_HEAD: AtomicUsize = AtomicUsize::new(0);
static RX_TAIL: AtomicUsize = AtomicUsize::new(0);

/// Scancode Set 1 to ASCII mapping (US QWERTY layout)
/// Index is the scancode, value is the ASCII character (0 = no mapping)
const SCANCODE_TO_ASCII: [u8; 128] = [
    0,    // 0x00 - Error
    0,    // 0x01 - Escape
    b'1', // 0x02
    b'2', // 0x03
    b'3', // 0x04
    b'4', // 0x05
    b'5', // 0x06
    b'6', // 0x07
    b'7', // 0x08
    b'8', // 0x09
    b'9', // 0x0A
    b'0', // 0x0B
    b'-', // 0x0C
    b'=', // 0x0D
    0x08, // 0x0E - Backspace
    b'\t',// 0x0F - Tab
    b'q', // 0x10
    b'w', // 0x11
    b'e', // 0x12
    b'r', // 0x13
    b't', // 0x14
    b'y', // 0x15
    b'u', // 0x16
    b'i', // 0x17
    b'o', // 0x18
    b'p', // 0x19
    b'[', // 0x1A
    b']', // 0x1B
    b'\n',// 0x1C - Enter
    0,    // 0x1D - Left Ctrl
    b'a', // 0x1E
    b's', // 0x1F
    b'd', // 0x20
    b'f', // 0x21
    b'g', // 0x22
    b'h', // 0x23
    b'j', // 0x24
    b'k', // 0x25
    b'l', // 0x26
    b';', // 0x27
    b'\'',// 0x28
    b'`', // 0x29
    0,    // 0x2A - Left Shift
    b'\\',// 0x2B
    b'z', // 0x2C
    b'x', // 0x2D
    b'c', // 0x2E
    b'v', // 0x2F
    b'b', // 0x30
    b'n', // 0x31
    b'm', // 0x32
    b',', // 0x33
    b'.', // 0x34
    b'/', // 0x35
    0,    // 0x36 - Right Shift
    b'*', // 0x37 - Keypad *
    0,    // 0x38 - Left Alt
    b' ', // 0x39 - Space
    0,    // 0x3A - Caps Lock
    0,    // 0x3B - F1
    0,    // 0x3C - F2
    0,    // 0x3D - F3
    0,    // 0x3E - F4
    0,    // 0x3F - F5
    0,    // 0x40 - F6
    0,    // 0x41 - F7
    0,    // 0x42 - F8
    0,    // 0x43 - F9
    0,    // 0x44 - F10
    0,    // 0x45 - Num Lock
    0,    // 0x46 - Scroll Lock
    b'7', // 0x47 - Keypad 7
    b'8', // 0x48 - Keypad 8
    b'9', // 0x49 - Keypad 9
    b'-', // 0x4A - Keypad -
    b'4', // 0x4B - Keypad 4
    b'5', // 0x4C - Keypad 5
    b'6', // 0x4D - Keypad 6
    b'+', // 0x4E - Keypad +
    b'1', // 0x4F - Keypad 1
    b'2', // 0x50 - Keypad 2
    b'3', // 0x51 - Keypad 3
    b'0', // 0x52 - Keypad 0
    b'.', // 0x53 - Keypad .
    0, 0, 0, // 0x54-0x56
    0,    // 0x57 - F11
    0,    // 0x58 - F12
    0, 0, 0, 0, 0, 0, 0, // 0x59-0x5F
    0, 0, 0, 0, 0, 0, 0, 0, // 0x60-0x67
    0, 0, 0, 0, 0, 0, 0, 0, // 0x68-0x6F
    0, 0, 0, 0, 0, 0, 0, 0, // 0x70-0x77
    0, 0, 0, 0, 0, 0, 0, 0, // 0x78-0x7F
];

/// Shifted scancode to ASCII mapping
const SCANCODE_TO_ASCII_SHIFT: [u8; 128] = [
    0,    // 0x00 - Error
    0,    // 0x01 - Escape
    b'!', // 0x02
    b'@', // 0x03
    b'#', // 0x04
    b'$', // 0x05
    b'%', // 0x06
    b'^', // 0x07
    b'&', // 0x08
    b'*', // 0x09
    b'(', // 0x0A
    b')', // 0x0B
    b'_', // 0x0C
    b'+', // 0x0D
    0x08, // 0x0E - Backspace
    b'\t',// 0x0F - Tab
    b'Q', // 0x10
    b'W', // 0x11
    b'E', // 0x12
    b'R', // 0x13
    b'T', // 0x14
    b'Y', // 0x15
    b'U', // 0x16
    b'I', // 0x17
    b'O', // 0x18
    b'P', // 0x19
    b'{', // 0x1A
    b'}', // 0x1B
    b'\n',// 0x1C - Enter
    0,    // 0x1D - Left Ctrl
    b'A', // 0x1E
    b'S', // 0x1F
    b'D', // 0x20
    b'F', // 0x21
    b'G', // 0x22
    b'H', // 0x23
    b'J', // 0x24
    b'K', // 0x25
    b'L', // 0x26
    b':', // 0x27
    b'"', // 0x28
    b'~', // 0x29
    0,    // 0x2A - Left Shift
    b'|', // 0x2B
    b'Z', // 0x2C
    b'X', // 0x2D
    b'C', // 0x2E
    b'V', // 0x2F
    b'B', // 0x30
    b'N', // 0x31
    b'M', // 0x32
    b'<', // 0x33
    b'>', // 0x34
    b'?', // 0x35
    0,    // 0x36 - Right Shift
    b'*', // 0x37 - Keypad *
    0,    // 0x38 - Left Alt
    b' ', // 0x39 - Space
    0,    // 0x3A - Caps Lock
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x3B-0x44 - Function keys
    0, 0, // 0x45-0x46 - Num Lock, Scroll Lock
    b'7', b'8', b'9', b'-', // 0x47-0x4A - Keypad
    b'4', b'5', b'6', b'+', // 0x4B-0x4E - Keypad
    b'1', b'2', b'3',       // 0x4F-0x51 - Keypad
    b'0', b'.',             // 0x52-0x53 - Keypad
    0, 0, 0, // 0x54-0x56
    0,    // 0x57 - F11
    0,    // 0x58 - F12
    0, 0, 0, 0, 0, 0, 0, // 0x59-0x5F
    0, 0, 0, 0, 0, 0, 0, 0, // 0x60-0x67
    0, 0, 0, 0, 0, 0, 0, 0, // 0x68-0x6F
    0, 0, 0, 0, 0, 0, 0, 0, // 0x70-0x77
    0, 0, 0, 0, 0, 0, 0, 0, // 0x78-0x7F
];

/// Scancode constants for modifier keys
const SCANCODE_LEFT_SHIFT: u8 = 0x2A;
const SCANCODE_RIGHT_SHIFT: u8 = 0x36;
const SCANCODE_LEFT_CTRL: u8 = 0x1D;
const SCANCODE_LEFT_ALT: u8 = 0x38;
const SCANCODE_CAPS_LOCK: u8 = 0x3A;

/// Unmask IRQ1.
pub fn init() {
    pic::enable_irq(pic::IRQ_KEYBOARD);
}

/// Read the raw scancode byte from the controller.
fn read_scancode() -> u8 {
    unsafe { inb(KEYBOARD_DATA_PORT) }
}

/// Translate a scancode into a key event, tracking modifiers.
pub fn translate(scancode: u8) -> Option<Key> {
    let state = unsafe { STATE.get() };
    let released = scancode & 0x80 != 0;
    let code = scancode & 0x7F;

    if scancode == 0xE0 {
        state.extended = true;
        return None;
    }

    // Extended keys arrive as `0xE0` followed by a make or break code, and the
    // prefix is repeated for the release, so the flag never outlives one byte.
    if state.extended {
        state.extended = false;
        if released {
            return None;
        }
        return match code {
            0x48 => Some(Key::Up),
            0x50 => Some(Key::Down),
            0x4B => Some(Key::Left),
            0x4D => Some(Key::Right),
            0x53 => Some(Key::Delete),
            0x47 => Some(Key::Home),
            0x4F => Some(Key::End),
            _ => None,
        };
    }

    match code {
        SCANCODE_LEFT_SHIFT | SCANCODE_RIGHT_SHIFT => {
            state.shift = !released;
            return None;
        }
        SCANCODE_LEFT_CTRL => {
            state.ctrl = !released;
            return None;
        }
        SCANCODE_LEFT_ALT => {
            state.alt = !released;
            return None;
        }
        SCANCODE_CAPS_LOCK if !released => {
            state.caps = !state.caps;
            return None;
        }
        _ => {}
    }

    if released {
        return None;
    }

    // Non-character keys: scancode set 1 make codes.
    match code {
        0x01 => return Some(Key::Escape),
        0x0E => return Some(Key::Backspace),
        0x0F => return Some(Key::Tab),
        0x1C => return Some(Key::Enter),
        0x3B => return Some(Key::F1),
        0x3C => return Some(Key::F2),
        0x3D => return Some(Key::F3),
        _ => {}
    }

    let ascii = if state.shift {
        SCANCODE_TO_ASCII_SHIFT[code as usize]
    } else {
        SCANCODE_TO_ASCII[code as usize]
    };
    if ascii == 0 {
        return None;
    }

    let ascii = if state.caps && ascii.is_ascii_alphabetic() {
        if state.shift {
            ascii.to_ascii_lowercase()
        } else {
            ascii.to_ascii_uppercase()
        }
    } else {
        ascii
    };

    Some(Key::Char(ascii))
}

fn push_key(key: Key) {
    let head = RX_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % RX_RING;
    if next == RX_TAIL.load(Ordering::Relaxed) {
        return; // buffer full: drop the keystroke
    }
    unsafe {
        RX.get()[head] = key;
    }
    RX_HEAD.store(next, Ordering::Release);
}

/// IRQ1 handler.
pub fn handle_interrupt() {
    let scancode = read_scancode();
    if let Some(key) = translate(scancode) {
        push_key(key);
    }
    pic::send_eoi(pic::IRQ_KEYBOARD);
}

/// Pop the oldest key event, if any.
pub fn pop_key() -> Option<Key> {
    let head = RX_HEAD.load(Ordering::Acquire);
    let tail = RX_TAIL.load(Ordering::Relaxed);
    if head == tail {
        return None;
    }
    let key = unsafe { RX.get()[tail] };
    RX_TAIL.store((tail + 1) % RX_RING, Ordering::Release);
    Some(key)
}

/// Pop a character for line-oriented consumers.
///
/// Navigation and function keys are dropped here; the text shell treats Enter,
/// Backspace and Tab as the control bytes it already understands.
pub fn pop_char() -> Option<u8> {
    loop {
        match pop_key()? {
            Key::Char(c) => return Some(c),
            Key::Enter => return Some(b'\r'),
            Key::Backspace => return Some(0x08),
            Key::Tab => return Some(b'\t'),
            _ => continue,
        }
    }
}
