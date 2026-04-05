//! Allocation tag registry — tracks every live heap allocation.

use std::cell::RefCell;
use std::collections::HashMap;

pub const TAG_STRING: u8 = 1;
pub const TAG_VEC: u8 = 2;
pub const TAG_MAP: u8 = 3;
pub const TAG_BOX: u8 = 4;
pub const TAG_STRUCT: u8 = 5;

thread_local! {
    static ALLOC_TAGS: RefCell<HashMap<i64, (u8, i64)>> = RefCell::new(HashMap::new());
    static STRUCT_TYPE_IDS: RefCell<HashMap<i64, u16>> = RefCell::new(HashMap::new());
}

pub fn tag_alloc(ptr: i64, tag: u8, size: i64) {
    ALLOC_TAGS.with(|t| t.borrow_mut().insert(ptr, (tag, size)));
}

pub fn untag_alloc(ptr: i64) -> Option<(u8, i64)> {
    ALLOC_TAGS.with(|t| t.borrow_mut().remove(&ptr))
}

pub fn is_tracked(ptr: i64) -> bool {
    ALLOC_TAGS.with(|t| t.borrow().contains_key(&ptr))
}

pub fn set_type_id(ptr: i64, type_id: u16) {
    STRUCT_TYPE_IDS.with(|t| t.borrow_mut().insert(ptr, type_id));
}

pub fn get_type_id(ptr: i64) -> u16 {
    STRUCT_TYPE_IDS.with(|t| t.borrow().get(&ptr).copied().unwrap_or(0))
}

pub fn remove_type_id(ptr: i64) {
    STRUCT_TYPE_IDS.with(|t| t.borrow_mut().remove(&ptr));
}
