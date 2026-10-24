# Vertex OS

[![build](https://github.com/SomberHighs/Vertex-OS/actions/workflows/build.yml/badge.svg)](https://github.com/SomberHighs/Vertex-OS/actions/workflows/build.yml)

**A bootable x86_64 research kernel whose filesystem is a content-addressed
node graph, with a graphical front end built around that graph.**

Vertex OS is not a Linux distribution. It is a small, self-contained kernel
written in `no_std` Rust that boots on bare metal or in QEMU via the Limine boot
protocol. Everything in the system — files, directories, users — is an
immutable *node* identified by the SHA-256 of its content, wired together by
typed edges.

![The Loom, Vertex OS's graphical front end](docs/loom.png)

It boots into **the Loom**, a desktop where the node graph is the interface: the
current directory is a constellation of orbiting nodes, the inspector shows the
selected node's identity and version chain, and a command bar drives the real
shell. `Esc` drops to the text console described below.

```
$ ./tools/build.sh run
Vertex OS 0.2.0 booting
=======================
[ok] bootloader: Limine 9.6.7
[ok] framebuffer: 1280x800 @ 32bpp (160x50 chars)
[ok] memory: 511 MiB total, 509 MiB free, HHDM at 0xffff800000000000
[ok] kernel image: 0xffffffff80000000..0xffffffff80037000
[ok] interrupts: GDT/IDT loaded, PIC remapped, PIT at 100 Hz
[ok] node graph: 6 nodes, root fb8851df2b8e9b3e...
[ok] scheduler: shell task spawned (id 1)
boot complete

  Vertex OS 0.2.0
  everything is a node

Type 'help' for commands.

vertex:/$ ls /
  docs                     dir            -  4c079826f39c
  readme.txt               text         351  7d755544b068
  users                    dir            -  bdcdcd843005
```

---

## Status

This kernel boots to an interactive shell on both BIOS and UEFI firmware, and
the shell can read, write, version and browse the node graph.

| Area | State |
| --- | --- |
| Limine boot (BIOS + UEFI) | works, verified in QEMU/SeaBIOS and QEMU/OVMF |
| Framebuffer console (8x16 font, colour, cursor, scrolling) | works |
| Serial console (output + input, 115200 8N1) | works |
| Physical memory manager (bitmap, 4 GiB window) | works |
| Virtual memory manager (4-level paging, map/unmap/translate) | works |
| Kernel heap (lazily mapped bump allocator, 64 MiB window) | works |
| GDT/IDT, exception handlers, PIC remap, PIT @ 100 Hz | works |
| PS/2 keyboard (scancode set 1, modifiers, ring buffer) | works |
| Preemptive round-robin scheduler (heap-backed stacks) | works |
| Content-addressed node graph + path VFS | works |
| Graphical front end (the Loom): constellation, inspector, command bar | works, verified on BIOS and UEFI |
| Interactive shell | works |
| Hosted C prototype of the node model | works (`make test_node`) |
| SMP, user mode/ring 3, disk drivers, networking, syscalls | **not implemented** |
| Persistent storage (the graph is in RAM) | **not implemented** |

There is no user mode: all code runs in ring 0, and the desktop and shell are
kernel threads. That is a deliberate scope limit of this incarnation, not an
accident. The front end is keyboard-driven; there is no mouse driver.

---

## Requirements

* **Rust nightly** with the `x86_64-unknown-none` target
  ```bash
  rustup toolchain install nightly --component rust-src --target x86_64-unknown-none
  ```
  (`rust-toolchain.toml` pins the target; nightly is required by the `limine`
  crate and bare-metal `core`.)
* **QEMU** (`qemu-system-x86`), and **xorriso** for ISO creation
  ```bash
  sudo apt install qemu-system-x86 xorriso ovmf    # Debian/Ubuntu
  brew install qemu xorriso                         # macOS
  ```
* **git**, `make`, and a C compiler for the hosted prototype.

The build script fetches the Limine binary release (`v9.x-binary`) on first use
and builds its host tool; no system-wide Limine install is needed.

---

## Build and run

```bash
./tools/build.sh build      # compile the kernel
./tools/build.sh iso        # compile + produce build/vertex-os.iso
./tools/build.sh run        # build the ISO and boot it (BIOS) in QEMU
./tools/build.sh run-uefi   # build the ISO and boot it (UEFI) in QEMU
./tools/build.sh test       # hosted C prototype of the node model
./tools/build.sh clean
```

Equivalent make targets: `make iso`, `make run`, `make run-uefi`,
`make test_node`, `make clean`.

QEMU selection can be overridden with `QEMU=/path/to/qemu-system-x86_64`. If
your QEMU keeps firmware outside its compiled-in data directory, set
`QEMU_DATA_DIR` (passed as `-L`).

The ISO is a hybrid image: burn it to a USB stick with `dd` and it boots on
UEFI or BIOS machines as-is.

### Automated boot test

`tools/smoke-test.py` boots the ISO headlessly, waits for a marker on the serial
port, drives either the serial console or the emulated PS/2 keyboard, asserts on
the transcript, and can save a screenshot of the framebuffer:

```bash
# graphical desktop, with a screenshot of the framebuffer
./tools/smoke-test.py build/vertex-os.iso --marker "loom ready" \
    --screenshot artifacts/loom.png

# walk the constellation with real PS/2 keys and assert on the log
./tools/smoke-test.py build/vertex-os.iso --marker "loom ready" \
    --keys "{down}{down}{enter}" --expect "[loom] open /users"

# leave the desktop and drive the text shell
./tools/smoke-test.py build/vertex-os.iso --marker "loom ready" \
    --keys "{esc}" --commands "ls /|write /a.txt hi|cat /a.txt" \
    --expect "wrote /a.txt"

./tools/smoke-test.py build/vertex-os.iso --marker "loom ready" --uefi
```

`--expect` may be repeated; every string must appear in the serial transcript or
the run fails. `--keys` types text through the emulated keyboard and understands
`{up}`, `{down}`, `{left}`, `{right}`, `{enter}`, `{esc}` and `{tab}`. Commands
run after any keys, so `--keys "{esc}" --commands "..."` exercises the text
console.

### Continuous integration

`.github/workflows/build.yml` runs on every push and pull request. It installs
the nightly toolchain, QEMU, xorriso and OVMF, builds the ISO, runs the hosted
node-model tests, and then boots the image five ways: the graphical desktop on
BIOS, keyboard navigation into a directory, a command-bar session, the text
console after `Esc`, and the graphical desktop on UEFI. Screenshots and the
built ISO are uploaded as workflow artefacts.

---

## The Loom (graphical front end)

On firmware that provides a linear framebuffer — both the BIOS and UEFI paths
do, in QEMU and on real hardware — Vertex OS boots into **the Loom**, a desktop
built around the node graph instead of around files.

* The **constellation** in the centre is the current directory: one node per
  entry, drawn with a silhouette that encodes its type (ring = directory, disc =
  text, diamond = user, triangle = process, square = anything else). Edges run
  from the core outwards with slowly travelling pulses.
* The **core** shows how many entries the directory holds.
* The **inspector** is the selected node: name, type, version count, edge flags,
  a SHA-256 fingerprint rendered as an 8x8 nibble grid, and then the directory's
  children, the node's properties, or the head of its content.
* The **command bar** is the real shell. Typed commands run through the same
  dispatcher as the text console, and their output is captured into the activity
  panel instead of the screen.
* `Esc` hands the framebuffer back to the text console; `gui` at the shell
  prompt returns to the desktop.

### Keys

| Key | Action |
| --- | --- |
| `Up` / `Down` | move the selection around the constellation |
| `Enter` / `Right` | open: descend into a directory, or read a file |
| `Left` | up one directory |
| printable keys | type into the command bar |
| `Enter` (with text typed) | run the command |
| `Backspace` | edit the command |
| `Esc` | clear the command, or leave for the text console |

### How it is drawn

`gui/canvas.rs` is a heap-backed compositor: each frame is composed in a 32-bit
canvas and presented with one blit, so animations never tear. All rasterisation
is integer-only — circles are anti-aliased by comparing an integer square root
against the ring radii and arcs walk a 91-entry sine table — because the kernel
never enables the FPU.

`gui/loom.rs` owns the constellation geometry, `gui/panels.rs` the chrome
(header, breadcrumb, inspector, footer, splash) and `gui/mod.rs` the desktop
state, input handling and frame loop. The console hands the framebuffer over
with `console::detach_framebuffer()` and takes it back on exit, so boot messages
and command output still reach the serial port — which is what the boot tests
read.

The desktop runs inside one kernel task. Composing a frame needs far more stack
than the shell did, and task stacks come from the same bump heap as the node
store, so `KERNEL_STACK_SIZE` is 64 KiB and every stack carries a guard word:
the scheduler halts with a message rather than letting an overflow quietly
scribble over nodes.

---

## The shell

| Command | Meaning |
| --- | --- |
| `help` | command list |
| `ls [path]` | list a directory (name, type, size, node id) |
| `cat <path>` | print a node's content |
| `write <path> <text>` | create a new version of a file |
| `mkdir <path>` / `rm <path>` | create / remove a directory entry |
| `cd [path]` / `pwd` | walk the graph (`..` is resolved by the shell) |
| `show <path\|id>` | id, type, size, edges, properties, verification |
| `history <path>` | walk the version chain |
| `verify <id>` | recompute a node's content address |
| `nodes` | node store statistics and a listing |
| `dedup` | demonstrate content-addressed deduplication |
| `info` | memory, heap, console and scheduler summary |
| `tasks` / `spawn` | inspect the scheduler / start two worker threads |
| `uptime`, `echo`, `clear` | as expected |
| `reboot`, `shutdown` | restart or power off |

Line editing: backspace, Ctrl-C (abandon line), Ctrl-L (clear), Ctrl-U (erase).
Input comes from the PS/2 keyboard *and* the serial port, so the same shell
works headless.

---

## How it works

### Boot

1. Limine loads the ELF, sets up long mode, the higher-half mapping, the
   framebuffer and the memory map, then jumps to `_start`.
2. `main.rs` brings up serial, the framebuffer console, memory management, the
   CPU tables, the node graph, and finally the scheduler and shell task.
3. The boot context becomes the idle task; the timer (100 Hz) preempts it into
   the shell.

Limine requests live in the `.requests` section and are marked `used`; the
linker script `KEEP`s the section so `--gc-sections` cannot drop them. The
kernel targets **base revision 3** (what Limine 8/9 implement) rather than the
`limine` crate's newer default.

### Memory

* **PMM** — a bitmap over the first 4 GiB. Only `MEMMAP_USABLE` regions are
  freed; the first megabyte and the kernel image (`__kernel_start`/`__kernel_end`)
  are reserved explicitly, so the allocator can never hand out live memory.
* **VMM** — walks and extends the PML4 Limine built. Page tables are allocated
  from the PMM and accessed through the HHDM. Provides `map_page`, `unmap_page`
  and `translate`.
* **Heap** — a 64 MiB virtual window at `0xffff_9000_0000_0000`, backed page by
  page on demand. Allocation never blocks and never frees: node storage is
  append-only by design.

### Scheduling

Round-robin with 50 ms slices. Each task owns a 16 KiB stack from the heap, and
a task's saved context is just a stack pointer, so tasks are never copied while
they run. All scheduler state is mutated with interrupts disabled, and the timer
handler preempts through the same `switch_context` path a voluntary yield uses.
The interrupt path cannot deadlock on the console because interrupt handlers
never print: they push into ring buffers that normal context drains.

### The node graph

`kernel/src/vfs/`:

* `sha256.rs` — a tested SHA-256 (verified against reference vectors, including
  multi-block and padding edge cases).
* `node.rs` — the immutable node: id, type, content, typed edges, properties and
  a `previous` link forming a version chain. A node's id is the hash of a
  canonical serialisation (type, content length, content, previous id, sorted
  edges, sorted properties), so identical content collapses onto one node and a
  node's address can always be re-derived and verified.
* `mod.rs` — paths over the graph. Directories are nodes whose edges are their
  entries. Updating a file builds a new file node and rebuilds every directory on
  the path, like a Git tree; the root id is the single pointer that defines the
  filesystem state.

Because nodes never change, `write` on an existing path appends a version rather
than overwriting: `history /notes.txt` shows the chain, and `verify` recomputes
the hash.

---

## Repository layout

```
vertex-os/
├── kernel/
│   ├── Cargo.toml            # limine crate dependency, release profile
│   ├── .cargo/config.toml    # bare-metal target, no red zone
│   ├── linker.ld             # higher-half layout, KEEP'd Limine requests
│   ├── limine.conf           # boot menu entry
│   └── src/
│       ├── main.rs           # boot sequence
│       ├── crt.rs            # freestanding mem* routines
│       ├── panic.rs
│       ├── shell.rs          # interactive shell + command dispatcher
│       ├── gui/              # the Loom: canvas, constellation, panels, desktop
│       ├── arch/x86_64/      # cpu, gdt, idt, interrupts, pic, timer, keyboard, serial
│       ├── console/          # framebuffer, font, console with colour + cursor
│       ├── mm/               # addr, pmm, vmm, heap
│       ├── sched/            # context switch, tasks, scheduler
│       ├── sync/             # interrupt-safe spinlock, shared cell
│       └── vfs/              # sha256, node store, path layer
├── graph/                    # standalone C model of the node system
├── tools/
│   ├── build.sh              # kernel -> ISO -> QEMU
│   └── smoke-test.py         # headless boot + GUI/shell + screenshot test
└── makefile
```

Repository root:

```
.github/workflows/build.yml   # CI: build the ISO, run the boot tests
README.md                     # this document
docs/loom.png                 # screenshot of the front end
```

The C prototype (`graph/node.c`, `graph/node.h`) predates the kernel
implementation and stays as a fast, dependency-free way to experiment with the
node model on a host:

```bash
make test_node && ./test_node
```

---

## Known limitations

* Single CPU. The scheduler and the lock primitives assume one core; `SpinLock`
  disables interrupts rather than implementing a fair multi-core lock.
* No user mode, no syscall interface, no process isolation — the shell is a
  kernel task.
* The graph lives in RAM and is reset on reboot; there is no block driver.
* The heap never frees. Roughly 1 MiB of node metadata plus content lives in the
  64 MiB window; the store holds up to 1024 nodes.
* Files are stored whole in memory; there is no paging to disk or size streaming.
* Directories are capped at 16 entries and paths at 8 components.
* The front end is keyboard-driven: there is no PS/2 mouse driver and no pointer.
  The layout needs a framebuffer of at least 860x560; below that the kernel
  stays on the text console.

## Resources

* [Limine boot protocol](https://github.com/limine-bootloader/limine/blob/trunk/PROTOCOL.md)
* [OSDev Wiki](https://wiki.osdev.org)
* [Writing an OS in Rust](https://os.phil-opp.com/)

## License

MIT.
