use crate::{alloc, free, struct_ops, tags, tracker};

fn setup() { tracker::reset(); }

#[test] fn string_alloc_and_free() { setup(); let p = alloc::alloc_str("hello"); assert!(tags::is_tracked(p)); free::mem_free(p); tracker::assert_clean(); }
#[test] fn struct_alloc_and_free() { setup(); let p = alloc::struct_new(3, 0); assert!(tags::is_tracked(p)); free::mem_free(p); tracker::assert_clean(); }
#[test] fn array_alloc_and_free() { setup(); let p = alloc::array_new(); assert!(tags::is_tracked(p)); free::mem_free(p); tracker::assert_clean(); }
#[test] fn map_alloc_and_free() { setup(); let p = alloc::map_new(); assert!(tags::is_tracked(p)); free::mem_free(p); tracker::assert_clean(); }

#[test] fn set_owned_moves_tracked_value() {
    setup();
    let s = alloc::struct_new(1, 0);
    let str_ptr = alloc::alloc_str("owned");
    struct_ops::set_owned(s, 0, str_ptr);
    free::mem_free(s);
    assert!(!tags::is_tracked(str_ptr));
    tracker::assert_clean();
}

#[test] fn set_owned_copies_untracked_value() {
    setup();
    let s = alloc::struct_new(1, 0);
    let raw = std::ffi::CString::new("borrowed").unwrap();
    let raw_ptr = raw.as_ptr() as i64;
    struct_ops::set_owned(s, 0, raw_ptr);
    let stored = struct_ops::get_ptr(s, 0);
    assert_ne!(stored, raw_ptr);
    assert!(tags::is_tracked(stored));
    assert_eq!(alloc::read_cstr(stored), "borrowed");
    free::mem_free(s);
    tracker::assert_clean();
}

#[test] fn set_owned_null_is_noop() { setup(); let s = alloc::struct_new(1, 0); struct_ops::set_owned(s, 0, 0); assert_eq!(struct_ops::get_ptr(s, 0), 0); free::mem_free(s); tracker::assert_clean(); }
#[test] fn struct_field_roundtrip_f64() { setup(); let s = alloc::struct_new(1, 0); struct_ops::set_f64(s, 0, 3.14); assert_eq!(struct_ops::get_f64(s, 0), 3.14); free::mem_free(s); tracker::assert_clean(); }
#[test] fn struct_field_roundtrip_ptr() { setup(); let s = alloc::struct_new(1, 0); let p = alloc::alloc_str("rt"); struct_ops::set_owned(s, 0, p); assert_eq!(alloc::read_cstr(struct_ops::get_ptr(s, 0)), "rt"); free::mem_free(s); tracker::assert_clean(); }

#[test] fn free_struct_frees_string_fields() { setup(); let s = alloc::struct_new(2, 0); struct_ops::set_owned(s, 0, alloc::alloc_str("a")); struct_ops::set_owned(s, 1, alloc::alloc_str("b")); free::mem_free(s); tracker::assert_clean(); }
#[test] fn free_struct_frees_nested_struct() { setup(); let inner = alloc::struct_new(1, 0); struct_ops::set_owned(inner, 0, alloc::alloc_str("deep")); let outer = alloc::struct_new(1, 0); struct_ops::set_owned(outer, 0, inner); free::mem_free(outer); tracker::assert_clean(); }
#[test] fn free_does_not_touch_untracked_children() { setup(); let s = alloc::struct_new(2, 0); struct_ops::set_f64(s, 0, 99.0); struct_ops::set_f64(s, 1, 1.0); free::mem_free(s); tracker::assert_clean(); }

#[test] fn double_free_is_noop() { setup(); let p = alloc::alloc_str("x"); free::mem_free(p); free::mem_free(p); tracker::assert_clean(); }
#[test] fn free_null_is_noop() { setup(); free::mem_free(0); let (a, f, _) = tracker::stats(); assert_eq!(a, 0); assert_eq!(f, 0); }
#[test] fn free_untracked_is_noop() { setup(); free::mem_free(0xDEAD_BEEF); let (a, f, _) = tracker::stats(); assert_eq!(a, 0); assert_eq!(f, 0); }

#[test] fn type_id_preserved_through_lifecycle() { setup(); let p = alloc::struct_new(1, 42); assert_eq!(tags::get_type_id(p), 42); free::mem_free(p); assert_eq!(tags::get_type_id(p), 0); tracker::assert_clean(); }
#[test] fn type_id_zero_for_non_structs() { setup(); let p = alloc::alloc_str("x"); assert_eq!(tags::get_type_id(p), 0); free::mem_free(p); tracker::assert_clean(); }

#[test] fn stats_count_allocs_and_frees() { setup(); let a = alloc::alloc_str("1"); let b = alloc::alloc_str("2"); let c = alloc::alloc_str("3"); let (al, fr, _) = tracker::stats(); assert_eq!(al, 3); assert_eq!(fr, 0); free::mem_free(a); free::mem_free(b); free::mem_free(c); tracker::assert_clean(); }
#[test] fn reset_zeroes_counters() { setup(); let _ = alloc::alloc_str("leak"); tracker::reset(); let (a, f, l) = tracker::stats(); assert_eq!((a, f, l), (0, 0, 0)); }
#[test] fn assert_clean_passes_when_balanced() { setup(); let p = alloc::alloc_str("ok"); free::mem_free(p); tracker::assert_clean(); }
#[test] #[should_panic(expected = "memory leak")] fn assert_clean_panics_on_leak() { setup(); let _ = alloc::alloc_str("leaked"); tracker::assert_clean(); }

#[test] fn name_to_type_id_deterministic() { assert_eq!(alloc::name_to_type_id("User"), alloc::name_to_type_id("User")); }
#[test] fn name_to_type_id_different_names_differ() { assert_ne!(alloc::name_to_type_id("User"), alloc::name_to_type_id("Point")); }
