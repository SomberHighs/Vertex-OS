# Vertex OS

![Build Status](https://img.shields.io/badge/build-passing-brightgreen?style=flat-square)
![Arch](https://img.shields.io/badge/arch-x86__64-blue?style=flat-square&logo=intel)
![Language](https://img.shields.io/badge/language-Rust_|_C-orange?style=flat-square&logo=rust)
![License](https://img.shields.io/badge/license-MIT-lightgrey?style=flat-square)

**Vertex OS** is a research operating system designed to synthesize the best architectural decisions of the last 40 years. It is not a Linux distribution or a Windows clone; it is a distinct kernel written in Rust that aims to merge the modularity of Unix with the robust Object Manager philosophy of Windows NT.

> **The Goal:** A "Zero-Curve" operating system where security is intrinsic, data is content-addressed, and the user experience adapts to refugees from macOS, Windows, and Linux alike.

---

## 🧠 Core Philosophy: The Node System

The heart of Vertex OS is the **Vertex Node Graph** (VFS). Unlike traditional hierarchical file systems (Unix) or drive-letter based systems (Windows), Vertex stores system state in a content-addressed graph.

As seen in `kernel/src/vfs/node.rs`, every entity in the system—files, users, processes, and capabilities—is a **Node**:

* **Content-Addressed:** Nodes are identified by the SHA-256 hash of their content.
* **Immutable:** Nodes never change. Modifications create new nodes, creating an intrinsic version history (similar to Git).
* **Typed Edges:** Relationships between nodes (e.g., "contains", "authorized_by", "parent_of") are typed and queryable.

This unifies the Unix "Everything is a file" with the NT "Everything is an Object" into **"Everything is a Node."**

---

## 🚀 Features

### **Kernel Architecture**
* 🦀 **Written in Rust:** Leveraging nightly Rust for memory safety and modern design patterns.
* ⚡ **Limine Bootloader:** Modern UEFI/BIOS support via the Limine protocol.
* 🔄 **Hybrid Scheduler:** Preemptive round-robin multithreading with dynamic time slices.
* 💾 **Memory Management:** Full PMM (Bitmap) and VMM (4-Level Paging) with Higher Half Direct Mapping (HHDM).

### **The Graph VFS**
* **Deduped Storage:** Identical files share the same ID automatically.
* **Strongly Typed:** Nodes have semantic types like `Blob`, `Text`, `User`, `Process`, and `Capability`.

### **Driver Support**
* 🖥️ **Video:** Framebuffer video output (VESA/GOP) with built-in font rendering.
* ⌨️ **Input:** PS/2 Keyboard driver with scancode translation.
* ⚙️ **Hardware:** 8259 PIC management and Serial I/O for debugging.

---

## 🛠️ Building & Running

Vertex OS includes a comprehensive build tool located at `tools/build.sh`.

### Prerequisites
You will need a Linux or macOS environment (WSL2 works on Windows) with the following installed:
* **Rust (Nightly):** `rustup override set nightly`
* **QEMU:** `qemu-system-x86_64`
* **Xorriso:** Required for ISO creation.
* **Build Essentials:** `gcc`, `make`

### Quick Start

1.  **Clone the repository:**
    ```bash
    git clone [https://github.com/yourusername/vertex-os.git](https://github.com/yourusername/vertex-os.git)
    cd vertex-os
    ```

2.  **Build and Run (BIOS Mode):**
    This will compile the kernel, download Limine, create an ISO, and launch QEMU.
    ```bash
    ./tools/build.sh run
    ```

3.  **Build and Run (UEFI Mode):**
    Requires OVMF firmware installed.
    ```bash
    ./tools/build.sh run-uefi
    ```

4.  **Test the Node Implementation (Userspace Prototype):**
    We include a C-based prototype of the graph node logic to test the hashing algorithms.
    ```bash
    make test_node
    ./test_node
    ```

---

## 📂 Project Structure

```bash
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
