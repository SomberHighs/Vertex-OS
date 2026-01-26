Vertex OS
Vertex OS is a research operating system designed to synthesize the best architectural decisions of the last 40 years. It is not a Linux distribution or a Windows clone; it is a distinct kernel written in Rust that aims to merge the modularity of Unix with the robust Object Manager philosophy of Windows NT.

The Goal: A "Zero-Curve" operating system where security is intrinsic, data is content-addressed, and the user experience adapts to refugees from macOS, Windows, and Linux alike.

🧠 Core Philosophy: The Node System
The heart of Vertex OS is the Vertex Node Graph (VFS). Unlike traditional hierarchical file systems (Unix) or drive-letter based systems (Windows), Vertex stores system state in a content-addressed graph.

As seen in kernel/src/vfs/node.rs, every entity in the system—files, users, processes, and capabilities—is a Node:

Content-Addressed: Nodes are identified by the SHA-256 hash of their content.

Immutable: Nodes never change. Modifications create new nodes, creating an intrinsic version history (similar to Git).

Typed Edges: Relationships between nodes (e.g., "contains", "authorized_by", "parent_of") are typed and queryable.

This unifies the Unix "Everything is a file" with the NT "Everything is an Object" into "Everything is a Node."

🚀 Features
Kernel Architecture:

Written in Rust (Nightly) for memory safety and modern design patterns.

Limine Bootloader protocol for modern UEFI/BIOS support.

Hybrid Scheduler: Preemptive round-robin multithreading with varying time slices.

Memory Management: Full PMM (Bitmap) and VMM (4-Level Paging) with Higher Half Direct Mapping (HHDM).

The Graph VFS:

Deduped storage (identical files share the same ID).

Strongly typed nodes (Blob, Text, User, Process, Capability).

Driver Support:

Framebuffer video output (VESA/GOP).

PS/2 Keyboard & 8259 PIC management.

Serial I/O for debugging.

🛠️ Building & Running
Vertex OS includes a comprehensive build tool located at tools/build.sh.

Prerequisites
You will need a Linux or macOS environment (WSL2 works on Windows) with the following installed:

Rust (Nightly): rustup override set nightly

QEMU: qemu-system-x86_64

Xorriso: For ISO creation.

Build Essentials: gcc, make, nasmd (optional).

Quick Start
Clone the repository:

Bash
git clone https://github.com/yourusername/vertex-os.git
cd vertex-os
Build and Run (BIOS Mode): This will compile the kernel, download Limine, create an ISO, and launch QEMU.

Bash
./tools/build.sh run
Build and Run (UEFI Mode): Requires OVMF firmware installed.

Bash
./tools/build.sh run-uefi
Test the Node Implementation (Userspace Prototype): We include a C-based prototype of the graph node logic.

Bash
make test_node
./test_node
📂 Project Structure
Bash
vertex-os/
├── kernel/                 # The Rust Kernel
│   ├── src/
│   │   ├── arch/           # x86_64 specific code (GDT, IDT, Paging)
│   │   ├── mm/             # Physical & Virtual Memory Managers
│   │   ├── sched/          # Scheduler & Task Management
│   │   ├── sync/           # Spinlocks & Synchronization
│   │   ├── syscall/        # System Call Interface
│   │   ├── vfs/            # The Vertex Node Graph implementation
│   │   └── main.rs         # Kernel Entry Point
│   ├── limine.conf         # Bootloader config
│   └── linker.ld           # Kernel memory layout
├── graph/                  # C prototypes for the Node system
├── tools/                  # Build scripts
└── build/                  # Build artifacts (ISO, binaries)
🗺️ Roadmap
Phase 1 (Current): Kernel basics, Memory management, Interrupts, Basic Scheduler.

Phase 2: Porting the Node graph to a persistent disk format.

Phase 3: Userspace loader (running ELF binaries).

Phase 4: The "Migration Bridge" (Compatibility layers for Linux/Windows apps).

Phase 5: The "Adaptive UI" Shell.

🤝 Contributing
Vertex OS is an open project. We welcome contributions from developers interested in systems programming, Rust, and OSDev.

Fork the Project

Create your Feature Branch (git checkout -b feature/AmazingFeature)

Commit your Changes (git commit -m 'Add some AmazingFeature')

Push to the Branch (git push origin feature/AmazingFeature)

Open a Pull Request

📄 License
Distributed under the MIT License. See LICENSE for more information.

Vertex OS — The best of all worlds.
