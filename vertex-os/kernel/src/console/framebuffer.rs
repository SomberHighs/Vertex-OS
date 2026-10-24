//! Linear framebuffer access.

use limine::framebuffer::Framebuffer as LimineFramebuffer;

/// A packed-RGB framebuffer provided by the bootloader.
pub struct Framebuffer {
    base: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bytes_per_pixel: usize,
    red_shift: u8,
    green_shift: u8,
    blue_shift: u8,
    red_size: u8,
    green_size: u8,
    blue_size: u8,
}

// The console serialises access to the framebuffer with a spinlock.
unsafe impl Send for Framebuffer {}
unsafe impl Sync for Framebuffer {}

impl Framebuffer {
    /// Build a framebuffer wrapper, returning `None` for unsupported formats.
    pub fn from_limine(fb: &LimineFramebuffer) -> Option<Self> {
        let bytes_per_pixel = (fb.bpp / 8) as usize;
        if bytes_per_pixel != 3 && bytes_per_pixel != 4 {
            return None;
        }
        if fb.width == 0 || fb.height == 0 || fb.pitch < fb.width * bytes_per_pixel as u64 {
            return None;
        }

        Some(Self {
            base: fb.address() as *mut u8,
            width: fb.width as usize,
            height: fb.height as usize,
            pitch: fb.pitch as usize,
            bytes_per_pixel,
            red_shift: fb.red_mask_shift,
            green_shift: fb.green_mask_shift,
            blue_shift: fb.blue_mask_shift,
            red_size: fb.red_mask_size,
            green_size: fb.green_mask_size,
            blue_size: fb.blue_mask_size,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Width and height in pixels.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Copy a full frame of `0x00RRGGBB` pixels into the framebuffer.
    ///
    /// This is the hot path for the GUI compositor: it encodes each pixel once
    /// and writes it without the per-call bounds check of [`Self::put_pixel`].
    pub fn blit(&mut self, src: &[u32]) {
        if self.width == 0 || self.height == 0 || src.len() < self.width * self.height {
            return;
        }
        let base = self.base;
        let pitch = self.pitch;
        let bpp = self.bytes_per_pixel;

        for y in 0..self.height {
            let row = src[y * self.width..(y + 1) * self.width].iter();
            let mut offset = y * pitch;
            for &color in row {
                let packed = self.encode(color);
                unsafe {
                    let target = base.add(offset);
                    match bpp {
                        4 => core::ptr::write_volatile(target as *mut u32, packed),
                        3 => {
                            core::ptr::write_volatile(target, packed as u8);
                            core::ptr::write_volatile(target.add(1), (packed >> 8) as u8);
                            core::ptr::write_volatile(target.add(2), (packed >> 16) as u8);
                        }
                        _ => {}
                    }
                }
                offset += bpp;
            }
        }
    }

    /// Pack a `0x00RRGGBB` colour into the framebuffer's native format.
    #[inline]
    fn encode(&self, color: u32) -> u32 {
        let r = (color >> 16) & 0xFF;
        let g = (color >> 8) & 0xFF;
        let b = color & 0xFF;

        // Values are already 8 bits wide; scale down for smaller channels.
        let scale = |value: u32, size: u8| -> u32 {
            if size == 0 {
                0
            } else if size >= 8 {
                value >> (8 - 8)
            } else {
                value >> (8 - size as u32)
            }
        };

        let mut packed = 0u32;
        if self.red_size > 0 {
            packed |= scale(r, self.red_size) << self.red_shift;
        }
        if self.green_size > 0 {
            packed |= scale(g, self.green_size) << self.green_shift;
        }
        if self.blue_size > 0 {
            packed |= scale(b, self.blue_size) << self.blue_shift;
        }
        packed
    }

    #[inline]
    pub fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = y * self.pitch + x * self.bytes_per_pixel;
        let packed = self.encode(color);
        unsafe {
            let target = self.base.add(offset);
            match self.bytes_per_pixel {
                4 => core::ptr::write_volatile(target as *mut u32, packed),
                3 => {
                    core::ptr::write_volatile(target, packed as u8);
                    core::ptr::write_volatile(target.add(1), (packed >> 8) as u8);
                    core::ptr::write_volatile(target.add(2), (packed >> 16) as u8);
                }
                _ => {}
            }
        }
    }

    /// Fill the whole screen.
    pub fn clear(&mut self, color: u32) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.put_pixel(x, y, color);
            }
        }
    }

    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        for dy in 0..h {
            for dx in 0..w {
                self.put_pixel(x + dx, y + dy, color);
            }
        }
    }

    /// Scroll the contents up by `pixels`, filling the exposed band.
    pub fn scroll(&mut self, pixels: usize, fill: u32) {
        if pixels >= self.height {
            self.clear(fill);
            return;
        }
        unsafe {
            let src = self.base.add(pixels * self.pitch);
            let count = (self.height - pixels) * self.pitch;
            core::ptr::copy(src, self.base, count);
        }
        for y in (self.height - pixels)..self.height {
            for x in 0..self.width {
                self.put_pixel(x, y, fill);
            }
        }
    }
}
