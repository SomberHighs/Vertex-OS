//! Console output via framebuffer

pub mod framebuffer;
pub mod font;

use framebuffer::Framebuffer;
use font::FONT;
use crate::sync::SpinLock;
use core::fmt::{self, Write};

/// Console state
struct Console {
    fb: Option<Framebuffer>,
    col: usize,
    row: usize,
    fg_color: u32,
    bg_color: u32,
}

impl Console {
    const fn new() -> Self {
        Self {
            fb: None,
            col: 0,
            row: 0,
            fg_color: 0xFFFFFF, // White
            bg_color: 0x000000, // Black
        }
    }
    
    fn init(&mut self, fb: Framebuffer) {
        self.fb = Some(fb);
        self.clear();
    }
    
    fn clear(&mut self) {
        if let Some(ref mut fb) = self.fb {
            fb.clear(self.bg_color);
        }
        self.col = 0;
        self.row = 0;
    }
    
    fn cols(&self) -> usize {
        self.fb.as_ref().map(|fb| fb.width() / FONT.width()).unwrap_or(0)
    }
    
    fn rows(&self) -> usize {
        self.fb.as_ref().map(|fb| fb.height() / FONT.height()).unwrap_or(0)
    }
    
    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        
        if self.row >= self.rows() {
            self.scroll();
            self.row = self.rows() - 1;
        }
    }
    
    fn scroll(&mut self) {
        if let Some(ref mut fb) = self.fb {
            let font_height = FONT.height();
            fb.scroll(font_height, self.bg_color);
        }
    }
    
    fn put_char(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            '\r' => self.col = 0,
            '\t' => {
                let tab_size = 4;
                let spaces = tab_size - (self.col % tab_size);
                for _ in 0..spaces {
                    self.put_char(' ');
                }
            }
            c => {
                if self.col >= self.cols() {
                    self.newline();
                }
                
                if let Some(ref mut fb) = self.fb {
                    let x = self.col * FONT.width();
                    let y = self.row * FONT.height();
                    FONT.draw_char(fb, x, y, c, self.fg_color, self.bg_color);
                }
                
                self.col += 1;
            }
        }
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.put_char(c);
        }
        Ok(())
    }
}

static CONSOLE: SpinLock<Console> = SpinLock::new(Console::new());

/// Initialize the console with a framebuffer
pub fn init(limine_fb: &limine::framebuffer::Framebuffer) {
    let fb = Framebuffer::from_limine(limine_fb);
    CONSOLE.lock().init(fb);
}

/// Print formatted text to the console
pub fn _print(args: fmt::Arguments) {
    CONSOLE.lock().write_fmt(args).unwrap();
}

/// Print macro
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

/// Print line macro
#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}
