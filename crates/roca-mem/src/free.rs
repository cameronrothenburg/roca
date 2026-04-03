//! Cleanup — recursive, tag-dispatch, idempotent.

use crate::tags::{self, TAG_STRING, TAG_VEC, TAG_MAP, TAG_STRUCT};
use crate::tracker;

pub fn mem_free(ptr: i64) {
    if ptr == 0 { return; }
    let (tag, size) = match tags::untag_alloc(ptr) {
        Some(t) => t,
        None => return,
    };
    match tag {
        TAG_STRING => {
            let layout = std::alloc::Layout::from_size_align(size as usize, 8).unwrap();
            unsafe { std::alloc::dealloc(ptr as *mut u8, layout); }
        }
        TAG_VEC | TAG_STRUCT => {
            if tag == TAG_STRUCT { tags::remove_type_id(ptr); }
            let v = unsafe { Box::from_raw(ptr as *mut Vec<i64>) };
            for &elem in v.iter() {
                if elem != 0 && tags::is_tracked(elem) { mem_free(elem); }
            }
            drop(v);
        }
        TAG_MAP => {
            let m = unsafe { Box::from_raw(ptr as *mut std::collections::HashMap<String, i64>) };
            for &val in m.values() {
                if val != 0 && tags::is_tracked(val) { mem_free(val); }
            }
            drop(m);
        }
        _ => {}
    }
    tracker::track_free(size);
}
