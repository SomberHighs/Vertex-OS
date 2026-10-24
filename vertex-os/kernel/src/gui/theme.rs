//! Palette and typography for the Loom front end.
//!
//! The scheme is a cold, plotted-graph look: near-black navy surfaces, a mint
//! accent for interactive elements, amber for identifiers and dim slate for
//! supporting text. Every colour is `0x00RRGGBB`.

use crate::vfs::NodeType;

/// Backdrop, top of the gradient.
pub const BG_TOP: u32 = 0x04060B;
/// Backdrop, bottom of the gradient.
pub const BG_BOTTOM: u32 = 0x0B0F18;
/// Panel surface.
pub const PANEL: u32 = 0x0D1219;
/// Slightly lighter surface for insets.
pub const PANEL_RAISED: u32 = 0x131923;
/// One pixel separators.
pub const HAIRLINE: u32 = 0x1E2836;
/// Plotting grid dots.
pub const GRID: u32 = 0x27313F;
/// Primary text.
pub const TEXT: u32 = 0xE8EFF9;
/// Secondary text.
pub const TEXT_DIM: u32 = 0x8A97AA;
/// Labels and hints.
pub const TEXT_FAINT: u32 = 0x49556A;
/// Interactive accent.
pub const ACCENT: u32 = 0x4FE3C1;
/// Darker accent for edges and rings.
pub const ACCENT_DEEP: u32 = 0x1E6F60;
/// Directory nodes.
pub const BLUE: u32 = 0x6FA8FF;
/// Identifiers.
pub const AMBER: u32 = 0xF0C27A;
/// User nodes.
pub const VIOLET: u32 = 0xC0A0FF;
/// Errors and destructive actions.
pub const ROSE: u32 = 0xFF8CA0;
/// Success.
pub const GREEN: u32 = 0x8DE6A6;

/// Accent colour for a node type.
pub fn kind_color(kind: NodeType) -> u32 {
    match kind {
        NodeType::Directory => BLUE,
        NodeType::Text | NodeType::Document => ACCENT,
        NodeType::Blob => TEXT_DIM,
        NodeType::User => VIOLET,
        NodeType::Process => AMBER,
        NodeType::Capability => ROSE,
        NodeType::Schema => GREEN,
        NodeType::Custom => TEXT_DIM,
    }
}

/// Short uppercase tag for a node type.
pub fn kind_label(kind: NodeType) -> &'static str {
    match kind {
        NodeType::Directory => "DIR",
        NodeType::Text => "TEXT",
        NodeType::Document => "DOC",
        NodeType::Blob => "BLOB",
        NodeType::User => "USER",
        NodeType::Process => "PROC",
        NodeType::Capability => "CAP",
        NodeType::Schema => "SCHEMA",
        NodeType::Custom => "NODE",
    }
}
