#!/usr/bin/env bash
#
# Vertex OS build script: kernel -> bootable ISO -> QEMU.
#
#   ./tools/build.sh build     compile the kernel
#   ./tools/build.sh iso       compile the kernel and produce build/vertex-os.iso
#   ./tools/build.sh run       build the ISO and boot it (BIOS) in QEMU
#   ./tools/build.sh run-uefi  build the ISO and boot it (UEFI) in QEMU
#   ./tools/build.sh test      run the hosted node-graph prototype tests
#   ./tools/build.sh clean     remove build artefacts

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
KERNEL_DIR="$PROJECT_ROOT/kernel"
BUILD_DIR="$PROJECT_ROOT/build"
ISO_ROOT="$BUILD_DIR/iso_root"
LIMINE_DIR="$BUILD_DIR/limine"
LIMINE_BRANCH="v9.x-binary"
LIMINE_REPO="https://github.com/limine-bootloader/limine.git"
ISO_FILE="$BUILD_DIR/vertex-os.iso"
KERNEL_ELF="$KERNEL_DIR/target/x86_64-unknown-none/release/vertex-kernel"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

info() { printf "${GREEN}[info]${NC} %s\n" "$*"; }
warn() { printf "${YELLOW}[warn]${NC} %s\n" "$*"; }
die() {
    printf "${RED}[error]${NC} %s\n" "$*" >&2
    exit 1
}

# Make a user-local rustup install visible even in non-login shells.
setup_toolchain() {
    if ! command -v cargo >/dev/null 2>&1; then
        for candidate in "$HOME/.cargo/bin" "/usr/local/cargo/bin"; do
            if [ -x "$candidate/cargo" ]; then
                export PATH="$candidate:$PATH"
                break
            fi
        done
    fi
    command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust: https://rustup.rs"
    command -v rustc >/dev/null 2>&1 || die "rustc not found."
}

require_qemu() {
    export QEMU_BIN="${QEMU:-qemu-system-x86_64}"
    command -v "$QEMU_BIN" >/dev/null 2>&1 ||
        die "qemu-system-x86_64 not found. Install QEMU or set QEMU=/path/to/qemu-system-x86_64"
}

check_deps() {
    setup_toolchain
    command -v xorriso >/dev/null 2>&1 ||
        warn "xorriso not found: ISO creation will fail (apt install xorriso)"
}

build_kernel() {
    info "building kernel (nightly, x86_64-unknown-none)"
    (
        cd "$KERNEL_DIR"
        # Absolute linker script path: cargo does not guarantee the CWD.
        RUSTFLAGS="-C link-arg=-T$KERNEL_DIR/linker.ld -C link-arg=--gc-sections -C relocation-model=static" \
            cargo build --release
    )
    [ -f "$KERNEL_ELF" ] || die "kernel build produced no ELF at $KERNEL_ELF"
    info "kernel: $(du -h "$KERNEL_ELF" | cut -f1) at $KERNEL_ELF"
}

get_limine() {
    if [ ! -d "$LIMINE_DIR/.git" ]; then
        info "fetching Limine ($LIMINE_BRANCH)"
        rm -rf "$LIMINE_DIR"
        command -v git >/dev/null 2>&1 || die "git is required to fetch Limine"
        git clone --branch="$LIMINE_BRANCH" --depth=1 "$LIMINE_REPO" "$LIMINE_DIR" >/dev/null 2>&1 ||
            die "could not clone Limine"
    fi

    # The binary branch ships limine.c; build the host tool if needed.
    if [ ! -x "$LIMINE_DIR/limine" ]; then
        info "building the limine host tool"
        make -C "$LIMINE_DIR" >/dev/null || die "failed to build the limine tool"
    fi
}

create_iso() {
    command -v xorriso >/dev/null 2>&1 || die "xorriso is required to create the ISO"

    info "assembling ISO tree"
    rm -rf "$ISO_ROOT"
    mkdir -p "$ISO_ROOT/boot/limine" "$ISO_ROOT/EFI/BOOT"

    cp "$KERNEL_ELF" "$ISO_ROOT/boot/vertex-kernel"
    cp "$KERNEL_DIR/limine.conf" "$ISO_ROOT/boot/limine/limine.conf"
    cp "$LIMINE_DIR/limine-bios.sys" "$ISO_ROOT/boot/limine/"
    cp "$LIMINE_DIR/limine-bios-cd.bin" "$ISO_ROOT/boot/limine/"
    cp "$LIMINE_DIR/limine-uefi-cd.bin" "$ISO_ROOT/boot/limine/"
    cp "$LIMINE_DIR/BOOTX64.EFI" "$ISO_ROOT/EFI/BOOT/"
    cp "$LIMINE_DIR/BOOTIA32.EFI" "$ISO_ROOT/EFI/BOOT/"

    info "creating El Torito hybrid ISO"
    xorriso -as mkisofs \
        -b boot/limine/limine-bios-cd.bin \
        -no-emul-boot -boot-load-size 4 -boot-info-table \
        --efi-boot boot/limine/limine-uefi-cd.bin \
        -efi-boot-part --efi-boot-image --protective-msdos-label \
        "$ISO_ROOT" -o "$ISO_FILE" >/dev/null

    "$LIMINE_DIR/limine" bios-install "$ISO_FILE" >/dev/null
    info "ISO: $ISO_FILE ($(du -h "$ISO_FILE" | cut -f1))"
}

kvm_args() {
    if [ -w /dev/kvm ]; then
        printf '%s' "-enable-kvm -cpu host"
    fi
}

run_bios() {
    require_qemu
    [ -f "$ISO_FILE" ] || die "ISO missing; run './tools/build.sh iso' first"
    info "booting BIOS image in QEMU (Ctrl-A X to quit)"
    # shellcheck disable=SC2046
    exec "$QEMU_BIN" $(kvm_args) \
        -M q35 -m 512M -cdrom "$ISO_FILE" \
        -serial stdio -no-reboot
}

find_ovmf() {
    for candidate in \
        /usr/share/OVMF/OVMF_CODE.fd \
        /usr/share/OVMF/OVMF_CODE_4M.fd \
        /usr/share/edk2/ovmf/OVMF_CODE.fd \
        /usr/share/edk2-ovmf/x64/OVMF_CODE.fd \
        /usr/share/qemu/OVMF.fd; do
        if [ -f "$candidate" ]; then
            printf '%s' "$candidate"
            return 0
        fi
    done
    return 1
}

run_uefi() {
    require_qemu
    local ovmf
    ovmf="$(find_ovmf)" || die "OVMF firmware not found (apt install ovmf)"
    [ -f "$ISO_FILE" ] || die "ISO missing; run './tools/build.sh iso' first"

    # 4 MiB OVMF images start with padding and are meant for pflash; `-bios`
    # only accepts the older combined image.
    local firmware=()
    local size
    size="$(wc -c < "$ovmf")"
    if [ "$size" -ge 4194304 ]; then
        firmware=(-drive "if=pflash,format=raw,unit=0,readonly=on,file=$ovmf")
        local variables="${ovmf/CODE/VARS}"
        if [ -f "$variables" ]; then
            cp "$variables" "$BUILD_DIR/OVMF_VARS.fd"
            firmware+=(-drive "if=pflash,format=raw,unit=1,file=$BUILD_DIR/OVMF_VARS.fd")
        fi
    else
        firmware=(-bios "$ovmf")
    fi

    info "booting UEFI image with $ovmf"
    exec "$QEMU_BIN" $(kvm_args) \
        -M q35 -m 512M "${firmware[@]}" -cdrom "$ISO_FILE" \
        -serial stdio -no-reboot
}

run_node_test() {
    info "building and running the node-graph prototype tests"
    make -C "$PROJECT_ROOT" test_node >/dev/null
    "$PROJECT_ROOT/test_node"
}

clean() {
    info "cleaning"
    rm -rf "$BUILD_DIR" "$PROJECT_ROOT/test_node"
    rm -f "$PROJECT_ROOT"/graph/*.o "$PROJECT_ROOT"/kernel/*.o
    (cd "$KERNEL_DIR" && cargo clean >/dev/null 2>&1 || true)
    info "done"
}

usage() {
    sed -n '3,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

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
        require_qemu
        get_limine
        build_kernel
        create_iso
        run_bios
        ;;
    run-uefi)
        check_deps
        require_qemu
        get_limine
        build_kernel
        create_iso
        run_uefi
        ;;
    test)
        run_node_test
        ;;
    clean)
        clean
        ;;
    help | --help | -h)
        usage
        ;;
    *)
        die "unknown command: $1 (try 'help')"
        ;;
esac
