//! The constellation: the node graph rendered as an orbiting diagram.
//!
//! This is the visual heart of the front end. The current directory sits in the
//! middle as a core, its entries orbit it at evenly spaced angles, and every
//! entry is drawn with a silhouette that encodes its node type:
//!
//! ```text
//!   ring      directory     disc      text       diamond   user
//!   triangle  process       square    other
//! ```
//!
//! Data flow is animated: small pulses travel from the core out to each entry,
//! and the selected entry carries a rotating arc. Everything is clipped to the
//! stage rectangle so labels never bleed into the inspector.

use crate::gui::canvas::{sin_cos, Canvas, Rect};
use crate::gui::theme;
use crate::gui::Entry;
use crate::vfs::NodeType;

/// What to draw in one frame.
pub struct Scene<'a> {
    pub entries: &'a [Entry],
    pub selected: usize,
    pub ticks: u64,
}

/// Radii of the core and of the orbiting entries.
struct Geometry {
    cx: i64,
    cy: i64,
    orbit: i64,
}

const NODE_RADIUS: i64 = 11;
const SELECTED_RADIUS: i64 = 15;

pub fn draw(canvas: &mut Canvas, stage: Rect, scene: &Scene) {
    canvas.set_clip(stage);
    canvas.dot_grid(28, theme::GRID, 22);

    let geometry = Geometry {
        cx: (stage.x + stage.w / 2) as i64,
        cy: (stage.y + stage.h / 2) as i64,
        orbit: ((stage.w.min(stage.h) * 31 / 100) as i64).clamp(96, 300),
    };

    draw_orbits(canvas, &geometry, scene.ticks);

    if scene.entries.is_empty() {
        draw_empty(canvas, stage, &geometry);
        canvas.reset_clip();
        return;
    }

    draw_edges(canvas, &geometry, scene);
    draw_core(canvas, &geometry, scene.entries.len(), scene.ticks);
    draw_nodes(canvas, stage, &geometry, scene);
    canvas.reset_clip();
}

/// Decorative orbit: a thin echo ring plus a rotating dashed survey ring.
fn draw_orbits(canvas: &mut Canvas, geometry: &Geometry, ticks: u64) {
    canvas.glow(geometry.cx, geometry.cy, geometry.orbit + 70, theme::ACCENT, 46);

    canvas.ring(geometry.cx, geometry.cy, geometry.orbit, 1, theme::HAIRLINE, 255);

    let outer = geometry.orbit + 52;
    let rotation = (ticks / 6) as i64;
    for dash in 0..24 {
        let start = rotation + dash * 15;
        canvas.arc(
            geometry.cx,
            geometry.cy,
            outer,
            start,
            start + 7,
            1,
            theme::ACCENT_DEEP,
            170,
        );
    }

    // Tick marks where entries sit, drawn a little outside the orbit.
    canvas.ring(
        geometry.cx,
        geometry.cy,
        geometry.orbit + 6,
        1,
        theme::HAIRLINE,
        120,
    );
}

/// Edges plus the travelling pulses that suggest data flow.
fn draw_edges(canvas: &mut Canvas, geometry: &Geometry, scene: &Scene) {
    let count = scene.entries.len() as i64;
    for (index, entry) in scene.entries.iter().enumerate() {
        let (nx, ny) = node_position(geometry, count, index as i64);
        let color = theme::kind_color(entry.kind);
        canvas.line(geometry.cx, geometry.cy, nx, ny, color, 70);

        // Pulse: phase advances with the tick counter and the entry index, so
        // the ring reads as a single slow wave rather than a strobe.
        let period = 150u64;
        let phase = ((scene.ticks + index as u64 * 19) % period) as i64;
        let px = geometry.cx + (nx - geometry.cx) * phase / period as i64;
        let py = geometry.cy + (ny - geometry.cy) * phase / period as i64;
        let brightness = if index == scene.selected { 230 } else { 110 };
        canvas.disc(px, py, 2, color, brightness);
    }
}

/// The centre: a disc showing how many entries the directory holds.
fn draw_core(canvas: &mut Canvas, geometry: &Geometry, entries: usize, ticks: u64) {
    canvas.glow(geometry.cx, geometry.cy, 110, theme::ACCENT, 70);
    canvas.disc(geometry.cx, geometry.cy, 52, theme::BG_BOTTOM, 200);
    canvas.disc(geometry.cx, geometry.cy, 44, theme::PANEL_RAISED, 255);
    canvas.ring(geometry.cx, geometry.cy, 44, 2, theme::ACCENT_DEEP, 255);

    let rotation = (ticks / 2) as i64;
    canvas.arc(
        geometry.cx,
        geometry.cy,
        49,
        rotation,
        rotation + 70,
        2,
        theme::ACCENT,
        220,
    );

    let count = FormatTwo::new(entries);
    let label = count.as_str();
    let width = Canvas::text_width(label, 2);
    canvas.text(
        (geometry.cx - width as i64 / 2) as usize,
        (geometry.cy - 27) as usize,
        label,
        theme::TEXT,
        2,
        255,
    );
    let tag = if entries == 1 { "ENTRY" } else { "ENTRIES" };
    let tag_width = Canvas::text_width(tag, 1) + 2 * (tag.len() - 1);
    canvas.text_tracked(
        (geometry.cx - tag_width as i64 / 2) as usize,
        (geometry.cy + 11) as usize,
        tag,
        theme::TEXT_FAINT,
        1,
        2,
        255,
    );
}

fn draw_nodes(canvas: &mut Canvas, stage: Rect, geometry: &Geometry, scene: &Scene) {
    let count = scene.entries.len() as i64;
    let lower = geometry.cy + geometry.orbit / 2;
    let stage_top = stage.y as i64;
    let stage_bottom = stage.bottom();

    for (index, entry) in scene.entries.iter().enumerate() {
        let (nx, ny) = node_position(geometry, count, index as i64);
        let selected = index == scene.selected;
        let radius = if selected {
            SELECTED_RADIUS
        } else {
            NODE_RADIUS
        };
        let color = theme::kind_color(entry.kind);

        if selected {
            canvas.glow(nx, ny, 52, color, 80);
            canvas.ring(nx, ny, radius + 8, 1, color, 210);
            let rotation = (scene.ticks + index as u64 * 40) as i64;
            canvas.arc(nx, ny, radius + 13, rotation, rotation + 90, 2, theme::ACCENT, 240);
        }

        // A dark body keeps the edge line from running through the silhouette.
        canvas.disc(nx, ny, radius - 3, theme::BG_BOTTOM, 255);
        draw_silhouette(canvas, entry.kind, nx, ny, radius, color, selected);

        let label = entry.name();
        let columns = 18.min((scene_width(geometry) / 8) as usize).max(6);
        let text_color = if selected {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };
        let above = ny < lower;
        let desired = if above {
            ny - radius - 26
        } else {
            ny + radius + 12
        };
        // Keep labels inside the stage: the orbit can approach the edge.
        let label_y = desired.max(stage_top + 4) as usize;
        let label_y = label_y.min(stage_bottom.saturating_sub(24));

        let clipped = Canvas::clip_text(label, columns);
        let width = Canvas::text_width(clipped, 1);
        let x = (nx - width as i64 / 2).max(0) as usize;
        canvas.text(x, label_y, clipped, text_color, 1, 255);

        if selected {
            // Underline the selection so it reads even when the arc is hidden.
            canvas.hline(label_y + 19, x, x + width, theme::ACCENT, 200);
            let tag = theme::kind_label(entry.kind);
            let tag_width = Canvas::text_width(tag, 1) + tag.len() - 1;
            let tag_x = (nx - tag_width as i64 / 2).max(0) as usize;
            let tag_y = if above { label_y - 18 } else { label_y + 26 };
            canvas.text_tracked(tag_x, tag_y, tag, color, 1, 1, 255);
        }
    }
}

/// Node type silhouettes, all drawn inside a disc of `radius`.
fn draw_silhouette(
    canvas: &mut Canvas,
    kind: NodeType,
    cx: i64,
    cy: i64,
    radius: i64,
    color: u32,
    selected: bool,
) {
    let alpha = if selected { 255 } else { 210 };
    match kind {
        NodeType::Directory => {
            canvas.disc(cx, cy, radius - 4, theme::PANEL, 255);
            canvas.ring(cx, cy, radius - 4, 2, color, alpha);
            // A hole tells you it contains other nodes.
            canvas.disc(cx, cy, radius / 3, color, 90);
        }
        NodeType::Text | NodeType::Document => {
            canvas.disc(cx, cy, radius - 2, color, alpha);
            canvas.disc(cx, cy, radius - 6, theme::BG_BOTTOM, 150);
        }
        NodeType::User => diamond(canvas, cx, cy, radius - 1, color, alpha),
        NodeType::Process => triangle(canvas, cx, cy, radius - 1, color, alpha),
        _ => {
            let side = radius - 3;
            canvas.fill(
                (cx - side) as usize,
                (cy - side) as usize,
                (side * 2) as usize,
                (side * 2) as usize,
                color,
            );
        }
    }
}

fn diamond(canvas: &mut Canvas, cx: i64, cy: i64, radius: i64, color: u32, alpha: u8) {
    for dy in -radius..=radius {
        let half = radius - dy.abs();
        let y = (cy + dy) as usize;
        canvas.hline(y, (cx - half) as usize, (cx + half + 1) as usize, color, alpha);
    }
}

fn triangle(canvas: &mut Canvas, cx: i64, cy: i64, radius: i64, color: u32, alpha: u8) {
    for dy in -radius..=radius {
        let half = (dy + radius) / 2;
        let y = (cy + dy) as usize;
        canvas.hline(y, (cx - half) as usize, (cx + half + 1) as usize, color, alpha);
    }
}

fn draw_empty(canvas: &mut Canvas, stage: Rect, geometry: &Geometry) {
    draw_core(canvas, geometry, 0, 0);
    canvas.text_centered(stage, (geometry.cy + 78) as usize, "EMPTY DIRECTORY", theme::TEXT_DIM, 1, 255);
    canvas.text_centered(
        stage,
        (geometry.cy + 104) as usize,
        "type: mkdir name   to add a node",
        theme::TEXT_FAINT,
        1,
        255,
    );
}

fn scene_width(geometry: &Geometry) -> i64 {
    geometry.orbit * 2
}

fn node_position(geometry: &Geometry, count: i64, index: i64) -> (i64, i64) {
    let angle = -90 + index * 360 / count.max(1);
    let (sin, cos) = sin_cos(angle);
    (
        geometry.cx + geometry.orbit * cos / 10_000,
        geometry.cy + geometry.orbit * sin / 10_000,
    )
}

/// Two-digit entry count without pulling in the allocation machinery.
struct FormatTwo {
    digits: [u8; 3],
    len: usize,
}

impl FormatTwo {
    fn new(value: usize) -> Self {
        if value >= 100 {
            return Self {
                digits: [b'+', b'+', 0],
                len: 2,
            };
        }
        if value >= 10 {
            Self {
                digits: [b'0' + (value / 10) as u8, b'0' + (value % 10) as u8, 0],
                len: 2,
            }
        } else {
            Self {
                digits: [b'0' + value as u8, 0, 0],
                len: 1,
            }
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.digits[..self.len]).unwrap_or("?")
    }
}
