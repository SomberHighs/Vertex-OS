//! Framebuffer handling

use volatile::Volatile;

/// Framebuffer wrapper
pub struct Framebuffer {
    buffer: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bpp: usize,
}

// Safety: We ensure single-threaded access through spinlock
unsafe impl Send for Framebuffer {}
unsafe impl Sync for Framebuffer {}

impl Framebuffer {
    /// Create a framebuffer from Limine's framebuffer info
    pub fn from_limine(fb: &limine::framebuffer::Framebuffer) -> Self {
        Self {
            buffer: fb.addr() as *mut u8,
            width: fb.width() as usize,
            height: fb.height() as usize,
            pitch: fb.pitch() as usize,
            bpp: fb.bpp() as usize / 8,
        }
    }
    
    /// Get the width in pixels
    pub fn width(&self) -> usize {
        self.width
    }
    
    /// Get the height in pixels
    pub fn height(&self) -> usize {
        self.height
    }
    
    /// Clear the framebuffer with a color
    pub fn clear(&mut self, color: u32) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.put_pixel(x, y, color);
            }
        }
    }
    
    /// Put a pixel at (x, y)
    pub fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        
        let offset = y * self.pitch + x * self.bpp;
        
        unsafe {
            let pixel = self.buffer.add(offset) as *mut Volatile<u32>;
            (*pixel).write(color);
        }
    }
    
    /// Scroll the framebuffer up by `lines` pixels
    pub fn scroll(&mut self, lines: usize, fill_color: u32) {
        if lines >= self.height {
            self.clear(fill_color);
            return;
        }
        
        // Copy pixels up
        unsafe {
            let src = self.buffer.add(lines * self.pitch);
            let dst = self.buffer;
            let count = (self.height - lines) * self.pitch;
            core::ptr::copy(src, dst, count);
        }
        
        // Fill the bottom with background color
        for y in (self.height - lines)..self.height {
            for x in 0..self.width {
                self.put_pixel(x, y, fill_color);
            }
        }
    }
    
    /// Fill a rectangle
    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        for dy in 0..h {
            for dx in 0..w {
                self.put_pixel(x + dx, y + dy, color);
            }
        }
    }
}
