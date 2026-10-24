#!/usr/bin/env python3
"""Headless boot test for Vertex OS.

Boots the ISO in QEMU, drives the shell over the serial console, saves the
transcript and (optionally) a screenshot of the framebuffer.

    ./tools/smoke-test.py build/vertex-os.iso --screenshot artifacts/screen.png
"""

import argparse
import os
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import zlib

BOOT_MARKER = b"Type 'help' for commands."


def convert_ppm_to_png(ppm_path: str, png_path: str) -> None:
    """Minimal P6 PPM -> PNG converter (no third-party dependencies)."""
    with open(ppm_path, "rb") as handle:
        data = handle.read()

    # P6\n<width> <height>\n<maxval>\n<binary>
    fields = []
    index = 0
    while len(fields) < 4:
        while index < len(data) and data[index : index + 1].isspace():
            index += 1
        if data[index : index + 1] == b"#":
            while data[index : index + 1] not in (b"\n", b""):
                index += 1
            continue
        start = index
        while index < len(data) and not data[index : index + 1].isspace():
            index += 1
        fields.append(data[start:index])
    index += 1  # single whitespace after maxval

    magic, width, height, maxval = fields
    if magic != b"P6" or int(maxval) != 255:
        raise ValueError("unexpected PPM format")
    width, height = int(width), int(height)

    pixels = data[index : index + width * height * 3]
    raw = bytearray()
    for row in range(height):
        raw.append(0)  # filter type: none
        start = row * width * 3
        raw.extend(pixels[start : start + width * 3])

    def chunk(kind: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + kind
            + payload
            + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    with open(png_path, "wb") as handle:
        handle.write(b"\x89PNG\r\n\x1a\n")
        handle.write(chunk(b"IHDR", header))
        handle.write(chunk(b"IDAT", zlib.compress(bytes(raw), 9)))
        handle.write(chunk(b"IEND", b""))


KEY_NAMES = {
    " ": "spc",
    "\n": "ret",
    "\r": "ret",
    "\x1b": "esc",
    "\t": "tab",
    "\x08": "backspace",
    "-": "minus",
    "=": "equal",
    ",": "comma",
    ".": "dot",
    "/": "slash",
    ";": "semicolon",
    "'": "apostrophe",
}

# Named keys for GUI navigation, so a test can say "down, down, ret".
KEY_ALIASES = {
    "up": "up",
    "down": "down",
    "left": "left",
    "right": "right",
    "enter": "ret",
    "esc": "esc",
    "tab": "tab",
}


def monitor_client(monitor_path: str):
    if not os.path.exists(monitor_path):
        return None
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(5)
    client.connect(monitor_path)
    return client


def type_on_keyboard(monitor_path: str, keys: str) -> bool:
    """Inject keystrokes through the emulated 8042 controller.

    Exercises the real PS/2 interrupt path rather than the serial console.
    Named keys can be written as `{down}` and are sent as navigation keys, so a
    test can drive the graphical front end: `{down}{enter}`.
    """
    client = monitor_client(monitor_path)
    if client is None:
        return False

    sequence = []
    index = 0
    while index < len(keys):
        if keys[index] == "{":
            end = keys.find("}", index)
            if end != -1:
                name = keys[index + 1 : end].lower()
                if name in KEY_ALIASES:
                    sequence.append(KEY_ALIASES[name])
                    index = end + 1
                    continue
        char = keys[index]
        sequence.append(KEY_NAMES.get(char, char.lower()))
        index += 1

    for name in sequence:
        client.sendall(f"sendkey {name}\n".encode())
        time.sleep(0.15)
    time.sleep(1.0)
    client.close()
    return True


def screenshot(monitor_path: str, png_path: str) -> bool:
    if not os.path.exists(monitor_path):
        return False
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(5)
    client.connect(monitor_path)
    ppm = os.path.join(tempfile.gettempdir(), "vertex-screendump.ppm")
    if os.path.exists(ppm):
        os.unlink(ppm)
    client.sendall(f"screendump {ppm}\n".encode())
    for _ in range(50):
        time.sleep(0.1)
        if os.path.exists(ppm) and os.path.getsize(ppm) > 1024:
            break
    client.close()
    if not os.path.exists(ppm):
        return False
    os.makedirs(os.path.dirname(png_path) or ".", exist_ok=True)
    convert_ppm_to_png(ppm, png_path)
    return True


def ovmf_arguments(ovmf: str, workdir: str) -> list:
    """QEMU arguments for an OVMF code image.

    4 MiB images are laid out for pflash: they start with padding, so `-bios`
    rejects them. Attach the code image read-only and a writable copy of the
    matching vars image, which keeps the system files untouched.
    """
    size = os.path.getsize(ovmf)
    if size >= 4 * 1024 * 1024 or "4M" in os.path.basename(ovmf):
        arguments = [
            "-drive",
            f"if=pflash,format=raw,unit=0,readonly=on,file={ovmf}",
        ]
        vars_path = os.path.join(
            os.path.dirname(ovmf), os.path.basename(ovmf).replace("CODE", "VARS")
        )
        if os.path.exists(vars_path):
            local_vars = os.path.join(workdir, "OVMF_VARS.fd")
            shutil.copyfile(vars_path, local_vars)
            arguments += [
                "-drive",
                f"if=pflash,format=raw,unit=1,file={local_vars}",
            ]
        return arguments
    return ["-bios", ovmf]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("iso", nargs="?", default="build/vertex-os.iso")
    parser.add_argument("--qemu", default=os.environ.get("QEMU", "qemu-system-x86_64"))
    parser.add_argument("--uefi", action="store_true", help="boot with OVMF instead of SeaBIOS")
    parser.add_argument("--ovmf", default=None)
    parser.add_argument("--screenshot", default=None)
    parser.add_argument(
        "--settle",
        type=float,
        default=0.0,
        help="seconds to wait before the screenshot (lets toasts expire)",
    )
    parser.add_argument(
        "--keys",
        default=None,
        help="text to type on the emulated PS/2 keyboard (ignores --commands); "
        "use {down}, {enter} and {esc} for navigation keys",
    )
    parser.add_argument("--timeout", type=int, default=40)
    parser.add_argument(
        "--marker",
        default=BOOT_MARKER.decode(),
        help="serial text that marks a successful boot",
    )
    parser.add_argument(
        "--expect",
        action="append",
        default=[],
        metavar="TEXT",
        help="assert the serial transcript contains TEXT (repeatable)",
    )
    parser.add_argument(
        "--commands",
        default="help|ls /|cat /readme.txt|write /notes.txt hello from vertex os|"
        "history /notes.txt|dedup|tasks|info",
        help="commands to run, separated by '|'",
    )
    parser.add_argument("--log", default=None)
    args = parser.parse_args()

    if not os.path.exists(args.iso):
        print(f"error: ISO not found: {args.iso}", file=sys.stderr)
        return 2

    workdir = tempfile.mkdtemp(prefix="vertex-smoke-")
    monitor = os.path.join(workdir, "monitor.sock")

    cmd = [
        args.qemu,
        "-M", "q35",
        "-m", "512M",
        "-cdrom", args.iso,
        "-serial", "stdio",
        "-display", "none",
        "-no-reboot",
        "-monitor", f"unix:{monitor},server,nowait",
    ]
    if os.path.exists("/dev/kvm") and os.access("/dev/kvm", os.W_OK):
        cmd[1:1] = ["-enable-kvm", "-cpu", "host"]
    # Non-standard QEMU installations (Homebrew, locally extracted packages)
    # keep firmware and ROM images outside QEMU's compiled-in data dir.
    data_dir = os.environ.get("QEMU_DATA_DIR")
    if data_dir:
        cmd[1:1] = ["-L", data_dir]
    if args.uefi:
        ovmf = args.ovmf
        if not ovmf:
            for candidate in (
                "/usr/share/OVMF/OVMF_CODE.fd",
                "/usr/share/OVMF/OVMF_CODE_4M.fd",
                "/usr/share/edk2/ovmf/OVMF_CODE.fd",
                "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd",
            ):
                if os.path.exists(candidate):
                    ovmf = candidate
                    break
        if not ovmf:
            print("error: OVMF firmware not found", file=sys.stderr)
            return 2
        cmd[1:1] = ovmf_arguments(ovmf, workdir)

    log_path = args.log or os.path.join(workdir, "serial.log")
    log_dir = os.path.dirname(log_path)
    if log_dir:
        os.makedirs(log_dir, exist_ok=True)
    log = open(log_path, "wb")
    process = subprocess.Popen(
        cmd, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT
    )
    stdin = process.stdin

    marker = args.marker.encode()
    booted = False
    deadline = time.time() + args.timeout
    while time.time() < deadline:
        time.sleep(0.5)
        if process.poll() is not None:
            break
        try:
            with open(log_path, "rb") as handle:
                if marker in handle.read():
                    booted = True
                    break
        except FileNotFoundError:
            continue

    if not booted:
        process.terminate()
        time.sleep(0.5)
        process.kill()
        log.close()
        print(f"FAIL: the kernel never printed {args.marker!r}", file=sys.stderr)
        with open(log_path, "rb") as handle:
            sys.stderr.write(handle.read().decode("utf-8", "replace"))
        return 1

    time.sleep(0.5)
    if args.keys:
        if not type_on_keyboard(monitor, args.keys):
            print("warning: could not type on the emulated keyboard", file=sys.stderr)
        time.sleep(1.2)

    # Commands run after any keyboard input, so `--keys {esc}` drops to the text
    # console and the commands then exercise the shell REPL.
    ran_commands = 0
    if args.commands:
        for command in filter(None, args.commands.split("|")):
            stdin.write(command.encode() + b"\r")
            stdin.flush()
            ran_commands += 1
            time.sleep(0.8)
        time.sleep(1.0)

    if args.screenshot:
        if args.settle > 0:
            time.sleep(args.settle)
        if screenshot(monitor, args.screenshot):
            print(f"screenshot: {args.screenshot}")
        else:
            print("warning: screenshot capture failed", file=sys.stderr)

    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
    log.close()

    with open(log_path, "rb") as handle:
        transcript = handle.read().decode("utf-8", "replace")
    print(transcript)

    missing = [text for text in args.expect if text not in transcript]
    if missing:
        print(f"FAIL: expected output missing from the serial log: {missing}", file=sys.stderr)
        return 1

    if args.keys:
        print(f"PASS: booted and typed {args.keys!r} on the PS/2 keyboard")
    if ran_commands:
        print(f"PASS: booted and ran {ran_commands} commands")
    if args.expect:
        print(f"PASS: serial log contains all {len(args.expect)} expected strings")
    return 0


if __name__ == "__main__":
    sys.exit(main())
