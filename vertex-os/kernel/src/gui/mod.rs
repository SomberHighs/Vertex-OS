//! The Loom: Vertex OS's graphical front end.
//!
//! The Loom is a keyboard-driven desktop for the node graph. The centre of the
//! screen shows the current directory as a constellation, the right column is
//! an inspector for the selected node, and the footer is both a command bar for
//! the real shell and a status line.
//!
//! Design notes:
//!   * the front end owns the framebuffer while it runs, so console output goes
//!     only to the serial port (which is what the boot test reads);
//!   * the whole frame is composed in a heap-backed canvas and presented in one
//!     blit, so nothing tears;
//!   * drawing uses no floating point, matching the rest of the kernel.

pub mod canvas;
pub mod loom;
pub mod panels;
pub mod theme;

use crate::arch::x86_64::cpu;
use crate::arch::x86_64::keyboard::{self, Key};
use crate::arch::x86_64::{serial, timer};
use crate::console::framebuffer::Framebuffer;
use crate::console::{self, FG_ERROR, FG_WARN};
use crate::gui::canvas::Canvas;
use crate::gui::theme as palette;
use crate::{kprintln, kprintln_color};
use crate::shell;
use crate::vfs::{self, node, NodeId, NodeType};

/// Smallest screen the layout is designed for.
const MIN_WIDTH: usize = 860;
const MIN_HEIGHT: usize = 560;
const MAX_ENTRIES: usize = 16;
const PATH_MAX: usize = 160;
const COMMAND_MAX: usize = 128;
const ACTIVITY_COLS: usize = 96;
/// Activity lines kept in the ring buffer.
pub const ACTIVITY_LINES: usize = 5;
/// Frames between animation updates (100 Hz timer).
const ANIMATION_INTERVAL: u64 = 6;

/// One entry of the directory being displayed.
#[derive(Clone, Copy)]
pub struct Entry {
    name: [u8; vfs::MAX_NAME],
    name_len: usize,
    pub id: NodeId,
    pub kind: NodeType,
    pub is_dir: bool,
    pub size: usize,
    pub versions: usize,
    pub flags: u32,
}

impl Entry {
    fn empty() -> Self {
        Self {
            name: [0; vfs::MAX_NAME],
            name_len: 0,
            id: vfs::ZERO_ID,
            kind: NodeType::Blob,
            is_dir: false,
            size: 0,
            versions: 0,
            flags: 0,
        }
    }

    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }

    pub fn entry_count(&self) -> usize {
        node::get(&self.id).map(|n| n.edges().len()).unwrap_or(0)
    }
}

/// A single line in the activity panel.
#[derive(Clone, Copy)]
pub struct ActivityLine {
    text: [u8; ACTIVITY_COLS],
    len: usize,
    pub color: u32,
}

impl ActivityLine {
    fn empty() -> Self {
        Self {
            text: [0; ACTIVITY_COLS],
            len: 0,
            color: palette::TEXT_FAINT,
        }
    }

    pub fn text(&self) -> &str {
        core::str::from_utf8(&self.text[..self.len]).unwrap_or("")
    }
}

/// Whether the main loop keeps going.
#[derive(PartialEq, Eq)]
enum Flow {
    Continue,
    Exit,
}

/// Desktop state.
pub struct App {
    cwd: [u8; PATH_MAX],
    pub cwd_len: usize,
    entries: [Entry; MAX_ENTRIES],
    pub entry_count: usize,
    pub selected: usize,
    command: [u8; COMMAND_MAX],
    pub command_len: usize,
    pub activity: [ActivityLine; ACTIVITY_LINES],
    pub activity_count: usize,
    pub activity_next: usize,
    toast: [u8; 96],
    toast_len: usize,
    toast_until: u64,
    serial_state: u8,
    serial_escape_at: u64,
    dirty: bool,
    last_frame: u64,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            cwd: [0; PATH_MAX],
            cwd_len: 1,
            entries: [Entry::empty(); MAX_ENTRIES],
            entry_count: 0,
            selected: 0,
            command: [0; COMMAND_MAX],
            command_len: 0,
            activity: [ActivityLine::empty(); ACTIVITY_LINES],
            activity_count: 0,
            activity_next: 0,
            toast: [0; 96],
            toast_len: 0,
            toast_until: 0,
            serial_state: 0,
            serial_escape_at: 0,
            dirty: true,
            last_frame: 0,
        };
        app.cwd[0] = b'/';
        app.reload();
        app.push_activity("loom: desktop online", palette::ACCENT);
        app.push_activity("arrows move, enter opens, type to run a command", palette::TEXT_FAINT);
        app
    }

    pub fn cwd_str(&self) -> &str {
        core::str::from_utf8(&self.cwd[..self.cwd_len]).unwrap_or("/")
    }

    pub fn command_str(&self) -> &str {
        core::str::from_utf8(&self.command[..self.command_len]).unwrap_or("")
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries[..self.entry_count]
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        if self.selected < self.entry_count {
            self.entries.get(self.selected)
        } else {
            None
        }
    }

    // --- directory state ---------------------------------------------------

    /// Read the current directory's entries out of the graph.
    fn reload(&mut self) {
        self.entry_count = 0;
        self.selected = 0;

        let mut path = [0u8; PATH_MAX];
        let length = self.cwd_len;
        path[..length].copy_from_slice(&self.cwd[..length]);
        let Ok(text) = core::str::from_utf8(&path[..length]) else {
            return;
        };
        let Ok(directory) = vfs::resolve(text) else {
            return;
        };
        let Some(node) = node::get(&directory) else {
            return;
        };

        for edge in node.edges() {
            if self.entry_count == MAX_ENTRIES {
                break;
            }
            let Some(child) = node::get(&edge.target) else {
                continue;
            };
            let entry = &mut self.entries[self.entry_count];
            let label = edge.label();
            let bytes = label.as_bytes();
            entry.name_len = bytes.len().min(vfs::MAX_NAME);
            entry.name[..entry.name_len].copy_from_slice(&bytes[..entry.name_len]);
            entry.id = edge.target;
            entry.kind = child.kind;
            entry.is_dir = child.is_directory();
            entry.size = child.data().len();
            entry.flags = edge.flags;
            entry.versions = vfs::version_count(edge.target);
            self.entry_count += 1;
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.entry_count == 0 {
            return;
        }
        let count = self.entry_count as i32;
        let mut index = self.selected as i32 + delta;
        if index < 0 {
            index = count - 1;
        }
        if index >= count {
            index = 0;
        }
        self.selected = index as usize;
    }

    fn activate(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let mut name = [0u8; vfs::MAX_NAME];
        let name_len = entry.name_len;
        name[..name_len].copy_from_slice(&entry.name[..name_len]);
        let is_dir = entry.is_dir;

        let name = core::str::from_utf8(&name[..name_len]).unwrap_or("");
        let mut target = [0u8; PATH_MAX];
        let Some(length) = self.join(self.cwd_str(), name, &mut target) else {
            self.toast("path too deep");
            return;
        };
        let path = core::str::from_utf8(&target[..length]).unwrap_or("");

        kprintln!("[loom] open {}", path);

        if is_dir {
            self.cwd_len = length;
            self.cwd[..length].copy_from_slice(&target[..length]);
            self.reload();
            self.push_activity(path, palette::BLUE);
            self.toast("entered directory");
        } else {
            // Opening a file prints its head into the activity panel.
            self.push_activity(path, palette::ACCENT);
            if let Some(entry) = self.selected_entry() {
                if let Some(node) = node::get(&entry.id) {
                    let text = core::str::from_utf8(node.data()).unwrap_or("");
                    if let Some(first) = text.lines().next() {
                        self.push_activity(first, palette::TEXT_DIM);
                    }
                }
            }
            self.toast("opened file");
        }
        self.dirty = true;
    }

    fn go_parent(&mut self) {
        if self.cwd_len <= 1 {
            self.toast("already at the root node");
            return;
        }
        let mut length = self.cwd_len;
        while length > 0 && self.cwd[length - 1] != b'/' {
            length -= 1;
        }
        if length > 0 {
            length -= 1;
        }
        if length == 0 {
            length = 1;
            self.cwd[0] = b'/';
        }
        self.cwd_len = length;
        let path = self.cwd_str();
        kprintln!("[loom] cd {}", path);
        self.reload();
        self.toast("up one level");
        self.dirty = true;
    }

    /// Normalise `base` + `target` into an absolute path.
    fn join(&self, base: &str, target: &str, out: &mut [u8; PATH_MAX]) -> Option<usize> {
        let mut segments: [&str; vfs::MAX_PATH_SEGMENTS] = [""; vfs::MAX_PATH_SEGMENTS];
        let mut depth = 0usize;
        let start: &str = if target.starts_with('/') { "" } else { base };

        for segment in start.split('/').chain(target.split('/')) {
            match segment {
                "" | "." => continue,
                ".." => depth = depth.saturating_sub(1),
                name => {
                    if depth == vfs::MAX_PATH_SEGMENTS {
                        return None;
                    }
                    segments[depth] = name;
                    depth += 1;
                }
            }
        }

        if depth == 0 {
            out[0] = b'/';
            return Some(1);
        }
        let mut length = 0usize;
        for name in &segments[..depth] {
            if length + name.len() + 1 >= PATH_MAX {
                return None;
            }
            out[length] = b'/';
            length += 1;
            out[length..length + name.len()].copy_from_slice(name.as_bytes());
            length += name.len();
        }
        Some(length)
    }

    fn change_directory(&mut self, target: &str) {
        let mut path = [0u8; PATH_MAX];
        let Some(length) = self.join(self.cwd_str(), target, &mut path) else {
            self.toast("path too deep");
            return;
        };
        let text = core::str::from_utf8(&path[..length]).unwrap_or("/");
        match vfs::resolve(text) {
            Ok(id) => match node::get(&id) {
                Some(node) if node.is_directory() => {
                    self.cwd_len = length;
                    self.cwd[..length].copy_from_slice(&path[..length]);
                    kprintln!("[loom] cd {}", text);
                    self.reload();
                    self.toast("changed directory");
                }
                Some(_) => self.toast("not a directory"),
                None => self.toast("dangling node"),
            },
            Err(_) => self.toast("no such node"),
        }
        self.dirty = true;
    }

    // --- activity ----------------------------------------------------------

    fn push_activity(&mut self, text: &str, color: u32) {
        let line = &mut self.activity[self.activity_next];
        let bytes = text.as_bytes();
        line.len = bytes.len().min(ACTIVITY_COLS);
        line.text[..line.len].copy_from_slice(&bytes[..line.len]);
        line.color = color;
        self.activity_next = (self.activity_next + 1) % ACTIVITY_LINES;
        if self.activity_count < ACTIVITY_LINES {
            self.activity_count += 1;
        }
    }

    fn push_output(&mut self, output: &str) {
        let mut pushed = 0usize;
        for line in output.split('\n') {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            let color = if line.contains("unknown command") || line.starts_with("error") {
                palette::ROSE
            } else {
                palette::TEXT_DIM
            };
            self.push_activity(line, color);
            pushed += 1;
        }
        if pushed == 0 {
            self.push_activity("(no output)", palette::TEXT_FAINT);
        }
    }

    fn toast(&mut self, text: &str) {
        let bytes = text.as_bytes();
        self.toast_len = bytes.len().min(self.toast.len());
        self.toast[..self.toast_len].copy_from_slice(&bytes[..self.toast_len]);
        self.toast_until = timer::ticks() + 250;
    }

    fn toast_text(&self) -> &str {
        core::str::from_utf8(&self.toast[..self.toast_len]).unwrap_or("")
    }

    // --- command bar -------------------------------------------------------

    fn run_command(&mut self) {
        let mut line = [0u8; COMMAND_MAX];
        let length = self.command_len;
        line[..length].copy_from_slice(&self.command[..length]);
        self.command_len = 0;
        self.dirty = true;

        let Ok(text) = core::str::from_utf8(&line[..length]) else {
            return;
        };
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        kprintln!("[loom] run: {}", text);
        self.push_activity(text, palette::ACCENT);

        if let Some(target) = text.strip_prefix("cd ") {
            self.change_directory(target.trim());
            return;
        }
        if text == "cd" {
            self.change_directory("/");
            return;
        }

        // Everything else is the real shell: capture its console output while it
        // runs and replay it into the activity panel.
        console::begin_capture();
        shell::execute_line(self.cwd_str(), text);
        // Read before ending: `end_capture` discards the buffer.
        let mut buffer = [0u8; 2048];
        let captured = console::capture_to(&mut buffer);
        console::end_capture();
        let output = core::str::from_utf8(&buffer[..captured]).unwrap_or("");
        self.push_output(output);

        // The graph may have changed underneath us.
        self.reload();
        self.toast("command finished");
    }

    // --- input -------------------------------------------------------------

    fn poll_key(&mut self) -> Option<Key> {
        if let Some(key) = keyboard::pop_key() {
            return Some(key);
        }

        // Serial input, with a tiny escape-sequence parser so the boot test can
        // drive the GUI over the serial port as well as the PS/2 keyboard.
        match self.serial_state {
            0 => match serial::try_read_byte()? {
                0x1B => {
                    self.serial_state = 1;
                    self.serial_escape_at = timer::ticks();
                    None
                }
                b'\r' | b'\n' => Some(Key::Enter),
                0x08 | 0x7F => Some(Key::Backspace),
                b'\t' => Some(Key::Tab),
                byte @ 0x20..=0x7E => Some(Key::Char(byte)),
                _ => None,
            },
            1 => match serial::try_read_byte() {
                Some(b'[') => {
                    self.serial_state = 2;
                    None
                }
                Some(_) => {
                    self.serial_state = 0;
                    None
                }
                None => {
                    if timer::ticks().wrapping_sub(self.serial_escape_at) > 2 {
                        self.serial_state = 0;
                        Some(Key::Escape)
                    } else {
                        None
                    }
                }
            },
            _ => {
                let byte = serial::try_read_byte()?;
                self.serial_state = 0;
                match byte {
                    b'A' => Some(Key::Up),
                    b'B' => Some(Key::Down),
                    b'C' => Some(Key::Right),
                    b'D' => Some(Key::Left),
                    _ => None,
                }
            }
        }
    }

    fn handle_key(&mut self, key: Key) -> Flow {
        self.dirty = true;
        match key {
            Key::Up => self.move_selection(-1),
            Key::Down => self.move_selection(1),
            Key::Right => self.activate(),
            Key::Left => self.go_parent(),
            Key::Enter => {
                if self.command_len > 0 {
                    self.run_command();
                } else {
                    self.activate();
                }
            }
            Key::Escape => {
                if self.command_len > 0 {
                    self.command_len = 0;
                    self.toast("command cleared");
                } else {
                    return Flow::Exit;
                }
            }
            Key::Backspace => {
                if self.command_len > 0 {
                    self.command_len -= 1;
                }
            }
            Key::Tab => {
                // Reserved for completion; a no-op keeps focus predictable.
            }
            Key::Char(byte) => {
                if byte >= 0x20 && self.command_len < COMMAND_MAX {
                    self.command[self.command_len] = byte;
                    self.command_len += 1;
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    // --- frame -------------------------------------------------------------

    fn render(&mut self, canvas: &mut Canvas, fb: &mut Framebuffer, ticks: u64) {
        canvas.reset_clip();
        canvas.vertical_gradient(palette::BG_TOP, palette::BG_BOTTOM);

        let layout = panels::layout(canvas.width(), canvas.height());
        let scene = loom::Scene {
            entries: &self.entries[..self.entry_count],
            selected: self.selected,
            ticks,
        };
        loom::draw(canvas, layout.stage, &scene);
        panels::breadcrumb(canvas, layout.stage, self.cwd_str());
        panels::header(
            canvas,
            &layout,
            node::node_count(),
            node::data_bytes(),
            timer::uptime_secs(),
        );
        panels::inspector(canvas, &layout, self, ticks);
        panels::footer(canvas, &layout, self, ticks);
        if ticks < self.toast_until {
            panels::toast(canvas, layout.stage, self.toast_text(), ticks);
        }
        canvas.present(fb);
    }

    fn run(&mut self, canvas: &mut Canvas, fb: &mut Framebuffer) {
        loop {
            let mut acted = false;
            while let Some(key) = self.poll_key() {
                acted = true;
                if self.handle_key(key) == Flow::Exit {
                    return;
                }
            }

            let now = timer::ticks();
            let animate = now.wrapping_sub(self.last_frame) >= ANIMATION_INTERVAL;
            if acted || animate || self.dirty {
                self.render(canvas, fb, now);
                self.last_frame = now;
                self.dirty = false;
            } else {
                cpu::hlt();
            }
        }
    }
}

/// Start the graphical front end.
///
/// Returns when the user leaves it with Escape, or immediately when there is no
/// framebuffer to take over (or one too small for the layout).
pub fn run() {
    let Some(framebuffer) = console::detach_framebuffer() else {
        return;
    };
    let (width, height) = framebuffer.dimensions();
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        console::attach_framebuffer(framebuffer);
        kprintln_color!(
            FG_WARN,
            "[loom] {}x{} is below the {}x{} the desktop needs",
            width,
            height,
            MIN_WIDTH,
            MIN_HEIGHT
        );
        return;
    }

    let Some(mut canvas) = Canvas::new(width, height) else {
        console::attach_framebuffer(framebuffer);
        kprintln_color!(FG_ERROR, "[loom] no memory for a {}x{} canvas", width, height);
        return;
    };

    kprintln!("[loom] graphical front end active: {}x{}", width, height);

    let mut framebuffer = framebuffer;
    let frames = 22usize;
    for frame in 0..=frames {
        panels::splash(&mut canvas, &mut framebuffer, frame, frames);
        timer::sleep_ms(40);
    }

    let mut app = App::new();
    kprintln!("[loom] constellation: {} entries under {}", app.entry_count, app.cwd_str());
    kprintln!("loom ready");

    app.run(&mut canvas, &mut framebuffer);

    kprintln!("[loom] graphical session ended, returning the screen to the text console");
    console::attach_framebuffer(framebuffer);
}
