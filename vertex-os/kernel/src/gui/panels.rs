//! Panels around the constellation: header, breadcrumb, inspector, footer and
//! the boot splash.
//!
//! Layout is computed from the screen size once per frame. The inspector is a
//! fixed-width column on the right; everything else shares the remaining space.

use crate::gui::canvas::{mix, Canvas, Rect};
use crate::gui::theme;
use crate::gui::{App, Entry, ACTIVITY_LINES};
use crate::vfs::node;

pub const HEADER_HEIGHT: usize = 72;
pub const FOOTER_HEIGHT: usize = 64;
const INSPECTOR_PADDING: usize = 28;

/// Rectangles of the current frame.
pub struct Layout {
    pub header: Rect,
    pub body: Rect,
    pub stage: Rect,
    pub inspector: Rect,
    pub footer: Rect,
}

pub fn layout(width: usize, height: usize) -> Layout {
    let header = Rect::new(0, 0, width, HEADER_HEIGHT);
    let footer = Rect::new(0, height - FOOTER_HEIGHT, width, FOOTER_HEIGHT);
    let body = Rect::new(
        0,
        HEADER_HEIGHT,
        width,
        height - HEADER_HEIGHT - FOOTER_HEIGHT,
    );

    let inspector_width = (width * 34 / 100).clamp(330, 470);
    let inspector = Rect::new(width - inspector_width, body.y, inspector_width, body.h);
    let stage = Rect::new(0, body.y, width - inspector_width, body.h);

    Layout {
        header,
        body,
        stage,
        inspector,
        footer,
    }
}

/// The vector mark: two strokes forming a V, drawn rather than typeset.
fn logo(canvas: &mut Canvas, x: usize, y: usize, height: usize, color: u32) {
    let half = height / 2;
    let width = height * 5 / 6;
    let stroke = (height / 9).max(2) as usize;
    let bottom = y + height;
    for offset in 0..stroke {
        canvas.line(
            (x + offset) as i64,
            y as i64,
            (x + half) as i64,
            (bottom - offset) as i64,
            color,
            255,
        );
        canvas.line(
            (x + half) as i64,
            (bottom - offset) as i64,
            (x + width + offset) as i64,
            (y + offset) as i64,
            color,
            255,
        );
    }
}

pub fn header(canvas: &mut Canvas, layout: &Layout, nodes: usize, store: u64, uptime_s: u64) {
    let area = layout.header;
    canvas.set_clip(area);
    canvas.fill(area.x, area.y, area.w, area.h, theme::PANEL);
    canvas.hline(area.bottom() - 1, area.x, area.right(), theme::HAIRLINE, 255);

    logo(canvas, 30, 22, 28, theme::ACCENT);

    let wordmark_x = 74;
    let tracked = canvas.text_tracked(
        wordmark_x,
        16,
        "VERTEX",
        theme::TEXT,
        2,
        3,
        255,
    );
    let pill_x = wordmark_x + tracked + 14;
    canvas.round_rect(Rect::new(pill_x, 18, 62, 24), 6, theme::ACCENT, 26);
    canvas.border(Rect::new(pill_x, 18, 62, 24), theme::ACCENT, 80);
    canvas.text_tracked(pill_x + 13, 23, "LOOM", theme::ACCENT, 1, 3, 255);

    canvas.text_tracked(
        wordmark_x,
        46,
        "CONTENT-ADDRESSED NODE GRAPH",
        theme::TEXT_FAINT,
        1,
        2,
        255,
    );

    // Right-hand statistics, right to left.
    let uptime = FormatSeconds::new(uptime_s);
    let store = FormatBytes::new(store);
    let node_count = FormatUptime::new(nodes as u64);
    let mut right = area.right() - 30;
    let stats: [(&str, &str); 3] = [
        ("NODES", node_count.as_str()),
        ("STORE", store.as_str()),
        ("UPTIME", uptime.as_str()),
    ];
    for (label, value) in stats.iter() {
        let width = Canvas::text_width(value, 1)
            .max(Canvas::text_width(label, 1) + 2 * label.len().saturating_sub(1));
        canvas.text_right(right, 20, label, theme::TEXT_FAINT, 1, 255);
        canvas.text_right(right, 40, value, theme::TEXT, 1, 255);
        right = right.saturating_sub(width + 34);
        canvas.vline(right + 17, 22, 54, theme::HAIRLINE, 255);
    }

    canvas.reset_clip();
}

pub fn breadcrumb(canvas: &mut Canvas, stage: Rect, cwd: &str) {
    canvas.set_clip(stage);
    let mut x = stage.x + 30;
    let y = stage.y + 22;
    let segments: usize = cwd.split('/').filter(|part| !part.is_empty()).count();
    if segments == 0 {
        canvas.text_tracked(x, y, "ROOT", theme::ACCENT, 1, 2, 255);
        canvas.reset_clip();
        return;
    }

    canvas.text_tracked(x, y, "ROOT", theme::TEXT_FAINT, 1, 2, 255);
    x += Canvas::text_width("ROOT", 1) + 2 * 3;

    let mut seen = 0usize;
    for part in cwd.split('/').filter(|part| !part.is_empty()) {
        canvas.text_tracked(x, y, ">", theme::TEXT_FAINT, 1, 0, 255);
        x += Canvas::text_width(">", 1) + 12;
        seen += 1;
        let color = if seen == segments {
            theme::ACCENT
        } else {
            theme::TEXT_DIM
        };
        let upper = Canvas::clip_text(part, 24);
        canvas.text_tracked(x, y, upper, color, 1, 2, 255);
        x += Canvas::text_width(upper, 1) + 2 * upper.len().max(1);
    }
    canvas.reset_clip();
}

pub fn inspector(canvas: &mut Canvas, layout: &Layout, app: &App, ticks: u64) {
    let panel = layout.inspector;
    canvas.set_clip(panel);
    canvas.fill(panel.x, panel.y, panel.w, panel.h, theme::PANEL);
    canvas.vline(panel.x, panel.y, panel.bottom(), theme::HAIRLINE, 255);

    let left = panel.x + INSPECTOR_PADDING;
    let right = panel.right() - INSPECTOR_PADDING;
    let mut y = panel.y + 26;

    canvas.text_tracked(left, y, "INSPECTOR", theme::ACCENT, 1, 3, 255);
    y += 22;
    canvas.hline(y, left, right, theme::HAIRLINE, 255);
    y += 26;

    let Some(entry) = app.selected_entry() else {
        canvas.text(left, y, "no node selected", theme::TEXT_DIM, 1, 255);
        canvas.reset_clip();
        return;
    };

    let kind_color = theme::kind_color(entry.kind);
    let name_columns = (right - left) / 16;
    canvas.text_clipped(left, y, entry.name(), name_columns, theme::TEXT, 2, 255);
    y += 40;

    // Type pill plus a size caption on the same line.
    let tag = theme::kind_label(entry.kind);
    let pill_width = Canvas::text_width(tag, 1) + 2 * tag.len().max(1) + 20;
    canvas.round_rect(Rect::new(left, y, pill_width, 22), 5, kind_color, 30);
    canvas.border(Rect::new(left, y, pill_width, 22), kind_color, 110);
    canvas.text_tracked(left + 10, y + 4, tag, kind_color, 1, 2, 255);

    let mut directory_caption = [0u8; 48];
    let size_caption = FormatBytes::new(entry.size as u64);
    let caption: &str = if entry.is_dir {
        describe_directory(&mut directory_caption, entry.entry_count())
    } else if entry.size == 0 {
        ""
    } else {
        size_caption.as_str()
    };
    if !caption.is_empty() {
        canvas.text_right(right, y + 4, caption, theme::TEXT_DIM, 1, 255);
    }
    y += 40;

    row(canvas, left, right, y, "TYPE", type_name(entry.kind));
    y += 24;
    row(
        canvas,
        left,
        right,
        y,
        "VERSIONS",
        FormatUptime::new(entry.versions as u64).as_str(),
    );
    y += 24;
    row(canvas, left, right, y, "EDGES", flag_text(entry.flags));
    y += 34;

    // Fingerprint: the first 32 bytes of the content hash as a 8x8 nibble grid.
    canvas.text_tracked(left, y, "SHA-256", theme::TEXT_FAINT, 1, 2, 255);
    y += 22;
    fingerprint(canvas, left, y, &entry.id, kind_color);
    y += 8 * 13 + 6;
    let mut hex = [0u8; 64];
    node::format_id(&entry.id, &mut hex);
    let short = core::str::from_utf8(&hex[..16]).unwrap_or("");
    canvas.text_tracked(left, y, short, theme::AMBER, 1, 1, 255);
    y += 30;

    // Directory entries, node properties, or the head of a file's content.
    let record = node::get(&entry.id);
    let has_props = record.map(|node| !node.props().is_empty()).unwrap_or(false);
    let section = if entry.is_dir {
        "CHILDREN"
    } else if has_props {
        "PROPERTIES"
    } else {
        "CONTENT"
    };
    canvas.text_tracked(left, y, section, theme::TEXT_FAINT, 1, 2, 255);
    y += 22;
    let columns = (right - left) / 8;
    match record {
        Some(node) if node.is_directory() => {
            if node.edges().is_empty() {
                canvas.text(left, y, "(empty)", theme::TEXT_FAINT, 1, 255);
            } else {
                for edge in node.edges().iter().take(5) {
                    let color = match node::get(&edge.target) {
                        Some(child) => theme::kind_color(child.kind),
                        None => theme::ROSE,
                    };
                    canvas.text_clipped(left, y, edge.label(), columns, color, 1, 255);
                    y += 18;
                }
            }
        }
        Some(node) if !node.props().is_empty() => {
            // Nodes without payload but with properties (users, capabilities).
            for prop in node.props().iter().take(4) {
                let int_value;
                let value: &str = match prop.kind {
                    crate::vfs::node::PropKind::Int => {
                        int_value = FormatInt::new(prop.number);
                        int_value.as_str()
                    }
                    crate::vfs::node::PropKind::Bool => {
                        if prop.number != 0 {
                            "true"
                        } else {
                            "false"
                        }
                    }
                    crate::vfs::node::PropKind::Text => prop.text_value(),
                };
                row(canvas, left, right, y, prop.key(), value);
                y += 20;
            }
        }
        Some(node) => {
            let text = core::str::from_utf8(node.data()).unwrap_or("");
            let mut drawn = 0usize;
            for line in text.split('\n') {
                if drawn == 5 {
                    break;
                }
                let line = line.trim_end();
                if line.is_empty() {
                    continue;
                }
                canvas.text_clipped(left, y, line, columns, theme::TEXT_DIM, 1, 255);
                y += 18;
                drawn += 1;
            }
            if drawn == 0 {
                canvas.text(left, y, "(empty)", theme::TEXT_FAINT, 1, 255);
            }
        }
        None => {
            canvas.text(left, y, "(dangling node)", theme::ROSE, 1, 255);
        }
    }

    activity(canvas, panel, app, ticks);
    canvas.reset_clip();
}

fn activity(canvas: &mut Canvas, panel: Rect, app: &App, ticks: u64) {
    let left = panel.x + INSPECTOR_PADDING;
    let right = panel.right() - INSPECTOR_PADDING;
    let lines = app.activity_count.min(ACTIVITY_LINES);
    let block_height = 24 + lines * 18;
    let top = panel.bottom().saturating_sub(20 + block_height);

    canvas.hline(top, left, right, theme::HAIRLINE, 255);
    canvas.text_tracked(left, top + 10, "ACTIVITY", theme::AMBER, 1, 2, 255);

    let mut y = top + 32;
    let start = app.activity_next + ACTIVITY_LINES - lines;
    for offset in 0..lines {
        let line = &app.activity[(start + offset) % ACTIVITY_LINES];
        let text = line.text();
        let columns = (right - left) / 8;
        let color = if offset + 1 == lines {
            line.color
        } else {
            theme::TEXT_FAINT
        };
        canvas.text_clipped(left, y, text, columns, color, 1, 255);
        y += 18;
    }

    // A slow pulse in the corner shows the desktop is live.
    let glow = 120 + ((ticks / 8) % 8) as u8 * 12;
    canvas.disc((right - 4) as i64, (panel.y + 30) as i64, 3, theme::ACCENT, glow);
}

pub fn footer(canvas: &mut Canvas, layout: &Layout, app: &App, ticks: u64) {
    let area = layout.footer;
    canvas.set_clip(area);
    canvas.fill(area.x, area.y, area.w, area.h, theme::PANEL_RAISED);
    canvas.hline(area.y, area.x, area.right(), theme::HAIRLINE, 255);

    let text_y = area.y + 24;
    canvas.text(36, text_y, ">", theme::ACCENT, 1, 255);
    let typed = app.command_str();
    let advance = canvas.text(52, text_y, typed, theme::TEXT, 1, 255);

    // Blinking caret.
    if (ticks / 40) % 2 == 0 {
        canvas.fill(52 + advance + 1, text_y, 8, 16, theme::ACCENT);
    }

    let hints = if app.command_len > 0 {
        "ENTER RUN   ESC CLEAR"
    } else {
        "UP/DOWN SELECT   ENTER OPEN   LEFT UP   ESC TERMINAL"
    };
    canvas.text_tracked(
        area.right().saturating_sub(36 + Canvas::text_width(hints, 1) + 2 * hints.len().max(1)),
        text_y,
        hints,
        theme::TEXT_FAINT,
        1,
        2,
        255,
    );
    canvas.reset_clip();
}

pub fn toast(canvas: &mut Canvas, stage: Rect, text: &str, ticks: u64) {
    if text.is_empty() {
        return;
    }
    let width = (Canvas::text_width(text, 1) + 2 * text.len().max(1) + 44).min(stage.w - 40);
    let height = 36;
    let x = stage.x + (stage.w - width) / 2;
    let y = stage.bottom() - height - 30;
    let rect = Rect::new(x, y, width, height);

    let _ = ticks;
    canvas.round_rect(rect, 8, theme::PANEL_RAISED, 240);
    canvas.border(rect, theme::ACCENT, 90);
    canvas.fill(rect.x + 10, rect.y + 10, 3, rect.h - 20, theme::ACCENT);
    canvas.text_clipped(
        rect.x + 24,
        rect.y + 10,
        text,
        (width.saturating_sub(34)) / 8,
        theme::TEXT,
        1,
        255,
    );
}

/// Boot splash: the mark, the wordmark and a progress bar.
pub fn splash(canvas: &mut Canvas, fb: &mut crate::console::framebuffer::Framebuffer, progress: usize, total: usize) {
    canvas.set_clip(canvas.bounds());
    canvas.vertical_gradient(theme::BG_TOP, theme::BG_BOTTOM);

    let width = canvas.width();
    let height = canvas.height();
    let center_x = width / 2;
    let center_y = height / 2 - 30;

    canvas.glow(center_x as i64, center_y as i64, 150, theme::ACCENT, 40);
    logo(canvas, center_x - 34, center_y - 44, 56, theme::ACCENT);

    let wordmark = "VERTEX";
    let width_px = Canvas::text_width(wordmark, 3) + 6 * (wordmark.len() - 1);
    canvas.text_tracked(
        center_x - width_px / 2,
        center_y + 30,
        wordmark,
        theme::TEXT,
        3,
        6,
        255,
    );
    let tagline = "LOOM";
    let tag_width = Canvas::text_width(tagline, 1) + 3 * (tagline.len() - 1);
    canvas.text_tracked(
        center_x - tag_width / 2,
        center_y + 84,
        tagline,
        theme::ACCENT,
        1,
        3,
        255,
    );

    let bar_width = 300usize;
    let bar = Rect::new(center_x - bar_width / 2, center_y + 128, bar_width, 3);
    canvas.fill(bar.x, bar.y, bar.w, bar.h, theme::HAIRLINE);
    let filled = bar_width * progress / total.max(1);
    canvas.fill(bar.x, bar.y, filled, bar.h, theme::ACCENT);

    canvas.text_centered(
        Rect::new(0, bar.y + 18, width, 20),
        bar.y + 20,
        "initialising node graph",
        theme::TEXT_FAINT,
        1,
        255,
    );

    canvas.present(fb);
}

fn row(canvas: &mut Canvas, left: usize, right: usize, y: usize, label: &str, value: &str) {
    canvas.text_tracked(left, y, label, theme::TEXT_FAINT, 1, 2, 255);
    canvas.text_right(right, y, value, theme::TEXT, 1, 255);
}

fn fingerprint(canvas: &mut Canvas, x: usize, y: usize, id: &[u8; 32], accent: u32) {
    let cell = 10usize;
    let gap = 3usize;
    for index in 0..64usize {
        let byte = id[index / 2];
        let nibble = if index % 2 == 0 { byte >> 4 } else { byte & 0x0F };
        let strength = (nibble as u32 * 15 + 12) as u8;
        let color = mix(theme::PANEL_RAISED, accent, strength);
        let cx = x + (index % 8) * (cell + gap);
        let cy = y + (index / 8) * (cell + gap);
        canvas.fill(cx, cy, cell, cell, color);
    }
}

fn type_name(kind: crate::vfs::NodeType) -> &'static str {
    use crate::vfs::NodeType::*;
    match kind {
        Directory => "directory",
        Text => "text",
        Document => "document",
        Blob => "blob",
        User => "user",
        Process => "process",
        Capability => "capability",
        Schema => "schema",
        Custom => "custom",
    }
}

fn flag_text(flags: u32) -> &'static str {
    match flags & 0x0007 {
        0x0001 => "strong",
        0x0002 => "ordered",
        0x0004 => "unique",
        0x0005 => "strong+unique",
        0x0003 => "strong+ordered",
        0x0006 => "ordered+unique",
        0x0007 => "all",
        _ => "weak",
    }
}

/// Signed integers without allocating.
struct FormatInt {
    buffer: [u8; 24],
    len: usize,
}

impl FormatInt {
    fn new(value: i64) -> Self {
        let mut buffer = [0u8; 24];
        let negative = value < 0;
        let mut magnitude = value.unsigned_abs();
        let mut digits = [0u8; 20];
        let mut count = 0;
        if magnitude == 0 {
            digits[0] = b'0';
            count = 1;
        }
        while magnitude > 0 && count < 20 {
            digits[count] = b'0' + (magnitude % 10) as u8;
            magnitude /= 10;
            count += 1;
        }
        let mut len = 0;
        if negative {
            buffer[len] = b'-';
            len += 1;
        }
        while count > 0 {
            count -= 1;
            buffer[len] = digits[count];
            len += 1;
        }
        Self { buffer, len }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buffer[..self.len]).unwrap_or("?")
    }
}

/// Seconds with a unit, for the header.
struct FormatSeconds {
    buffer: [u8; 16],
    len: usize,
}

impl FormatSeconds {
    fn new(seconds: u64) -> Self {
        let digits = FormatUptime::new(seconds.min(999_999));
        let mut buffer = [0u8; 16];
        let text = digits.as_str().as_bytes();
        let length = text.len().min(buffer.len() - 1);
        buffer[..length].copy_from_slice(&text[..length]);
        buffer[length] = b's';
        Self {
            buffer,
            len: length + 1,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buffer[..self.len]).unwrap_or("?")
    }
}

fn describe_directory(buffer: &mut [u8; 48], count: usize) -> &str {
    // "N entries" without any allocation.
    let mut number = [0u8; 6];
    let mut value = count;
    let mut index = 6;
    if value == 0 {
        index -= 1;
        number[index] = b'0';
    }
    while value > 0 && index > 0 {
        index -= 1;
        number[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    let digits = &number[index..];

    let mut length = 0;
    for &byte in digits {
        if length < buffer.len() {
            buffer[length] = byte;
            length += 1;
        }
    }
    let suffix: &[u8] = if count == 1 { b" entry" } else { b" entries" };
    for &byte in suffix {
        if length < buffer.len() {
            buffer[length] = byte;
            length += 1;
        }
    }
    core::str::from_utf8(&buffer[..length]).unwrap_or("?")
}

/// Render an integer as text without allocating.
struct FormatUptime {
    buffer: [u8; 12],
    len: usize,
}

impl FormatUptime {
    fn new(value: u64) -> Self {
        let mut buffer = [0u8; 12];
        let mut len = 0;
        let mut digits = [0u8; 10];
        let mut count = 0;
        let mut value = value;
        if value == 0 {
            digits[0] = b'0';
            count = 1;
        }
        while value > 0 && count < 10 {
            digits[count] = b'0' + (value % 10) as u8;
            value /= 10;
            count += 1;
        }
        while count > 0 {
            count -= 1;
            buffer[len] = digits[count];
            len += 1;
        }
        Self { buffer, len }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buffer[..self.len]).unwrap_or("?")
    }
}

/// Byte counts as `812 B`, `3.4K`, `1.2M`.
struct FormatBytes {
    buffer: [u8; 16],
    len: usize,
}

impl FormatBytes {
    fn new(bytes: u64) -> Self {
        let mut output = [0u8; 16];
        let mut len = 0usize;
        let (value, suffix) = if bytes >= 1024 * 1024 {
            (bytes * 10 / (1024 * 1024), "M")
        } else if bytes >= 1024 {
            (bytes * 10 / 1024, "K")
        } else {
            (bytes * 10, " B")
        };

        if value >= 10 {
            let whole = value / 10;
            let mut digits = [0u8; 20];
            let mut count = 0;
            let mut whole = whole;
            if whole == 0 {
                digits[0] = b'0';
                count = 1;
            }
            while whole > 0 && count < 20 {
                digits[count] = b'0' + (whole % 10) as u8;
                whole /= 10;
                count += 1;
            }
            while count > 0 {
                count -= 1;
                output[len] = digits[count];
                len += 1;
            }
            if suffix != " B" {
                output[len] = b'.';
                len += 1;
                output[len] = b'0' + (value % 10) as u8;
                len += 1;
            }
        } else {
            output[0] = b'0';
            output[1] = b'.';
            output[2] = b'0' + (value % 10) as u8;
            len = 3;
        }

        for &byte in suffix.as_bytes() {
            if len < output.len() {
                output[len] = byte;
                len += 1;
            }
        }
        Self {
            buffer: output,
            len,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buffer[..self.len]).unwrap_or("?")
    }
}

/// Exposed for the inspector's entry type column.
pub fn entry_kind_label(entry: &Entry) -> &'static str {
    theme::kind_label(entry.kind)
}
