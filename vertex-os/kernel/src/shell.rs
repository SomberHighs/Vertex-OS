//! The Vertex shell.
//!
//! Runs as a kernel task, reads from the PS/2 keyboard and the serial port, and
//! exposes the node graph. Kept deliberately small: no allocation on the input
//! path, fixed-size buffers only.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::x86_64::{cpu, keyboard, serial, timer};
use crate::console::{self, FG_ACCENT, FG_DIM, FG_ERROR, FG_MAGENTA, FG_OK};
use crate::{kprint, kprint_color, kprintln, kprintln_color};
use crate::sched;
use crate::vfs::node::{self, NodeId, NodeType};
use crate::vfs;

const LINE_MAX: usize = 256;
const PATH_MAX: usize = 160;
const MAX_ARGS: usize = 6;

static WORKER_TICKS: AtomicU64 = AtomicU64::new(0);

/// Start the desktop, then fall back to the text REPL.
///
/// The graphical front end runs first when the firmware gave us a framebuffer;
/// leaving it with Escape returns here, and `gui` enters it again.
pub extern "C" fn run() -> ! {
    if console::has_framebuffer() {
        crate::gui::run();
    }
    banner();
    let mut state = ShellState::new();
    loop {
        let mut line = [0u8; LINE_MAX];
        let length = state.read_line(&mut line);
        if length > 0 {
            state.execute(&line[..length]);
        }
    }
}

struct ShellState {
    cwd: [u8; PATH_MAX],
    cwd_len: usize,
}

impl ShellState {
    fn new() -> Self {
        let mut cwd = [0u8; PATH_MAX];
        cwd[0] = b'/';
        Self { cwd, cwd_len: 1 }
    }

    fn cwd(&self) -> &str {
        core::str::from_utf8(&self.cwd[..self.cwd_len]).unwrap_or("/")
    }

    /// Interactive prompt; returns the entered line without its newline.
    fn read_line(&mut self, buffer: &mut [u8; LINE_MAX]) -> usize {
        let mut length = 0usize;
        kprint_color!(FG_ACCENT, "vertex");
        kprint_color!(FG_DIM, ":{}", self.cwd());
        kprint_color!(FG_MAGENTA, "$ ");

        loop {
            let Some(byte) = poll_byte() else {
                cpu::hlt();
                continue;
            };

            match byte {
                b'\r' | b'\n' => {
                    kprintln!();
                    return length;
                }
                0x08 | 0x7F => {
                    if length > 0 {
                        length -= 1;
                        kprint!("\x08 \x08");
                    }
                }
                0x03 => {
                    // Ctrl-C: abandon the line.
                    kprintln!("^C");
                    return 0;
                }
                0x0C => {
                    console::clear();
                    kprint_color!(FG_ACCENT, "vertex");
                    kprint_color!(FG_DIM, ":{}", self.cwd());
                    kprint_color!(FG_MAGENTA, "$ ");
                    if let Ok(text) = core::str::from_utf8(&buffer[..length]) {
                        kprint!("{}", text);
                    }
                }
                0x15 => {
                    while length > 0 {
                        length -= 1;
                        kprint!("\x08 \x08");
                    }
                }
                0x20..=0x7E => {
                    if length < LINE_MAX - 1 {
                        buffer[length] = byte;
                        length += 1;
                        kprint!("{}", byte as char);
                    }
                }
                _ => {}
            }
        }
    }

    /// Resolve a possibly relative path into an absolute, normalised path.
    fn absolute(&self, argument: &str, out: &mut [u8; PATH_MAX]) -> Option<usize> {
        let base: &str = if argument.starts_with('/') || argument.is_empty() {
            ""
        } else {
            self.cwd()
        };

        let mut stack: [&str; vfs::MAX_PATH_SEGMENTS] = [""; vfs::MAX_PATH_SEGMENTS];
        let mut depth = 0usize;

        for segment in base.split('/').chain(argument.split('/')) {
            match segment {
                "" | "." => continue,
                ".." => {
                    depth = depth.saturating_sub(1);
                }
                name => {
                    if depth == vfs::MAX_PATH_SEGMENTS {
                        return None;
                    }
                    stack[depth] = name;
                    depth += 1;
                }
            }
        }

        let mut length = 0usize;
        if depth == 0 {
            out[0] = b'/';
            return Some(1);
        }
        for name in &stack[..depth] {
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

    fn resolve(&self, argument: &str) -> Result<NodeId, ()> {
        let mut path = [0u8; PATH_MAX];
        let Some(length) = self.absolute(argument, &mut path) else {
            return Err(());
        };
        let text = core::str::from_utf8(&path[..length]).map_err(|_| ())?;
        vfs::resolve(text).map_err(|_| ())
    }

    fn set_cwd(&mut self, argument: &str) {
        let mut path = [0u8; PATH_MAX];
        match self.absolute(argument, &mut path) {
            Some(length) => {
                self.cwd = path;
                self.cwd_len = length;
            }
            None => kprintln_color!(FG_ERROR, "path too long"),
        }
    }

    fn execute(&mut self, line: &[u8]) {
        let Ok(text) = core::str::from_utf8(line) else {
            kprintln_color!(FG_ERROR, "invalid input encoding");
            return;
        };
        let text = text.trim();
        if text.is_empty() {
            return;
        }

        let (command, rest) = match text.find(' ') {
            Some(index) => (&text[..index], text[index + 1..].trim()),
            None => (text, ""),
        };

        match command {
            "help" => self.cmd_help(),
            "ls" => self.cmd_ls(rest),
            "cat" => self.cmd_cat(rest),
            "write" => self.cmd_write(rest),
            "mkdir" => self.cmd_mkdir(rest),
            "rm" => self.cmd_rm(rest),
            "cd" => self.cmd_cd(rest),
            "pwd" => kprintln!("{}", self.cwd()),
            "show" => self.cmd_show(rest),
            "history" => self.cmd_history(rest),
            "verify" => self.cmd_verify(rest),
            "gui" | "loom" => crate::gui::run(),
            "nodes" => cmd_nodes(),
            "dedup" => cmd_dedup(),
            "info" => cmd_info(),
            "tasks" => {
                kprintln!("tasks:");
                sched::print_stats();
            }
            "spawn" => cmd_spawn(),
            "clear" => console::clear(),
            "echo" => kprintln!("{}", rest),
            "uptime" => {
                let seconds = timer::uptime_secs();
                kprintln!(
                    "up {}s ({} ms) at {} Hz, {} ticks",
                    seconds,
                    timer::uptime_ms(),
                    timer::TIMER_FREQUENCY,
                    timer::ticks()
                );
            }
            "reboot" => {
                kprintln!("rebooting...");
                timer::sleep_ms(200);
                cpu::reboot();
            }
            "shutdown" => {
                kprintln!("shutting down...");
                timer::sleep_ms(200);
                cpu::shutdown();
            }
            other => kprintln_color!(FG_ERROR, "unknown command: '{}' (try help)", other),
        }
    }

    fn cmd_help(&self) {
        kprintln!("commands:");
        let rows = [
            ("help", "this list"),
            ("ls [path]", "list a directory"),
            ("cat <path>", "print a node's content"),
            ("write <path> <text>", "create a new version of a file"),
            ("mkdir <path>", "create a directory"),
            ("rm <path>", "remove a directory entry"),
            ("cd [path]", "change directory (supports ..)"),
            ("pwd", "print the working directory"),
            ("show <path|id>", "node details: id, type, edges, properties"),
            ("history <path>", "walk a node's version chain"),
            ("verify <id>", "recompute a node's content address"),
            ("nodes", "node store statistics"),
            ("dedup", "demonstrate content-addressed deduplication"),
            ("info", "memory, heap and boot information"),
            ("gui", "return to the graphical desktop"),
            ("tasks", "scheduler state"),
            ("spawn", "start two background worker tasks"),
            ("echo <text>", "print text"),
            ("clear", "clear the screen (Ctrl-L)"),
            ("uptime", "time since boot"),
            ("reboot / shutdown", "restart or power off"),
        ];
        for (name, description) in rows {
            kprintln!("  {:<22} {}", name, description);
        }
    }

    fn cmd_ls(&self, argument: &str) {
        let argument = if argument.is_empty() { "." } else { argument };
        let id = match self.resolve(argument) {
            Ok(id) => id,
            Err(_) => {
                kprintln_color!(FG_ERROR, "ls: {}: no such node", argument);
                return;
            }
        };
        let Some(node) = node::get(&id) else {
            kprintln_color!(FG_ERROR, "ls: dangling node");
            return;
        };
        if !node.is_directory() {
            let name = argument.rsplit('/').next().unwrap_or(argument);
            print_entry(name, &id, node);
            return;
        }

        let mut total = 0usize;
        for edge in node.edges() {
            let Some(child) = node::get(&edge.target) else {
                continue;
            };
            print_entry(edge.label(), &edge.target, child);
            total += 1;
        }
        if total == 0 {
            kprintln_color!(FG_DIM, "(empty directory)");
        }
    }

    fn cmd_cat(&self, argument: &str) {
        if argument.is_empty() {
            kprintln_color!(FG_ERROR, "usage: cat <path>");
            return;
        }
        let id = match self.resolve(argument) {
            Ok(id) => id,
            Err(_) => {
                kprintln_color!(FG_ERROR, "cat: {}: no such node", argument);
                return;
            }
        };
        let Some(node) = node::get(&id) else {
            return;
        };
        if node.is_directory() {
            kprintln_color!(FG_ERROR, "cat: {}: is a directory", argument);
            return;
        }
        print_data(node);
    }

    fn cmd_write(&mut self, rest: &str) {
        let (path, payload) = match rest.find(' ') {
            Some(index) => (&rest[..index], rest[index + 1..].trim_start()),
            None => {
                kprintln_color!(FG_ERROR, "usage: write <path> <text>");
                return;
            }
        };
        let mut absolute = [0u8; PATH_MAX];
        let Some(length) = self.absolute(path, &mut absolute) else {
            kprintln_color!(FG_ERROR, "write: path too long");
            return;
        };
        let Ok(resolved) = core::str::from_utf8(&absolute[..length]) else {
            return;
        };

        match vfs::write_file(resolved, payload.as_bytes()) {
            Ok(id) => {
                let versions = vfs::version_count(id);
                let mut hex = [0u8; 64];
                node::format_id(&id, &mut hex);
                kprintln_color!(
                    FG_OK,
                    "wrote {} ({} bytes)",
                    resolved,
                    payload.len()
                );
                kprintln_color!(
                    FG_DIM,
                    "  node {} (version {})",
                    core::str::from_utf8(&hex).unwrap_or("?"),
                    versions
                );
            }
            Err(error) => kprintln_color!(
                FG_ERROR,
                "write: {}: {}",
                resolved,
                error.message()
            ),
        }
    }

    fn cmd_mkdir(&mut self, argument: &str) {
        if argument.is_empty() {
            kprintln_color!(FG_ERROR, "usage: mkdir <path>");
            return;
        }
        let mut absolute = [0u8; PATH_MAX];
        let Some(length) = self.absolute(argument, &mut absolute) else {
            kprintln_color!(FG_ERROR, "mkdir: path too long");
            return;
        };
        let Ok(resolved) = core::str::from_utf8(&absolute[..length]) else {
            return;
        };
        match vfs::mkdir(resolved) {
            Ok(_) => kprintln_color!(FG_OK, "created directory {}", resolved),
            Err(error) => {
                kprintln_color!(FG_ERROR, "mkdir: {}: {}", resolved, error.message())
            }
        }
    }

    fn cmd_rm(&mut self, argument: &str) {
        if argument.is_empty() {
            kprintln_color!(FG_ERROR, "usage: rm <path>");
            return;
        }
        let mut absolute = [0u8; PATH_MAX];
        let Some(length) = self.absolute(argument, &mut absolute) else {
            return;
        };
        let Ok(resolved) = core::str::from_utf8(&absolute[..length]) else {
            return;
        };
        if resolved == "/" {
            kprintln_color!(FG_ERROR, "rm: refusing to remove the root");
            return;
        }
        match vfs::remove(resolved) {
            Ok(_) => kprintln_color!(FG_OK, "removed {}", resolved),
            Err(error) => kprintln_color!(FG_ERROR, "rm: {}: {}", resolved, error.message()),
        }
    }

    fn cmd_cd(&mut self, argument: &str) {
        let argument = if argument.is_empty() { "/" } else { argument };
        let mut path = [0u8; PATH_MAX];
        let Some(length) = self.absolute(argument, &mut path) else {
            kprintln_color!(FG_ERROR, "cd: path too long");
            return;
        };
        let Ok(text) = core::str::from_utf8(&path[..length]) else {
            return;
        };
        match vfs::resolve(text) {
            Ok(id) => match node::get(&id) {
                Some(node) if node.is_directory() => self.set_cwd(text),
                Some(_) => kprintln_color!(FG_ERROR, "cd: {}: not a directory", text),
                None => kprintln_color!(FG_ERROR, "cd: dangling node"),
            },
            Err(_) => kprintln_color!(FG_ERROR, "cd: {}: no such directory", text),
        }
    }

    fn cmd_show(&self, argument: &str) {
        if argument.is_empty() {
            kprintln_color!(FG_ERROR, "usage: show <path|id-prefix>");
            return;
        }

        let id = if argument.len() >= 4 && argument.bytes().all(|b| b.is_ascii_hexdigit()) {
            match node::find_by_prefix(argument) {
                (Some(found), false) => found.id,
                (Some(_), true) => {
                    kprintln_color!(FG_ERROR, "show: id prefix '{}' is ambiguous", argument);
                    return;
                }
                _ => match self.resolve(argument) {
                    Ok(id) => id,
                    Err(_) => {
                        kprintln_color!(FG_ERROR, "show: no such node");
                        return;
                    }
                },
            }
        } else {
            match self.resolve(argument) {
                Ok(id) => id,
                Err(_) => {
                    kprintln_color!(FG_ERROR, "show: {}: no such node", argument);
                    return;
                }
            }
        };

        let Some(node) = node::get(&id) else {
            return;
        };
        let mut hex = [0u8; 64];
        node::format_id(&id, &mut hex);

        kprintln!("node {}", core::str::from_utf8(&hex).unwrap_or("?"));
        kprintln!("  type:       {}", node.kind.name());
        kprintln!("  content:    {} bytes", node.data_len);
        kprintln!("  created:    sequence {}", node.created);
        kprintln!("  verified:   {}", if node::verify(&id) { "yes" } else { "NO" });
        if let Some(previous) = node.previous {
            let mut old = [0u8; 64];
            node::format_id(&previous, &mut old);
            kprintln!(
                "  previous:   {}",
                core::str::from_utf8(&old).unwrap_or("?")
            );
        }

        if !node.edges().is_empty() {
            kprintln!("  edges ({}):", node.edges().len());
            for edge in node.edges() {
                let kind = node::get(&edge.target)
                    .map(|target| target.kind.name())
                    .unwrap_or("?");
                kprintln!("    {} -> {} [{}]", edge.label(), short_id(&edge.target), kind);
            }
        }

        if !node.props().is_empty() {
            kprintln!("  properties ({}):", node.props().len());
            for prop in node.props() {
                kprint!("    {} = ", prop.key());
                match prop.kind {
                    node::PropKind::Int => kprintln!("{}", prop.number),
                    node::PropKind::Bool => kprintln!("{}", prop.number != 0),
                    node::PropKind::Text => kprintln!("{}", prop.text_value()),
                }
            }
        }

        if node.data_len > 0 && node.data_len <= 96 && !node.is_directory() {
            kprintln!("  content preview:");
            print_data(node);
        }
    }

    fn cmd_history(&self, argument: &str) {
        if argument.is_empty() {
            kprintln_color!(FG_ERROR, "usage: history <path>");
            return;
        }
        let Ok(mut id) = self.resolve(argument) else {
            kprintln_color!(FG_ERROR, "history: {}: no such node", argument);
            return;
        };
        let mut depth = 0;
        kprintln!("version chain for {}:", argument);
        while let Some(node) = node::get(&id) {
            kprintln!(
                "  v{:<3} {} {} ({} bytes)",
                depth,
                short_id(&id),
                node.kind.name(),
                node.data_len
            );
            depth += 1;
            match node.previous {
                Some(previous) => id = previous,
                None => break,
            }
        }
        if depth == 1 {
            kprintln_color!(FG_DIM, "  (first version)");
        }
    }

    fn cmd_verify(&self, argument: &str) {
        let id = node::parse_id(argument)
            .or_else(|| node::find_by_prefix(argument).0.map(|found| found.id));
        let Some(id) = id else {
            kprintln_color!(FG_ERROR, "verify: expected a node id");
            return;
        };
        if node::get(&id).is_none() {
            kprintln_color!(FG_ERROR, "verify: no such node");
            return;
        }
        if node::verify(&id) {
            kprintln_color!(FG_OK, "verified: hash matches content");
        } else {
            kprintln_color!(FG_ERROR, "MISMATCH: content does not hash to this id");
        }
    }
}

fn banner() {
    kprintln_color!(FG_ACCENT, "  Vertex OS 0.2.0");
    kprintln_color!(FG_DIM, "  everything is a node");
    kprintln!();
    kprintln!("Type 'help' for commands.");
    kprintln_color!(FG_DIM, "Type 'gui' for the graphical desktop.");
    kprintln!();
}

/// Execute one command line against an explicit working directory.
///
/// The graphical front end drives the real shell through this entry point, so
/// every command behaves identically in both front ends.
pub fn execute_line(cwd: &str, line: &str) {
    let mut state = ShellState::new();
    let mut path = [0u8; PATH_MAX];
    let length = cwd.len().min(PATH_MAX - 1);
    if length > 0 {
        path[..length].copy_from_slice(&cwd.as_bytes()[..length]);
        state.cwd = path;
        state.cwd_len = length;
    }
    state.execute(line.as_bytes());
}

fn poll_byte() -> Option<u8> {
    if let Some(byte) = keyboard::pop_char() {
        return Some(byte);
    }
    serial::try_read_byte()
}

/// A 12-character prefix of a node id, formatted on demand.
struct ShortId([u8; 12]);

impl core::fmt::Display for ShortId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.pad(core::str::from_utf8(&self.0).unwrap_or("?"))
    }
}

fn short_id(id: &NodeId) -> ShortId {
    let mut hex = [0u8; 64];
    node::format_id(id, &mut hex);
    let mut out = [0u8; 12];
    out.copy_from_slice(&hex[..12]);
    ShortId(out)
}

fn print_entry(name: &str, id: &NodeId, node: &node::Node) {
    let short = short_id(id);
    match node.kind {
        NodeType::Directory => kprintln!(
            "  {:<24} {:<9} {:>6}  {}",
            name, "dir", "-", short
        ),
        _ => kprintln!(
            "  {:<24} {:<9} {:>6}  {}",
            name,
            node.kind.name(),
            node.data_len,
            short
        ),
    }
}

fn print_data(node: &node::Node) {
    let data = node.data();
    match core::str::from_utf8(data) {
        Ok(text) => {
            kprint!("{}", text);
            if !text.ends_with('\n') {
                kprintln!();
            }
        }
        Err(_) => {
            kprintln_color!(FG_DIM, "{} bytes of binary content:", data.len());
            for (index, byte) in data.iter().take(128).enumerate() {
                if index % 16 == 0 {
                    kprint!("\n  {:04x}: ", index);
                }
                kprint!("{:02x} ", byte);
            }
            kprintln!();
        }
    }
}

fn cmd_nodes() {
    let count = node::node_count();
    let capacity = node::capacity();
    let bytes = node::data_bytes();
    kprintln!("node store:");
    kprintln!("  nodes:      {} / {}", count, capacity);
    kprintln!("  data:       {} bytes", bytes);
    let mut root_hex = [0u8; 64];
    node::format_id(&node::root(), &mut root_hex);
    kprintln!(
        "  root:       {}",
        core::str::from_utf8(&root_hex).unwrap_or("?")
    );
    kprintln!();
    kprintln!("  {:<14} {:<10} {:>7}", "id", "type", "bytes");
    let mut shown = 0;
    node::for_each(|stored| {
        if shown >= 24 {
            return;
        }
        shown += 1;
        let short = short_id(&stored.id);
        kprintln!(
            "  {:<14} {:<10} {:>7}",
            short,
            stored.kind.name(),
            stored.data_len
        );
    });
    if count > shown {
        kprintln_color!(FG_DIM, "  ... {} more", count - shown);
    }
}

fn cmd_dedup() {
    kprintln!("content addressing self-test:");
    let payload = b"vertex dedup probe";
    let first = node::create(NodeType::Blob, payload, None, &[], &[]);
    let second = node::create(NodeType::Blob, payload, None, &[], &[]);
    let before = node::node_count();

    match (first, second) {
        (Some(a), Some(b)) => {
            let same = a == b;
            let mut hex = [0u8; 64];
            node::format_id(&a, &mut hex);
            kprintln!("  id:      {}", core::str::from_utf8(&hex).unwrap_or("?"));
            kprintln!("  first:   {}", if node::get(&a).is_some() { "stored" } else { "missing" });
            kprintln!("  second:  {}", if node::get(&b).is_some() { "stored" } else { "missing" });
            if same {
                kprintln_color!(FG_OK, "  identical content collapsed to one node");
            } else {
                kprintln_color!(FG_ERROR, "  FAIL: hashes differ");
            }
            if before == node::node_count() {
                kprintln_color!(FG_OK, "  store grew by 0 nodes (deduplicated)");
            }
        }
        _ => kprintln_color!(FG_ERROR, "  FAIL: could not create probe nodes"),
    }
}

fn cmd_info() {
    let (used, capacity) = crate::mm::heap::usage();
    let (cols, rows) = console::size();
    kprintln!("Vertex OS 0.2.0");
    kprintln!("  cpu:        x86_64, single core, ring 0 only");
    kprintln!("  boot:       Limine (base revision 3)");
    kprintln!("  memory:     {} MiB total, {} MiB free", crate::mm::pmm::total_mib(), crate::mm::pmm::free_mib());
    kprintln!(
        "  heap:       {} KiB used of {} MiB, {} allocations",
        used / 1024,
        capacity / 1024 / 1024,
        crate::mm::heap::allocation_count()
    );
    kprintln!("  nodes:      {} / {}", node::node_count(), node::capacity());
    kprintln!("  console:    {}x{} characters", cols, rows);
    kprintln!("  uptime:     {} s", timer::uptime_secs());
    kprintln!("  tasks:      {}", sched::task_count());
    kprintln!("  background: {} worker ticks", WORKER_TICKS.load(Ordering::Relaxed));
}

extern "C" fn worker_a() -> ! {
    loop {
        WORKER_TICKS.fetch_add(1, Ordering::Relaxed);
        timer::sleep_ms(20);
    }
}

extern "C" fn worker_b() -> ! {
    loop {
        WORKER_TICKS.fetch_add(3, Ordering::Relaxed);
        timer::sleep_ms(30);
    }
}

fn cmd_spawn() {
    match (sched::spawn("worker.a", worker_a), sched::spawn("worker.b", worker_b)) {
        (Some(a), Some(b)) => kprintln_color!(FG_OK, "spawned worker.a (id {}) and worker.b (id {})", a, b),
        _ => kprintln_color!(FG_ERROR, "could not spawn workers (task table full?)"),
    }
}

