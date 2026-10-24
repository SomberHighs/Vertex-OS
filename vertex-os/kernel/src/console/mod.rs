//! Text console: framebuffer rendering mirrored to the serial port.

pub mod font;
pub mod framebuffer;

use core::fmt::{self, Write};

use crate::arch::x86_64::serial;
use crate::sync::SpinLock;
use font::FONT;
use framebuffer::Framebuffer;

/// Default foreground (near-white).
pub const FG_DEFAULT: u32 = 0xCDD6F4;
/// Muted text.
pub const FG_DIM: u32 = 0x7F849C;
/// Accent used for the prompt and headings.
pub const FG_ACCENT: u32 = 0x89B4FA;
/// Success.
pub const FG_OK: u32 = 0xA6E3A1;
/// Warning.
pub const FG_WARN: u32 = 0xF9E2AF;
/// Error.
pub const FG_ERROR: u32 = 0xF38BA8;
/// Highlight.
pub const FG_MAGENTA: u32 = 0xCBA6F7;
/// Background.
pub const BG_DEFAULT: u32 = 0x11111B;

struct Console {
    fb: Option<Framebuffer>,
    col: usize,
    row: usize,
    fg: u32,
    bg: u32,
    cursor: Option<(usize, usize)>,
}

impl Console {
    const fn new() -> Self {
        Self {
            fb: None,
            col: 0,
            row: 0,
            fg: FG_DEFAULT,
            bg: BG_DEFAULT,
            cursor: None,
        }
    }

    fn init(&mut self, fb: Framebuffer) {
        self.fb = Some(fb);
        self.clear();
    }

    fn cols(&self) -> usize {
        self.fb
            .as_ref()
            .map(|fb| fb.width() / FONT.width())
            .unwrap_or(80)
    }

    fn rows(&self) -> usize {
        self.fb
            .as_ref()
            .map(|fb| fb.height() / FONT.height())
            .unwrap_or(25)
    }

    fn clear(&mut self) {
        self.erase_cursor();
        if let Some(fb) = self.fb.as_mut() {
            fb.clear(self.bg);
        }
        self.col = 0;
        self.row = 0;
        self.draw_cursor();
    }

    fn erase_cursor(&mut self) {
        if let (Some(fb), Some((col, row))) = (self.fb.as_mut(), self.cursor.take()) {
            FONT.draw_char(
                fb,
                col * FONT.width(),
                row * FONT.height(),
                ' ',
                self.fg,
                self.bg,
            );
        }
    }

    fn draw_cursor(&mut self) {
        if let Some(fb) = self.fb.as_mut() {
            let x = self.col * FONT.width();
            let y = self.row * FONT.height() + FONT.height() - 2;
            fb.fill_rect(x, y, FONT.width(), 2, self.fg);
            self.cursor = Some((self.col, self.row));
        }
    }

    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= self.rows() {
            if let Some(fb) = self.fb.as_mut() {
                fb.scroll(FONT.height(), self.bg);
            }
            self.row = self.rows().saturating_sub(1);
        }
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.cols().saturating_sub(1);
        } else {
            return;
        }
        if let Some(fb) = self.fb.as_mut() {
            FONT.draw_char(
                fb,
                self.col * FONT.width(),
                self.row * FONT.height(),
                ' ',
                self.fg,
                self.bg,
            );
        }
    }

    fn put_char(&mut self, c: char) {
        self.erase_cursor();

        match c {
            '\n' => self.newline(),
            '\r' => self.col = 0,
            '\x08' => self.backspace(),
            '\t' => {
                let spaces = 4 - (self.col % 4);
                for _ in 0..spaces {
                    self.put_char(' ');
                }
                return;
            }
            c => {
                if self.col >= self.cols() {
                    self.newline();
                }
                if let Some(fb) = self.fb.as_mut() {
                    FONT.draw_char(
                        fb,
                        self.col * FONT.width(),
                        self.row * FONT.height(),
                        c,
                        self.fg,
                        self.bg,
                    );
                }
                self.col += 1;
            }
        }

        self.draw_cursor();
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        serial::write_str(s);
        for c in s.chars() {
            self.put_char(c);
        }
        Ok(())
    }
}

static CONSOLE: SpinLock<Console> = SpinLock::new(Console::new());

// --- output capture ---------------------------------------------------------
//
// A graphical front end runs shell commands and shows their text, but the shell
// writes through `kprintln!` straight at the console. Rather than thread a sink
// through every command, printing is mirrored into this buffer while capture is
// active. The text console keeps working: only the GUI reads the buffer, and it
// holds the framebuffer in the meantime.

/// Maximum bytes of captured output.
pub const CAPTURE_CAPACITY: usize = 4096;

struct Capture {
    active: bool,
    len: usize,
    buffer: [u8; CAPTURE_CAPACITY],
}

static CAPTURE: SpinLock<Capture> = SpinLock::new(Capture {
    active: false,
    len: 0,
    buffer: [0; CAPTURE_CAPACITY],
});

struct CaptureWriter;

impl fmt::Write for CaptureWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut capture = CAPTURE.lock();
        if capture.active {
            for &byte in s.as_bytes() {
                let index = capture.len;
                if index < CAPTURE_CAPACITY {
                    capture.buffer[index] = byte;
                    capture.len = index + 1;
                }
            }
        }
        Ok(())
    }
}

fn capture(args: fmt::Arguments) {
    if !CAPTURE.lock().active {
        return;
    }
    let _ = CaptureWriter.write_fmt(args);
}

/// Mirror console output into the capture buffer until [`end_capture`].
pub fn begin_capture() {
    let mut capture = CAPTURE.lock();
    capture.active = true;
    capture.len = 0;
}

/// Stop capturing and discard the buffer.
pub fn end_capture() {
    let mut capture = CAPTURE.lock();
    capture.active = false;
    capture.len = 0;
}

/// Copy the captured text out without clearing it.
pub fn capture_to(out: &mut [u8]) -> usize {
    let capture = CAPTURE.lock();
    let length = capture.len.min(out.len());
    out[..length].copy_from_slice(&capture.buffer[..length]);
    length
}

/// Release the capture lock after a fault.
pub fn force_unlock_capture() {
    unsafe {
        CAPTURE.force_unlock();
    }
}

/// Hand the framebuffer to a graphical front end.
///
/// The console keeps printing to the serial port; it just stops drawing.
pub fn detach_framebuffer() -> Option<framebuffer::Framebuffer> {
    let mut console = CONSOLE.lock();
    console.cursor = None;
    console.fb.take()
}

/// Take the framebuffer back from a front end and resume text output.
pub fn attach_framebuffer(fb: framebuffer::Framebuffer) {
    let mut console = CONSOLE.lock();
    console.fb = Some(fb);
    console.cursor = None;
    console.col = 0;
    console.row = 0;
    console.clear();
}

/// Attach the bootloader framebuffer and clear the screen.
pub fn init(limine_fb: &limine::framebuffer::Framebuffer) -> bool {
    match Framebuffer::from_limine(limine_fb) {
        Some(fb) => {
            CONSOLE.lock().init(fb);
            true
        }
        None => false,
    }
}

/// True once a framebuffer is attached.
pub fn has_framebuffer() -> bool {
    CONSOLE.lock().fb.is_some()
}

/// Clear the screen.
pub fn clear() {
    CONSOLE.lock().clear();
}

/// Terminal size in characters.
pub fn size() -> (usize, usize) {
    let console = CONSOLE.lock();
    (console.cols(), console.rows())
}

pub fn _print(args: fmt::Arguments) {
    capture(args);
    let mut console = CONSOLE.lock();
    let _ = console.write_fmt(args);
}

/// Print in a specific colour, restoring the previous colour afterwards.
pub fn _print_colored(color: u32, args: fmt::Arguments) {
    capture(args);
    let mut console = CONSOLE.lock();
    let previous = console.fg;
    console.fg = color;
    let _ = console.write_fmt(args);
    console.erase_cursor();
    console.fg = previous;
    console.draw_cursor();
}

/// Release the console lock after a fault, so panic reporting cannot deadlock.
pub fn force_unlock() {
    unsafe {
        CONSOLE.force_unlock();
    }
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kprint_color {
    ($color:expr, $($arg:tt)*) => {
        $crate::console::_print_colored($color, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! kprintln_color {
    ($color:expr, $($arg:tt)*) => {
        $crate::console::_print_colored($color, format_args!("{}\n", format_args!($($arg)*)))
    };
}
