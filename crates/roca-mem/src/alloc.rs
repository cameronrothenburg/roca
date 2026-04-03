//! Allocation functions — string, struct, array, map, copy.

use std::ffi::CStr;
use crate::tags::{self, TAG_STRING, TAG_STRUCT, TAG_VEC, TAG_MAP};
use crate::tracker;

pub fn alloc_str(s: &str) -> i64 {
    let bytes = format!("{}\0", s);
    let total = bytes.len() as i64;
    let layout = std::alloc::Layout::from_size_align(bytes.len(), 8).unwrap();
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() { return 0; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), base, bytes.len()); }
    let ptr = base as i64;
    tags::tag_alloc(ptr, TAG_STRING, total);
    tracker::track_alloc(total);
    ptr
}

pub fn read_cstr(ptr: i64) -> &'static str {
    if ptr == 0 { return ""; }
    unsafe { CStr::from_ptr(ptr as *const i8) }.to_str().unwrap_or("")
}

pub fn string_new(src: i64) -> i64 {
    if src == 0 { return 0; }
    alloc_str(read_cstr(src))
}

pub fn struct_new(num_fields: i64, type_id: i64) -> i64 {
    let size = 24 + num_fields * 8;
    let ptr = Box::into_raw(Box::new(vec![0i64; num_fields as usize])) as i64;
    tags::tag_alloc(ptr, TAG_STRUCT, size);
    tags::set_type_id(ptr, type_id as u16);
    tracker::track_alloc(size);
    ptr
}

pub fn array_new() -> i64 {
    let ptr = Box::into_raw(Box::new(Vec::<i64>::new())) as i64;
    tags::tag_alloc(ptr, TAG_VEC, 24);
    tracker::track_alloc(24);
    ptr
}

pub fn map_new() -> i64 {
    let ptr = Box::into_raw(Box::new(std::collections::HashMap::<String, i64>::new())) as i64;
    tags::tag_alloc(ptr, TAG_MAP, 48);
    tracker::track_alloc(48);
    ptr
}

pub fn copy(src: i64) -> i64 {
    if src == 0 { return 0; }
    let tag = tags::untag_alloc(src);
    if let Some((t, s)) = tag {
        tags::tag_alloc(src, t, s); // re-insert — copying, not consuming
        match t {
            TAG_STRING => alloc_str(read_cstr(src)),
            TAG_STRUCT => {
                let type_id = tags::get_type_id(src);
                let v = unsafe { &*(src as *const Vec<i64>) };
                let new_ptr = struct_new(v.len() as i64, type_id as i64);
                let new_v = unsafe { &mut *(new_ptr as *mut Vec<i64>) };
                for (i, &val) in v.iter().enumerate() {
                    if val != 0 && tags::is_tracked(val) {
                        new_v[i] = copy(val);
                    } else {
                        new_v[i] = val;
                    }
                }
                new_ptr
            }
            _ => 0,
        }
    } else {
        0
    }
}

pub fn name_to_type_id(name: &str) -> u16 {
    let mut hash: u32 = 5381;
    for b in name.bytes() { hash = hash.wrapping_mul(33).wrapping_add(b as u32); }
    (hash & 0xFFFF) as u16
}
