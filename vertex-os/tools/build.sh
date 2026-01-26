#!/bin/bash
# Vertex OS Build Script
# This script builds the kernel and creates a bootable ISO using Limine

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
KERNEL_DIR="$PROJECT_ROOT/kernel"
BUILD_DIR="$PROJECT_ROOT/build"
ISO_DIR="$BUILD_DIR/iso_root"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Check dependencies
check_deps() {
    info "Checking dependencies..."
    
    if ! command -v rustup &> /dev/null; then
        error "rustup not found. Please install Rust: https://rustup.rs"
    fi
    
    if ! command -v xorriso &> /dev/null; then
        warn "xorriso not found. ISO creation will not work."
        warn "Install with: apt install xorriso (Linux) or brew install xorriso (macOS)"
    fi
}

# Download Limine bootloader if not present
get_limine() {
    LIMINE_DIR="$BUILD_DIR/limine"
    
    if [ ! -d "$LIMINE_DIR" ]; then
        info "Downloading Limine bootloader..."
        git clone https://github.com/limine-bootloader/limine.git --branch=v7.x-binary --depth=1 "$LIMINE_DIR"
    fi
    
    # Build limine utility if needed
    if [ ! -f "$LIMINE_DIR/limine" ]; then
        info "Building Limine utility..."
        make -C "$LIMINE_DIR"
    fi
}

# Build the kernel
build_kernel() {
    info "Building kernel..."
    cd "$KERNEL_DIR"
    cargo build --release
    
    KERNEL_BIN="$KERNEL_DIR/target/x86_64-unknown-none/release/vertex-kernel"
    if [ ! -f "$KERNEL_BIN" ]; then
        error "Kernel build failed!"
    fi
    
    info "Kernel built successfully: $KERNEL_BIN"
}

# Create bootable ISO
create_iso() {
    info "Creating bootable ISO..."
    
    LIMINE_DIR="$BUILD_DIR/limine"
    KERNEL_BIN="$KERNEL_DIR/target/x86_64-unknown-none/release/vertex-kernel"
    
    # Clean and create ISO directory structure
    rm -rf "$ISO_DIR"
    mkdir -p "$ISO_DIR/boot"
    mkdir -p "$ISO_DIR/boot/limine"
    mkdir -p "$ISO_DIR/EFI/BOOT"
    
    # Copy kernel
    cp "$KERNEL_BIN" "$ISO_DIR/boot/vertex-kernel"
    
    # Copy Limine configuration
    cp "$KERNEL_DIR/limine.conf" "$ISO_DIR/boot/limine/"
    
    # Copy Limine files
    cp "$LIMINE_DIR/limine-bios.sys" "$ISO_DIR/boot/limine/"
    cp "$LIMINE_DIR/limine-bios-cd.bin" "$ISO_DIR/boot/limine/"
    cp "$LIMINE_DIR/limine-uefi-cd.bin" "$ISO_DIR/boot/limine/"
    cp "$LIMINE_DIR/BOOTX64.EFI" "$ISO_DIR/EFI/BOOT/"
    cp "$LIMINE_DIR/BOOTIA32.EFI" "$ISO_DIR/EFI/BOOT/"
    
    # Create ISO
    ISO_FILE="$BUILD_DIR/vertex-os.iso"
    xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
        -no-emul-boot -boot-load-size 4 -boot-info-table \
        --efi-boot boot/limine/limine-uefi-cd.bin \
        -efi-boot-part --efi-boot-image --protective-msdos-label \
        "$ISO_DIR" -o "$ISO_FILE"
    
    # Install Limine for BIOS boot
    "$LIMINE_DIR/limine" bios-install "$ISO_FILE"
    
    info "ISO created: $ISO_FILE"
}

# Run in QEMU
run_qemu() {
    ISO_FILE="$BUILD_DIR/vertex-os.iso"
    
    if [ ! -f "$ISO_FILE" ]; then
        error "ISO not found. Run './build.sh iso' first."
    fi
    
    info "Starting QEMU..."
    
    # Try to use KVM if available
    KVM_OPTS=""
    if [ -e /dev/kvm ]; then
        KVM_OPTS="-enable-kvm"
    fi
    
    qemu-system-x86_64 \
        $KVM_OPTS \
        -M q35 \
        -m 256M \
        -cdrom "$ISO_FILE" \
        -serial stdio \
        -no-reboot \
        -no-shutdown
}

# Run in QEMU with UEFI
run_qemu_uefi() {
    ISO_FILE="$BUILD_DIR/vertex-os.iso"
    
    if [ ! -f "$ISO_FILE" ]; then
        error "ISO not found. Run './build.sh iso' first."
    fi
    
    # Check for OVMF
    OVMF_PATHS=(
        "/usr/share/OVMF/OVMF_CODE.fd"
        "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd"
        "/usr/share/qemu/OVMF.fd"
    )
    
    OVMF=""
    for path in "${OVMF_PATHS[@]}"; do
        if [ -f "$path" ]; then
            OVMF="$path"
            break
        fi
    done
    
    if [ -z "$OVMF" ]; then
        error "OVMF not found. Install OVMF for UEFI support."
    fi
    
    info "Starting QEMU with UEFI..."
    
    qemu-system-x86_64 \
        -M q35 \
        -m 256M \
        -bios "$OVMF" \
        -cdrom "$ISO_FILE" \
        -serial stdio \
        -no-reboot \
        -no-shutdown
}

# Clean build artifacts
clean() {
    info "Cleaning build artifacts..."
    rm -rf "$BUILD_DIR"
    cd "$KERNEL_DIR" && cargo clean
    info "Clean complete."
}

# Print usage
usage() {
    echo "Vertex OS Build Script"
    echo ""
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  build     Build the kernel"
    echo "  iso       Build kernel and create bootable ISO"
    echo "  run       Build and run in QEMU (BIOS)"
    echo "  run-uefi  Build and run in QEMU (UEFI)"
    echo "  clean     Clean build artifacts"
    echo "  help      Show this help message"
}

# Main
case "${1:-build}" in
    build)
        check_deps
        build_kernel
        ;;
    iso)
        check_deps
        get_limine
        build_kernel
        create_iso
        ;;
    run)
        check_deps
        get_limine
        build_kernel
        create_iso
        run_qemu
        ;;
    run-uefi)
        check_deps
        get_limine
        build_kernel
        create_iso
        run_qemu_uefi
        ;;
    clean)
        clean
        ;;
    help|--help|-h)
        usage
        ;;
    *)
        error "Unknown command: $1"
        usage
        exit 1
        ;;
esac
