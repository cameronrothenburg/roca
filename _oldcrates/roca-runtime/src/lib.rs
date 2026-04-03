//! Host runtime for JIT-compiled Roca code — provides stdlib functions,
//! single-owner memory management, and the memory tracker.
//!
//! # Domain Boundary
//!
//! This crate owns **HOW** memory is freed — the implementation:
//! - Allocation tagging (`ALLOC_TAGS`): every heap value gets a tag on creation
//! - `roca_free`: reads the tag, dispatches by type, deallocates with correct
//!   layout, recursively frees children (e.g. strings inside arrays)
//! - `MemTracker`: thread-local alloc/free counters for leak detection in tests
//!
//! It does NOT own **WHEN** values are freed — that belongs in `roca-cranelift`.
//! Cranelift's Body decides scope exit, reassignment, temp flush timing.
//! This crate just provides `roca_free(ptr)` as a callable host function.
//!
//! Stdlib functions (string ops, array ops, JSON, HTTP, etc.) allocate
//! internally and tag their allocations. This is why the allocator lives
//! here — stdlib functions must allocate, and moving the allocator elsewhere
//! would create circular dependencies.
//!
//! This crate does NOT know about:
//! - Cranelift IR, function compilation, or module building
//! - Roca AST nodes, type checking, or language semantics
//! - Test orchestration or test runner logic (roca-native's domain)
//!
//! Tests here should verify individual stdlib functions and roca_free
//! correctness. Memory lifecycle tests (allocs == frees after running
//! compiled code) belong in roca-cranelift.
//!
//! # Memory Model
//!
//! Every heap value has one owner. When the owner goes away, the value is freed
//! via [`roca_free`]. No reference counting. No type dispatch at the IR level.
//!
//! The runtime tags each allocation so `roca_free` knows how to deallocate:
//! - Strings (raw bytes with null terminator)
//! - Arrays/Structs/Enums (`Vec<i64>` field slots)
//! - Maps (`HashMap<String, i64>`)
//! - Boxes (opaque extern types with drop trampolines)
//!
//! # Key exports
//!
//! - [`roca_free`] — free any heap value (reads tag, dispatches internally)
//! - **Struct ops** — [`roca_struct_alloc`], `roca_struct_get_*`, `roca_struct_set_*`
//! - **String helpers** — [`alloc_str`], [`read_cstr`]
//! - [`MEM`] / [`MemTracker`] — thread-local allocation counters for tests

pub mod stdlib;
pub use stdlib::*;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};

// ─── Allocation tags ─────────────────────────────────

const TAG_STRING: u8 = 1;
const TAG_VEC: u8 = 2;     // arrays, structs, enums — all Vec<i64>
const TAG_MAP: u8 = 3;
const TAG_BOX: u8 = 4;     // opaque extern types with drop trampoline

thread_local! {
    static ALLOC_TAGS: RefCell<HashMap<i64, (u8, i64)>> = RefCell::new(HashMap::new());
}

fn tag_alloc(ptr: i64, tag: u8, size: i64) {
    ALLOC_TAGS.with(|t| t.borrow_mut().insert(ptr, (tag, size)));
}

fn untag_alloc(ptr: i64) -> Option<(u8, i64)> {
    ALLOC_TAGS.with(|t| t.borrow_mut().remove(&ptr))
}

// ─── String helpers ─────────────────────────────────

pub fn read_cstr(ptr: i64) -> &'static str {
    if ptr == 0 { return ""; }
    unsafe { CStr::from_ptr(ptr as *const i8) }.to_str().unwrap_or("")
}

pub fn alloc_str(s: &str) -> i64 {
    let bytes = format!("{}\0", s);
    let total = bytes.len() as i64;
    let layout = std::alloc::Layout::from_size_align(bytes.len(), 8).unwrap();
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() { return 0; }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), base, bytes.len());
    }
    let ptr = base as i64;
    tag_alloc(ptr, TAG_STRING, total);
    MEM.track_alloc(total);
    if MEM.is_debug() {
        eprintln!("  [mem] alloc_str \"{}\" -> {:#x}", s, ptr);
    }
    ptr
}

// ─── Memory tracking ─────────────────────────────────

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
        self.debug.store(on, Ordering::Relaxed);
    }

    pub fn is_debug(&self) -> bool {
        self.debug.load(Ordering::Relaxed)
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

// ─── Unified free ────────────────────────────────────

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
    tag_alloc(ptr, TAG_VEC, size);
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

/// Create a heap string from a static C string pointer.
pub extern "C" fn roca_string_new(static_ptr: i64) -> i64 {
    if static_ptr == 0 { return 0; }
    let s = read_cstr(static_ptr);
    alloc_str(s)
}

/// Free any heap value. Reads the tag from the allocation registry
/// and dispatches to the appropriate deallocation path.
pub extern "C" fn roca_free(ptr: i64) {
    if ptr == 0 { return; }
    let (tag, size) = match untag_alloc(ptr) {
        Some(t) => t,
        None => {
            if MEM.is_debug() {
                eprintln!("  [mem] free {:#x} — untagged (already freed or static)", ptr);
            }
            return;
        }
    };
    if MEM.is_debug() {
        eprintln!("  [mem] free {:#x} tag={} size={}", ptr, tag, size);
    }
    match tag {
        TAG_STRING => {
            let layout = std::alloc::Layout::from_size_align(size as usize, 8).unwrap();
            unsafe { std::alloc::dealloc(ptr as *mut u8, layout); }
        }
        TAG_VEC => {
            let v = unsafe { Box::from_raw(ptr as *mut Vec<i64>) };
            for &elem in v.iter() {
                if elem != 0 {
                    let is_tracked = ALLOC_TAGS.with(|t| t.borrow().contains_key(&elem));
                    if is_tracked {
                        roca_free(elem);
                    }
                }
            }
            drop(v);
        }
        TAG_MAP => {
            let m = unsafe { Box::from_raw(ptr as *mut std::collections::HashMap<String, i64>) };
            for &val in m.values() {
                if val != 0 {
                    let is_tracked = ALLOC_TAGS.with(|t| t.borrow().contains_key(&val));
                    if is_tracked {
                        roca_free(val);
                    }
                }
            }
            drop(m);
        }
        TAG_BOX => {
            // Box header: [drop_fn: u64][total_size: u64] before payload
            unsafe {
                let base = (ptr as *mut u8).sub(BOX_HEADER);
                let drop_fn = *(base as *const u64);
                let total = *((base as *const u64).add(1)) as usize;
                if drop_fn != 0 {
                    let dropper: fn(*mut u8) = std::mem::transmute(drop_fn);
                    dropper(ptr as *mut u8);
                }
                let layout = std::alloc::Layout::from_size_align_unchecked(total, BOX_ALIGN);
                std::alloc::dealloc(base, layout);
            }
        }
        _ => {}
    }
    MEM.track_free(size);
}

// Legacy aliases — these call roca_free internally.
// Kept temporarily while cranelift migrates to __free.

// Box header constants — used by TAG_BOX deallocation path
pub(crate) const BOX_HEADER: usize = 16;
pub(crate) const BOX_ALIGN: usize = 16;
