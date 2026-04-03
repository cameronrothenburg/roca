//! roca-mem — Memory management for the Roca language.
//!
//! One crate owns all allocation, ownership, and cleanup.
//! `set_owned` enforces that containers always own their heap fields.

pub mod tags;
pub mod tracker;
pub mod alloc;
pub mod free;
pub mod struct_ops;

#[unsafe(no_mangle)] pub extern "C" fn mem_string_new(src: i64) -> i64 { alloc::string_new(src) }
#[unsafe(no_mangle)] pub extern "C" fn mem_struct_new(n: i64, t: i64) -> i64 { alloc::struct_new(n, t) }
#[unsafe(no_mangle)] pub extern "C" fn mem_array_new() -> i64 { alloc::array_new() }
#[unsafe(no_mangle)] pub extern "C" fn mem_map_new() -> i64 { alloc::map_new() }
#[unsafe(no_mangle)] pub extern "C" fn mem_struct_get_f64(p: i64, i: i64) -> f64 { struct_ops::get_f64(p, i) }
#[unsafe(no_mangle)] pub extern "C" fn mem_struct_set_f64(p: i64, i: i64, v: f64) { struct_ops::set_f64(p, i, v) }
#[unsafe(no_mangle)] pub extern "C" fn mem_struct_get_ptr(p: i64, i: i64) -> i64 { struct_ops::get_ptr(p, i) }
#[unsafe(no_mangle)] pub extern "C" fn mem_struct_set_owned(p: i64, i: i64, v: i64) { struct_ops::set_owned(p, i, v) }
#[unsafe(no_mangle)] pub extern "C" fn mem_free(ptr: i64) { free::mem_free(ptr) }
#[unsafe(no_mangle)] pub extern "C" fn mem_is_tracked(ptr: i64) -> bool { tags::is_tracked(ptr) }
#[unsafe(no_mangle)] pub extern "C" fn mem_type_id(ptr: i64) -> i64 { tags::get_type_id(ptr) as i64 }

pub use alloc::{alloc_str, read_cstr, name_to_type_id};
pub use tracker::{stats as mem_stats, reset as mem_reset, assert_clean as mem_assert_clean};

#[cfg(test)]
mod tests;
