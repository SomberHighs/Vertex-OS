//! PS/2 Keyboard driver
//!
//! Handles keyboard scancodes from port 0x60 and translates them to ASCII

use super::cpu::inb;
use super::pic;
use crate::kprint;

/// PS/2 keyboard data port
const KEYBOARD_DATA_PORT: u16 = 0x60;

/// Keyboard state
static mut SHIFT_PRESSED: bool = false;
static mut CAPS_LOCK: bool = false;
static mut CTRL_PRESSED: bool = false;
static mut ALT_PRESSED: bool = false;

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

/// Read a scancode from the keyboard
pub fn read_scancode() -> u8 {
    unsafe { inb(KEYBOARD_DATA_PORT) }
}

/// Process a scancode and return the ASCII character if applicable
pub fn process_scancode(scancode: u8) -> Option<char> {
    let released = scancode & 0x80 != 0;
    let scancode = scancode & 0x7F;

    unsafe {
        match scancode {
            SCANCODE_LEFT_SHIFT | SCANCODE_RIGHT_SHIFT => {
                SHIFT_PRESSED = !released;
                return None;
            }
            SCANCODE_LEFT_CTRL => {
                CTRL_PRESSED = !released;
                return None;
            }
            SCANCODE_LEFT_ALT => {
                ALT_PRESSED = !released;
                return None;
            }
            SCANCODE_CAPS_LOCK if !released => {
                CAPS_LOCK = !CAPS_LOCK;
                return None;
            }
            _ => {}
        }

        // Only process key press, not release
        if released {
            return None;
        }

        let ascii = if SHIFT_PRESSED {
            SCANCODE_TO_ASCII_SHIFT[scancode as usize]
        } else {
            SCANCODE_TO_ASCII[scancode as usize]
        };

        if ascii == 0 {
            return None;
        }

        // Apply caps lock to letters
        let ascii = if CAPS_LOCK && ascii.is_ascii_alphabetic() {
            if SHIFT_PRESSED {
                ascii.to_ascii_lowercase()
            } else {
                ascii.to_ascii_uppercase()
            }
        } else {
            ascii
        };

        Some(ascii as char)
    }
}

/// Keyboard interrupt handler - called from the IRQ handler
pub fn handle_interrupt() {
    let scancode = read_scancode();
    
    if let Some(c) = process_scancode(scancode) {
        // For now, just print the character
        kprint!("{}", c);
    }
    
    // Send End of Interrupt to PIC
    pic::send_eoi(pic::IRQ_KEYBOARD);
}
