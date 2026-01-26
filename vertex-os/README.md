# Vertex OS

A hobby operating system kernel written in Rust, targeting x86_64 architecture.

## Features

- **Limine Bootloader**: Uses the modern Limine boot protocol
- **Framebuffer Console**: Text output via VESA framebuffer with built-in font
- **Memory Management**:
  - Physical Memory Manager (PMM) with bitmap allocator
  - Virtual Memory Manager (VMM) with 4-level paging support
- **CPU Architecture Support**:
  - Global Descriptor Table (GDT)
  - Interrupt Descriptor Table (IDT)
  - Exception handlers
- **Synchronization**: Basic spinlock primitive

## Project Structure

```
vertex-os/
├── kernel/
│   ├── Cargo.toml           # Rust package manifest
│   ├── rust-toolchain.toml  # Nightly Rust + components
│   ├── .cargo/config.toml   # Build configuration
│   ├── linker.ld            # Kernel linker script
│   ├── limine.conf          # Bootloader configuration
│   └── src/
│       ├── main.rs          # Kernel entry point
│       ├── panic.rs         # Panic handler
│       ├── arch/x86_64/     # x86_64 architecture code
│       ├── mm/              # Memory management
│       ├── console/         # Framebuffer console
│       └── sync/            # Synchronization primitives
├── tools/
│   └── build.sh             # Build and run script
└── README.md
```

## Prerequisites

### Required

- **Rust** (nightly): Install via [rustup](https://rustup.rs)
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- **QEMU**: For testing the OS
  ```bash
  # Ubuntu/Debian
  sudo apt install qemu-system-x86
  
  # macOS
  brew install qemu
  
  # Windows
  # Download from https://www.qemu.org/download/#windows
  ```

### Optional (for ISO creation)

- **xorriso**: For creating bootable ISO images
  ```bash
  # Ubuntu/Debian
  sudo apt install xorriso
  
  # macOS
  brew install xorriso
  ```

- **OVMF**: For UEFI testing
  ```bash
  # Ubuntu/Debian
  sudo apt install ovmf
  ```

## Building

### Build the Kernel Only

```bash
cd kernel
cargo build --release
```

### Build and Create ISO

```bash
chmod +x tools/build.sh
./tools/build.sh iso
```

### Build and Run in QEMU

```bash
# BIOS mode
./tools/build.sh run

# UEFI mode
./tools/build.sh run-uefi
```

### Clean Build Artifacts

```bash
./tools/build.sh clean
```

## Architecture

### Boot Process

1. BIOS/UEFI loads Limine bootloader
2. Limine sets up long mode (64-bit) and higher-half mapping
3. Limine loads kernel at `0xffffffff80000000`
4. Kernel entry point `_start()` is called
5. GDT and IDT are initialized
6. Framebuffer console is set up
7. Memory managers are initialized
8. Kernel enters halt loop

### Memory Layout

- **Higher Half Kernel**: Kernel is loaded at `0xffffffff80000000`
- **HHDM**: Higher Half Direct Map for physical memory access
- **Page Size**: 4 KiB pages (2 MiB and 1 GiB huge pages supported)

### Interrupt Handling

The kernel sets up handlers for CPU exceptions (vectors 0-31):
- Divide Error (#DE)
- Debug (#DB)
- NMI
- Breakpoint (#BP)
- General Protection Fault (#GP)
- Page Fault (#PF)
- And more...

## Development

### Adding New Features

1. Create a new module in the appropriate directory
2. Add `mod` declaration in the parent `mod.rs`
3. Use `kprintln!` macro for debug output

### Debugging Tips

- Use `kprintln!` for kernel debug output
- QEMU's `-serial stdio` flag shows serial output
- Add `-d int` to QEMU for interrupt debugging
- Use `-s -S` flags for GDB debugging

## License

This project is open source. Feel free to use, modify, and distribute.

## Resources

- [OSDev Wiki](https://wiki.osdev.org)
- [Limine Boot Protocol](https://github.com/limine-bootloader/limine/blob/trunk/PROTOCOL.md)
- [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
- [Writing an OS in Rust](https://os.phil-opp.com/)
