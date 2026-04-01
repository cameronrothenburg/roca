//! Roca runtime — host implementations for JIT-compiled code.
//! Provides stdlib functions, memory management (RC), and the memory tracker.

pub mod stdlib;
pub use stdlib::*;

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};

// ─── String helpers ─────────────────────────────────

pub fn read_cstr(ptr: i64) -> &'static str {
    if ptr == 0 { return ""; }
    unsafe { CStr::from_ptr(ptr as *const i8) }.to_str().unwrap_or("")
}

pub fn alloc_str(s: &str) -> i64 {
    let bytes = format!("{}\0", s);
    let ptr = roca_rc_alloc(bytes.len() as i64);
    if ptr == 0 { return 0; }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
    }
    if MEM.is_debug() {
        eprintln!("  [mem] alloc_str \"{}\" -> {:#x}", s, ptr);
    }
    ptr
}

// ─── Memory tracking ─────────────────────────────────
//
// RC heap layout:
//   [refcount: i64][total_size: i64][payload bytes...]
//   ^ptr points here
//
// Payload is accessed at ptr + 16.

/// Thread-local memory tracker — each test thread has its own counters.
/// This eliminates race conditions between parallel tests.
pub struct MemTracker {
    pub debug: AtomicBool,
}

thread_local! {
    static TL_ALLOCS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TL_FREES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TL_RETAINS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TL_RELEASES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TL_LIVE_BYTES: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

pub static MEM: MemTracker = MemTracker {
    debug: AtomicBool::new(false),
};

impl MemTracker {
    pub fn set_debug(&self, on: bool) {
        self.debug.store(on, Ordering::SeqCst);
    }

    pub fn is_debug(&self) -> bool {
        self.debug.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        TL_ALLOCS.with(|c| c.set(0));
        TL_FREES.with(|c| c.set(0));
        TL_RETAINS.with(|c| c.set(0));
        TL_RELEASES.with(|c| c.set(0));
        TL_LIVE_BYTES.with(|c| c.set(0));
    }

    pub fn stats(&self) -> (u64, u64, u64, u64, i64) {
        (
            TL_ALLOCS.with(|c| c.get()),
            TL_FREES.with(|c| c.get()),
            TL_RETAINS.with(|c| c.get()),
            TL_RELEASES.with(|c| c.get()),
            TL_LIVE_BYTES.with(|c| c.get()),
        )
    }

    #[allow(dead_code)]
    pub fn assert_clean(&self) {
        let (allocs, frees, ..) = self.stats();
        assert_eq!(allocs, frees, "memory leak: {} allocs but only {} frees", allocs, frees);
        let live = TL_LIVE_BYTES.with(|c| c.get());
        assert_eq!(live, 0, "live bytes should be 0 but got {}", live);
    }

    pub fn track_alloc(&self, bytes: i64) {
        TL_ALLOCS.with(|c| c.set(c.get() + 1));
        TL_LIVE_BYTES.with(|c| c.set(c.get() + bytes));
    }

    pub fn track_free(&self, bytes: i64) {
        TL_FREES.with(|c| c.set(c.get() + 1));
        TL_LIVE_BYTES.with(|c| c.set(c.get() - bytes));
    }

    pub fn track_retain(&self) {
        TL_RETAINS.with(|c| c.set(c.get() + 1));
    }

    pub fn track_release(&self) {
        TL_RELEASES.with(|c| c.set(c.get() + 1));
    }
}

// RC layout: [refcount: i64][total_size: i64][payload...]
// User code receives a pointer to PAYLOAD (header + 16).
// RC functions back up 16 bytes to find the header.
const RC_HEADER_SIZE: usize = 16;

/// Get header pointer from payload pointer
fn rc_header(payload_ptr: i64) -> *mut i64 {
    unsafe { (payload_ptr as *mut u8).sub(RC_HEADER_SIZE) as *mut i64 }
}

// ─── Constraint validation ──────────────────────────

thread_local! {
    static TL_CONSTRAINT_VIOLATED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[allow(dead_code)]
pub fn reset_constraint_violated() {
    TL_CONSTRAINT_VIOLATED.with(|c| c.set(false));
}

#[allow(dead_code)]
pub fn constraint_violated() -> bool {
    TL_CONSTRAINT_VIOLATED.with(|c| c.get())
}

pub extern "C" fn roca_constraint_panic(msg: i64) {
    let s = read_cstr(msg);
    eprintln!("constraint violation: {}", s);
    TL_CONSTRAINT_VIOLATED.with(|c| c.set(true));
}

// ─── Struct operations ───────────────────────────────

pub extern "C" fn roca_struct_alloc(num_fields: i64) -> i64 {
    let size = 24 + num_fields as i64 * 8;
    let ptr = Box::into_raw(Box::new(vec![0i64; num_fields as usize])) as i64;
    MEM.track_alloc(size);
    ptr
}

pub extern "C" fn roca_struct_set_f64(ptr: i64, idx: i64, val: f64) {
    if ptr == 0 { return; }
    if let Some(slot) = unsafe { &mut *(ptr as *mut Vec<i64>) }.get_mut(idx as usize) {
        *slot = val.to_bits() as i64;
    }
}

pub extern "C" fn roca_struct_get_f64(ptr: i64, idx: i64) -> f64 {
    if ptr == 0 { return 0.0; }
    unsafe { &*(ptr as *const Vec<i64>) }.get(idx as usize).map(|&b| f64::from_bits(b as u64)).unwrap_or(0.0)
}

pub extern "C" fn roca_struct_set_ptr(ptr: i64, idx: i64, val: i64) {
    if ptr == 0 { return; }
    if let Some(slot) = unsafe { &mut *(ptr as *mut Vec<i64>) }.get_mut(idx as usize) {
        *slot = val;
    }
}

pub extern "C" fn roca_struct_get_ptr(ptr: i64, idx: i64) -> i64 {
    if ptr == 0 { return 0; }
    unsafe { &*(ptr as *const Vec<i64>) }.get(idx as usize).copied().unwrap_or(0)
}

// ─── Conversion ──────────────────────────────────────

pub extern "C" fn roca_f64_to_bool(n: f64) -> u8 {
    if n == 0.0 { 0 } else { 1 }
}

// ─── Memory management ──────────────────────────────

/// Create an RC'd string from a static C string pointer.
pub extern "C" fn roca_string_new(static_ptr: i64) -> i64 {
    if static_ptr == 0 { return 0; }
    let s = read_cstr(static_ptr);
    alloc_str(s)
}

/// Allocate a block with RC header. Returns pointer to PAYLOAD.
pub extern "C" fn roca_rc_alloc(payload_size: i64) -> i64 {
    let total = RC_HEADER_SIZE + payload_size as usize;
    let layout = std::alloc::Layout::from_size_align(total, 8).unwrap();
    let base = unsafe { std::alloc::alloc_zeroed(layout) };
    if base.is_null() { return 0; }
    unsafe {
        *(base as *mut i64) = 1;                      // refcount = 1
        *((base as *mut i64).add(1)) = total as i64;  // total size
    }
    MEM.track_alloc(total as i64);
    unsafe { base.add(RC_HEADER_SIZE) as i64 }
}

/// Increment refcount. Takes payload pointer.
pub extern "C" fn roca_rc_retain(ptr: i64) {
    if ptr == 0 { return; }
    let header = rc_header(ptr);
    unsafe { *header += 1; }
    let new_rc = unsafe { *header };
    MEM.track_retain();
    if MEM.is_debug() {
        let s = read_cstr(ptr);
        eprintln!("  [mem] retain {:#x} rc={} \"{}\"", ptr, new_rc, s);
    }
}

/// Free an array (Vec<i64>) and track it.
pub extern "C" fn roca_free_array(ptr: i64) {
    if ptr == 0 { return; }
    if MEM.is_debug() { eprintln!("  [mem] free_array {:#x}", ptr); }
    let v = unsafe { Box::from_raw(ptr as *mut Vec<i64>) };
    drop(v);
    MEM.track_free(32);
}

/// Free a struct (Vec<i64>). Releases the first n_heap_fields slots as RC pointers.
pub extern "C" fn roca_free_struct(ptr: i64, n_heap_fields: i64) {
    if ptr == 0 { return; }
    if MEM.is_debug() { eprintln!("  [mem] free_struct {:#x} (heap_fields={})", ptr, n_heap_fields); }
    let v = unsafe { &*(ptr as *const Vec<i64>) };
    for i in 0..(n_heap_fields as usize).min(v.len()) {
        let field_ptr = v[i];
        if field_ptr != 0 {
            roca_rc_release(field_ptr);
        }
    }
    let v = unsafe { Box::from_raw(ptr as *mut Vec<i64>) };
    let size = 24 + v.len() as i64 * 8;
    drop(v);
    MEM.track_free(size);
}

/// Decrement refcount. Free if zero. Takes payload pointer.
pub extern "C" fn roca_rc_release(ptr: i64) {
    if ptr == 0 { return; }
    MEM.track_release();
    let header = rc_header(ptr);
    let rc = unsafe { &mut *header };
    *rc -= 1;
    if MEM.is_debug() {
        let s = read_cstr(ptr);
        eprintln!("  [mem] release {:#x} rc={} \"{}\" {}", ptr, *rc, s, if *rc <= 0 { "→ FREE" } else { "" });
    }
    if *rc <= 0 {
        let total_size = unsafe { *header.add(1) } as usize;
        if total_size < RC_HEADER_SIZE {
            MEM.track_free(0);
            return;
        }
        MEM.track_free(total_size as i64);
        let base = unsafe { (ptr as *mut u8).sub(RC_HEADER_SIZE) };
        let layout = std::alloc::Layout::from_size_align(total_size, 8).unwrap();
        unsafe { std::alloc::dealloc(base, layout); }
    }
}

// roca_box_alloc, roca_box_free, and roca_free_json_array are in stdlib.rs
// (they use the proper box header layout with drop trampolines)
