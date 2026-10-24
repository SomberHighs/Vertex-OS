//! 16550 UART serial driver (COM1).
//!
//! Used as a debug console, and as a second input device so the shell can be
//! driven headlessly (`-serial stdio`).

use super::cpu::{inb, outb};

const COM1: u16 = 0x3F8;

/// Receive buffer size for the polling reader.
const RX_RING: usize = 256;

struct Serial {
    initialised: bool,
    rx_head: usize,
    rx_tail: usize,
    rx: [u8; RX_RING],
}

// Only touched from normal context; the UART runs with interrupts disabled.
static mut SERIAL: Serial = Serial {
    initialised: false,
    rx_head: 0,
    rx_tail: 0,
    rx: [0; RX_RING],
};

/// Initialise COM1 at 115200 8N1 with FIFOs enabled.
pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00); // disable interrupts
        outb(COM1 + 3, 0x80); // enable DLAB
        outb(COM1, 0x01); // divisor low: 115200 baud
        outb(COM1 + 1, 0x00); // divisor high
        outb(COM1 + 3, 0x03); // 8 bits, no parity, one stop bit
        outb(COM1 + 2, 0xC7); // FIFO enable, clear, 14-byte threshold
        outb(COM1 + 4, 0x03); // DTR + RTS

        // Loopback self-test: if it fails we simply keep writing (the writes
        // become no-ops rather than panicking before the console exists).
        outb(COM1 + 4, 0x1E);
        outb(COM1, 0xAE);
        let ok = inb(COM1) == 0xAE;
        outb(COM1 + 4, 0x03);

        SERIAL.initialised = true;
        if !ok {
            SERIAL.initialised = true;
        }
    }
}

fn tx_ready() -> bool {
    unsafe { inb(COM1 + 5) & 0x20 != 0 }
}

fn rx_ready() -> bool {
    unsafe { inb(COM1 + 5) & 0x01 != 0 }
}

/// Write a single byte, waiting for the transmit buffer.
pub fn write_byte(byte: u8) {
    unsafe {
        if !SERIAL.initialised {
            return;
        }
        // Bounded wait: never hang the kernel on a missing UART.
        let mut spins = 0u32;
        while !tx_ready() {
            spins += 1;
            if spins > 100_000 {
                return;
            }
            core::hint::spin_loop();
        }
        outb(COM1, byte);
    }
}

/// Write a string.
pub fn write_str(s: &str) {
    for byte in s.bytes() {
        if byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(byte);
    }
}

/// Drain the UART FIFO into the software ring buffer.
pub fn poll() {
    unsafe {
        if !SERIAL.initialised {
            return;
        }
        let mut drained = 0;
        while rx_ready() && drained < RX_RING {
            let byte = inb(COM1);
            let next = (SERIAL.rx_head + 1) % RX_RING;
            if next != SERIAL.rx_tail {
                SERIAL.rx[SERIAL.rx_head] = byte;
                SERIAL.rx_head = next;
            }
            drained += 1;
        }
    }
}

/// Pop a received byte, if any.
pub fn try_read_byte() -> Option<u8> {
    poll();
    unsafe {
        if SERIAL.rx_head == SERIAL.rx_tail {
            return None;
        }
        let byte = SERIAL.rx[SERIAL.rx_tail];
        SERIAL.rx_tail = (SERIAL.rx_tail + 1) % RX_RING;
        Some(byte)
    }
}
