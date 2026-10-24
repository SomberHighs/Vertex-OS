# Vertex OS (kernel source tree)

This directory contains the kernel, the graphical front end, the node-graph
prototype and the build tooling.

The authoritative documentation — architecture, build instructions, key
bindings, shell commands, verification and known limitations — is in the
[project README](../README.md).

Quick reference:

```bash
./tools/build.sh run       # build the ISO and boot the desktop (BIOS) in QEMU
./tools/build.sh run-uefi  # same, with OVMF/UEFI
./tools/smoke-test.py build/vertex-os.iso --marker "loom ready" \
    --screenshot artifacts/loom.png
make test_node && ./test_node   # hosted C prototype of the node model
```

```
kernel/src
├── main.rs            boot sequence and Limine requests
├── shell.rs           interactive shell and command dispatcher
├── gui                the Loom: canvas, constellation, panels, desktop loop
├── arch/x86_64        cpu, gdt, idt, interrupts, pic, timer, keyboard, serial
├── console            framebuffer, font, coloured console, output capture
├── mm                 pmm (bitmap), vmm (4-level paging), heap
├── sched              context switch, tasks, round-robin scheduler
├── sync               interrupt-safe spinlock, shared cell
└── vfs                sha256, content-addressed node store, path layer
```
