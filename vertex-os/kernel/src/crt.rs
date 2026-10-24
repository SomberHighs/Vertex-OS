//! Minimal freestanding runtime support.
//!
//! `core` for `x86_64-unknown-none` does not ship `memcpy`/`memset`, so the
//! kernel must provide them: LLVM lowers slice copies and struct moves to these
//! symbols even in `no_std` code.

use core::ffi::{c_char, c_int, c_void};

#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, count: usize) -> *mut c_void {
    let dest = dest as *mut u8;
    let src = src as *const u8;
    let mut i = 0;
    while i < count {
        dest.add(i).write_volatile(src.add(i).read_volatile());
        i += 1;
    }
    dest as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, count: usize) -> *mut c_void {
    let dest_bytes = dest as *mut u8;
    let src_bytes = src as *const u8;
    if (dest as usize) < (src as usize) {
        let mut i = 0;
        while i < count {
            dest_bytes.add(i).write_volatile(src_bytes.add(i).read_volatile());
            i += 1;
        }
    } else {
        let mut i = count;
        while i > 0 {
            i -= 1;
            dest_bytes.add(i).write_volatile(src_bytes.add(i).read_volatile());
        }
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memset(dest: *mut c_void, value: c_int, count: usize) -> *mut c_void {
    let dest_bytes = dest as *mut u8;
    let byte = value as u8;
    let mut i = 0;
    while i < count {
        dest_bytes.add(i).write_volatile(byte);
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const c_void, b: *const c_void, count: usize) -> c_int {
    let a = a as *const u8;
    let b = b as *const u8;
    let mut i = 0;
    while i < count {
        let left = a.add(i).read_volatile();
        let right = b.add(i).read_volatile();
        if left != right {
            return left as c_int - right as c_int;
        }
        i += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let s = s as *const u8;
    let mut len = 0;
    while s.add(len).read_volatile() != 0 {
        len += 1;
    }
    len
}
