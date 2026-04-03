//! Struct field access + the ownership gate (set_owned).

use crate::tags;
use crate::alloc;

pub fn get_f64(ptr: i64, idx: i64) -> f64 {
    if ptr == 0 { return 0.0; }
    unsafe { &*(ptr as *const Vec<i64>) }
        .get(idx as usize).map(|&b| f64::from_bits(b as u64)).unwrap_or(0.0)
}

pub fn set_f64(ptr: i64, idx: i64, val: f64) {
    if ptr == 0 { return; }
    if let Some(slot) = unsafe { &mut *(ptr as *mut Vec<i64>) }.get_mut(idx as usize) {
        *slot = val.to_bits() as i64;
    }
}

pub fn get_ptr(ptr: i64, idx: i64) -> i64 {
    if ptr == 0 { return 0; }
    unsafe { &*(ptr as *const Vec<i64>) }.get(idx as usize).copied().unwrap_or(0)
}

/// Store a heap value into a struct field with ownership semantics.
/// Tracked (owned) → move. Untracked (borrowed) → copy.
pub fn set_owned(ptr: i64, idx: i64, val: i64) {
    if ptr == 0 || val == 0 { return; }
    let stored = if tags::is_tracked(val) { val } else { alloc::string_new(val) };
    if let Some(slot) = unsafe { &mut *(ptr as *mut Vec<i64>) }.get_mut(idx as usize) {
        *slot = stored;
    }
}
