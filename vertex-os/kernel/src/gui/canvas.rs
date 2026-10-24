//! Software canvas: the compositor's back buffer and drawing primitives.
//!
//! The kernel never enables the FPU, so every shape here is rasterised with
//! integer arithmetic. Circles are anti-aliased by comparing an integer square
//! root against the ring radii, which costs a few instructions per pixel and
//! keeps edges smooth without floating point.
//!
//! A full frame is drawn off-screen and handed to the framebuffer in one blit,
//! so animations never tear.

use crate::console::font::FONT;
use crate::console::framebuffer::Framebuffer;
use crate::mm::heap;

/// An axis-aligned rectangle in pixels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Rect {
    pub const fn new(x: usize, y: usize, w: usize, h: usize) -> Self {
        Self { x, y, w, h }
    }

    pub const fn right(&self) -> usize {
        self.x + self.w
    }

    pub const fn bottom(&self) -> usize {
        self.y + self.h
    }

    pub const fn center(&self) -> (usize, usize) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn inset(&self, amount: usize) -> Rect {
        Rect::new(
            self.x + amount,
            self.y + amount,
            self.w.saturating_sub(amount * 2),
            self.h.saturating_sub(amount * 2),
        )
    }

    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
    }
}

/// Integer square root (Newton's method).
pub fn isqrt(value: u64) -> u64 {
    if value == 0 {
        return 0;
    }
    let mut x = value;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + value / x) / 2;
    }
    x
}

/// Blend `b` over `a`; `t` is the weight of `b` in 0..=255.
#[inline]
pub fn mix(a: u32, b: u32, t: u8) -> u32 {
    if t == 0 {
        return a;
    }
    if t == 255 {
        return b;
    }
    let t = t as u32;
    let inv = 255 - t;
    let r = (((a >> 16) & 0xFF) * inv + ((b >> 16) & 0xFF) * t) / 255;
    let g = (((a >> 8) & 0xFF) * inv + ((b >> 8) & 0xFF) * t) / 255;
    let bl = ((a & 0xFF) * inv + (b & 0xFF) * t) / 255;
    (r << 16) | (g << 8) | bl
}

/// A heap-backed RGB frame buffer.
pub struct Canvas {
    pixels: &'static mut [u32],
    width: usize,
    height: usize,
    clip: Rect,
}

impl Canvas {
    /// Allocate a canvas covering the whole screen.
    pub fn new(width: usize, height: usize) -> Option<Self> {
        let bytes = width.checked_mul(height)?.checked_mul(4)?;
        // Page-align so the bump allocator never wastes most of a page.
        let raw = unsafe { heap::alloc(bytes, 4096) as *mut u32 };
        if raw.is_null() {
            return None;
        }
        let pixels = unsafe { core::slice::from_raw_parts_mut(raw, width * height) };
        Some(Self {
            pixels,
            width,
            height,
            clip: Rect::new(0, 0, width, height),
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    /// Restrict drawing to `rect` (clipped to the canvas).
    pub fn set_clip(&mut self, rect: Rect) {
        self.clip = rect.intersect(&self.bounds());
    }

    pub fn reset_clip(&mut self) {
        self.clip = self.bounds();
    }

    #[inline]
    fn in_clip(&self, x: usize, y: usize) -> bool {
        x >= self.clip.x && x < self.clip.right() && y >= self.clip.y && y < self.clip.bottom()
    }

    /// Write one pixel if it falls inside the clip rectangle.
    #[inline]
    pub fn put(&mut self, x: usize, y: usize, color: u32) {
        if self.in_clip(x, y) {
            self.pixels[y * self.width + x] = color;
        }
    }

    /// Blend one pixel over what is already there.
    #[inline]
    pub fn put_blend(&mut self, x: usize, y: usize, color: u32, alpha: u8) {
        if alpha == 0 || !self.in_clip(x, y) {
            return;
        }
        let index = y * self.width + x;
        if alpha == 255 {
            self.pixels[index] = color;
        } else {
            self.pixels[index] = mix(self.pixels[index], color, alpha);
        }
    }

    pub fn clear(&mut self, color: u32) {
        let area = self.clip;
        for y in area.y..area.bottom() {
            let start = y * self.width + area.x;
            for pixel in &mut self.pixels[start..start + area.w] {
                *pixel = color;
            }
        }
    }

    /// Fill the clip rectangle with a top-to-bottom gradient.
    pub fn vertical_gradient(&mut self, top: u32, bottom: u32) {
        let area = self.clip;
        for y in area.y..area.bottom() {
            let t = ((y - area.y) * 255 / area.h.max(1)) as u8;
            let color = mix(top, bottom, t);
            let start = y * self.width + area.x;
            for pixel in &mut self.pixels[start..start + area.w] {
                *pixel = color;
            }
        }
    }

    pub fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        self.fill_alpha(x, y, w, h, color, 255);
    }

    pub fn fill_alpha(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32, alpha: u8) {
        if alpha == 0 {
            return;
        }
        let area = Rect::new(x, y, w, h).intersect(&self.clip);
        if area.is_empty() {
            return;
        }
        for py in area.y..area.bottom() {
            let start = py * self.width + area.x;
            if alpha == 255 {
                for pixel in &mut self.pixels[start..start + area.w] {
                    *pixel = color;
                }
            } else {
                for pixel in &mut self.pixels[start..start + area.w] {
                    *pixel = mix(*pixel, color, alpha);
                }
            }
        }
    }

    /// Fill a rectangle with rounded corners.
    pub fn round_rect(&mut self, rect: Rect, radius: usize, color: u32, alpha: u8) {
        if rect.is_empty() || alpha == 0 {
            return;
        }
        let radius = radius.min(rect.w / 2).min(rect.h / 2);
        for y in rect.y..rect.bottom() {
            let dy = if y < rect.y + radius {
                (rect.y + radius) - y
            } else if y >= rect.bottom().saturating_sub(radius) {
                y + 1 - rect.bottom().saturating_sub(radius)
            } else {
                0
            };
            let inset = if dy == 0 {
                0
            } else {
                radius - isqrt((radius * radius).saturating_sub(dy * dy) as u64) as usize
            };
            let x = rect.x + inset;
            let w = rect.w.saturating_sub(inset * 2);
            if w == 0 {
                continue;
            }
            self.fill_alpha(x, y, w, 1, color, alpha);
        }
    }

    /// Draw a one pixel outline.
    pub fn border(&mut self, rect: Rect, color: u32, alpha: u8) {
        if rect.is_empty() {
            return;
        }
        self.fill_alpha(rect.x, rect.y, rect.w, 1, color, alpha);
        self.fill_alpha(rect.x, rect.bottom() - 1, rect.w, 1, color, alpha);
        self.fill_alpha(rect.x, rect.y, 1, rect.h, color, alpha);
        self.fill_alpha(rect.right() - 1, rect.y, 1, rect.h, color, alpha);
    }

    pub fn hline(&mut self, y: usize, x0: usize, x1: usize, color: u32, alpha: u8) {
        if x1 <= x0 {
            return;
        }
        self.fill_alpha(x0, y, x1 - x0, 1, color, alpha);
    }

    pub fn vline(&mut self, x: usize, y0: usize, y1: usize, color: u32, alpha: u8) {
        if y1 <= y0 {
            return;
        }
        self.fill_alpha(x, y0, 1, y1 - y0, color, alpha);
    }

    /// Bresenham line with per-pixel blending.
    pub fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, color: u32, alpha: u8) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && y >= 0 {
                self.put_blend(x as usize, y as usize, color, alpha);
            }
            if x == x1 && y == y1 {
                break;
            }
            let doubled = error * 2;
            if doubled >= dy {
                error += dy;
                x += sx;
            }
            if doubled <= dx {
                error += dx;
                y += sy;
            }
        }
    }

    /// Filled disc.
    pub fn disc(&mut self, cx: i64, cy: i64, radius: i64, color: u32, alpha: u8) {
        self.annulus(cx, cy, 0, radius, color, alpha);
    }

    /// Anti-aliased ring between `inner` and `outer` radius (inclusive).
    pub fn annulus(&mut self, cx: i64, cy: i64, inner: i64, outer: i64, color: u32, alpha: u8) {
        if outer <= 0 || alpha == 0 {
            return;
        }
        let x0 = (cx - outer).max(self.clip.x as i64) as usize;
        let y0 = (cy - outer).max(self.clip.y as i64) as usize;
        let x1 = ((cx + outer) as usize).min(self.clip.right().saturating_sub(1));
        let y1 = ((cy + outer) as usize).min(self.clip.bottom().saturating_sub(1));
        if x1 < x0 || y1 < y0 {
            return;
        }

        for y in y0..=y1 {
            let dy = y as i64 - cy;
            for x in x0..=x1 {
                let dx = x as i64 - cx;
                let distance = isqrt((dx * dx + dy * dy) as u64) as i64;
                if distance > outer + 1 || distance < inner - 1 {
                    continue;
                }
                // One pixel of coverage falloff on both sides of the band.
                let mut coverage: i64 = 255;
                if distance > outer {
                    coverage = 0;
                } else if distance < inner {
                    coverage = 0;
                } else if distance == outer {
                    coverage = 128;
                } else if inner > 0 && distance == inner {
                    coverage = 128;
                }
                if coverage > 0 {
                    let combined = (alpha as i64 * coverage / 255) as u8;
                    self.put_blend(x, y, color, combined);
                }
            }
        }
    }

    /// Ring of the given thickness centred on `radius`.
    pub fn ring(&mut self, cx: i64, cy: i64, radius: i64, thickness: i64, color: u32, alpha: u8) {
        let half = (thickness / 2).max(1);
        self.annulus(cx, cy, radius - half, radius + half, color, alpha);
    }

    /// Arc rasterised by walking the ring in one-degree steps.
    ///
    /// Angles are in degrees, zero at three o'clock, growing clockwise on
    /// screen (y grows downwards).
    pub fn arc(
        &mut self,
        cx: i64,
        cy: i64,
        radius: i64,
        start: i64,
        end: i64,
        thickness: i64,
        color: u32,
        alpha: u8,
    ) {
        let steps = (end - start).max(1);
        for step in 0..steps {
            let (sin, cos) = sin_cos(start + step);
            let x = cx + (radius * cos) / 10_000;
            let y = cy + (radius * sin) / 10_000;
            self.disc(x, y, (thickness / 2).max(1), color, alpha);
        }
    }

    /// A soft radial glow, built from concentric translucent discs.
    pub fn glow(&mut self, cx: i64, cy: i64, radius: i64, color: u32, strength: u8) {
        let rings = 14i64;
        for step in (1..=rings).rev() {
            let r = radius * step / rings;
            let alpha = (strength as i64 * (rings - step + 1) / (rings * 6)) as u8;
            self.disc(cx, cy, r, color, alpha);
        }
    }

    /// A faint square dot grid, used to give panels a plotted-paper feel.
    pub fn dot_grid(&mut self, spacing: usize, color: u32, alpha: u8) {
        let area = self.clip;
        let mut y = area.y + spacing / 2;
        while y < area.bottom() {
            let mut x = area.x + spacing / 2;
            while x < area.right() {
                self.put_blend(x, y, color, alpha);
                x += spacing;
            }
            y += spacing;
        }
    }

    // --- text --------------------------------------------------------------

    /// Width in pixels of `text` at the given scale.
    pub fn text_width(text: &str, scale: usize) -> usize {
        text.chars().count() * 8 * scale.max(1)
    }

    /// Draw `text`, returning the advance width.
    ///
    /// `tracking` adds extra pixels between glyphs, which is what makes the
    /// small uppercase labels read as typography rather than terminal output.
    pub fn text(&mut self, x: usize, y: usize, text: &str, color: u32, scale: usize, alpha: u8) -> usize {
        self.text_tracked(x, y, text, color, scale, 0, alpha)
    }

    pub fn text_tracked(
        &mut self,
        x0: usize,
        y0: usize,
        text: &str,
        color: u32,
        scale: usize,
        tracking: usize,
        alpha: u8,
    ) -> usize {
        let scale = scale.max(1);
        let mut pen = x0;
        for ch in text.chars() {
            let rows = FONT.glyph_rows(ch);
            for (row, bits) in rows.iter().enumerate() {
                if *bits == 0 {
                    continue;
                }
                for col in 0..8 {
                    if (bits >> (7 - col)) & 1 == 0 {
                        continue;
                    }
                    let px = pen + col * scale;
                    let py = y0 + row * scale;
                    if scale == 1 {
                        self.put_blend(px, py, color, alpha);
                    } else {
                        self.fill_alpha(px, py, scale, scale, color, alpha);
                    }
                }
            }
            pen += 8 * scale + tracking;
        }
        pen.saturating_sub(x0).saturating_sub(tracking)
    }

    /// Draw text so that it ends at `right`.
    pub fn text_right(
        &mut self,
        right: usize,
        y: usize,
        text: &str,
        color: u32,
        scale: usize,
        alpha: u8,
    ) -> usize {
        let width = Canvas::text_width(text, scale);
        let x = right.saturating_sub(width);
        self.text(x, y, text, color, scale, alpha);
        width
    }

    /// Draw text horizontally centred in `rect`.
    pub fn text_centered(
        &mut self,
        rect: Rect,
        y: usize,
        text: &str,
        color: u32,
        scale: usize,
        alpha: u8,
    ) {
        let width = Canvas::text_width(text, scale);
        let x = rect.x + rect.w.saturating_sub(width) / 2;
        self.text(x, y, text, color, scale, alpha);
    }

    /// Truncate `text` to at most `columns` characters.
    pub fn clip_text(text: &str, columns: usize) -> &str {
        match text.char_indices().nth(columns) {
            Some((index, _)) => &text[..index],
            None => text,
        }
    }

    /// Draw text truncated to `columns` characters, with an ellipsis byte when
    /// it had to be cut.
    pub fn text_clipped(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        columns: usize,
        color: u32,
        scale: usize,
        alpha: u8,
    ) {
        if columns == 0 {
            return;
        }
        let length = text.chars().count();
        if length <= columns {
            self.text(x, y, text, color, scale, alpha);
            return;
        }
        let kept = Canvas::clip_text(text, columns.saturating_sub(1));
        self.text(x, y, kept, color, scale, alpha);
        self.text(x + kept.chars().count() * 8 * scale, y, ".", color, scale, alpha);
    }

    /// Copy the frame to the framebuffer.
    pub fn present(&self, fb: &mut Framebuffer) {
        if fb.dimensions() != (self.width, self.height) {
            return;
        }
        fb.blit(self.pixels);
    }
}

/// Sine and cosine in fixed point (10_000 = 1.0) for whole degrees.
pub fn sin_cos(degrees: i64) -> (i64, i64) {
    let angle = ((degrees % 360) + 360) % 360;
    let (sin, cos) = match angle {
        0..=90 => (TABLE[angle as usize], TABLE[(90 - angle) as usize]),
        91..=180 => (TABLE[(180 - angle) as usize], -TABLE[(angle - 90) as usize]),
        181..=270 => (-TABLE[(angle - 180) as usize], -TABLE[(270 - angle) as usize]),
        _ => (-TABLE[(360 - angle) as usize], TABLE[(angle - 270) as usize]),
    };
    (sin, cos)
}

/// sin(0..=90 degrees), scaled by 10_000.
static TABLE: [i64; 91] = [
    0, 175, 349, 523, 698, 872, 1045, 1219, 1392, 1564, 1736, 1908, 2079, 2250, 2419, 2588, 2756,
    2924, 3090, 3256, 3420, 3584, 3746, 3907, 4067, 4226, 4384, 4540, 4695, 4848, 5000, 5150, 5299,
    5446, 5592, 5736, 5878, 6018, 6157, 6293, 6428, 6561, 6691, 6820, 6947, 7071, 7193, 7314, 7431,
    7547, 7660, 7771, 7880, 7986, 8090, 8192, 8290, 8387, 8480, 8572, 8660, 8746, 8829, 8910, 8988,
    9063, 9135, 9205, 9272, 9336, 9397, 9455, 9511, 9563, 9613, 9659, 9703, 9744, 9781, 9816, 9848,
    9877, 9903, 9925, 9945, 9962, 9976, 9986, 9994, 9998, 10_000,
];
