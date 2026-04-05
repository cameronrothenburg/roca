//! Memory diagnostics — alloc/free counting and leak detection.

use std::cell::Cell;

thread_local! {
    static TL_ALLOCS: Cell<u64> = const { Cell::new(0) };
    static TL_FREES: Cell<u64> = const { Cell::new(0) };
    static TL_LIVE_BYTES: Cell<i64> = const { Cell::new(0) };
}

pub fn track_alloc(bytes: i64) {
    TL_ALLOCS.with(|c| c.set(c.get() + 1));
    TL_LIVE_BYTES.with(|c| c.set(c.get() + bytes));
}

pub fn track_free(bytes: i64) {
    TL_FREES.with(|c| c.set(c.get() + 1));
    TL_LIVE_BYTES.with(|c| c.set(c.get() - bytes));
}

pub fn stats() -> (u64, u64, i64) {
    (TL_ALLOCS.with(|c| c.get()), TL_FREES.with(|c| c.get()), TL_LIVE_BYTES.with(|c| c.get()))
}

pub fn reset() {
    TL_ALLOCS.with(|c| c.set(0));
    TL_FREES.with(|c| c.set(0));
    TL_LIVE_BYTES.with(|c| c.set(0));
}

pub fn assert_clean() {
    let (allocs, frees, live) = stats();
    assert_eq!(allocs, frees, "memory leak: {} allocs but {} frees", allocs, frees);
    assert_eq!(live, 0, "live bytes should be 0 but got {}", live);
}
