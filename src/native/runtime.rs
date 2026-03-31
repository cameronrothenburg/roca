//! Runtime function declarations and implementations.
//! Generated Cranelift code calls these Rust functions for string ops, I/O, etc.
//!
//! To add a new runtime function:
//! 1. Add one line to the `runtime_funcs!` table
//! 2. Implement the `extern "C" fn` below
//! That's it — the macro generates the struct, register, declare, and import.

use cranelift_codegen::ir::types;
use cranelift_jit::JITBuilder;
use cranelift_module::{Module, Linkage, FuncId};
use std::ffi::CStr;
use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};
use std::sync::Mutex;

/// Define all runtime functions in one place.
/// Format: (emit_key, symbol_name, fn_ptr, [param_types], [return_types])
macro_rules! runtime_funcs {
    (
        $( ($key:ident, $sym:expr, $ptr:expr, [$($p:expr),*], [$($r:expr),*]) ),* $(,)?
    ) => {
        pub struct RuntimeFuncs {
            $( pub $key: FuncId, )*
        }

        pub fn register_symbols(builder: &mut JITBuilder) {
            $( builder.symbol($sym, $ptr as *const u8); )*
        }

        pub fn declare_runtime<M: Module>(module: &mut M) -> RuntimeFuncs {
            RuntimeFuncs {
                $( $key: declare_fn(module, $sym, &[$($p),*], &[$($r),*]), )*
            }
        }

        impl RuntimeFuncs {
            pub fn import_all<M: Module>(
                &self,
                module: &mut M,
                func: &mut cranelift_codegen::ir::Function,
                compiled: &super::emit::CompiledFuncs,
            ) -> std::collections::HashMap<String, cranelift_codegen::ir::FuncRef> {
                let mut refs = std::collections::HashMap::new();
                $( refs.insert(
                    concat!("__", stringify!($key)).to_string(),
                    module.declare_func_in_func(self.$key, func),
                ); )*
                for (name, fid) in &compiled.funcs {
                    refs.insert(name.clone(), module.declare_func_in_func(*fid, func));
                }
                refs
            }
        }
    };
}

runtime_funcs! {
    // I/O
    (print,             "roca_print",             roca_print,             [types::I64],                                []),
    (print_f64,         "roca_print_f64",         roca_print_f64,         [types::F64],                                []),
    (print_bool,        "roca_print_bool",        roca_print_bool,        [types::I8],                                 []),

    // String core
    (string_eq,         "roca_string_eq",         roca_string_eq,         [types::I64, types::I64],                    [types::I8]),
    (string_concat,     "roca_string_concat",     roca_string_concat,     [types::I64, types::I64],                    [types::I64]),
    (string_len,        "roca_string_len",        roca_string_len,        [types::I64],                                [types::I64]),
    (string_from_f64,   "roca_string_from_f64",   roca_string_from_f64,   [types::F64],                                [types::I64]),

    // String methods
    (string_includes,   "roca_string_includes",   roca_string_includes,   [types::I64, types::I64],                    [types::I8]),
    (string_starts_with,"roca_string_starts_with",roca_string_starts_with,[types::I64, types::I64],                    [types::I8]),
    (string_ends_with,  "roca_string_ends_with",  roca_string_ends_with,  [types::I64, types::I64],                    [types::I8]),
    (string_trim,       "roca_string_trim",       roca_string_trim,       [types::I64],                                [types::I64]),
    (string_to_upper,   "roca_string_to_upper",   roca_string_to_upper,   [types::I64],                                [types::I64]),
    (string_to_lower,   "roca_string_to_lower",   roca_string_to_lower,   [types::I64],                                [types::I64]),
    (string_slice,      "roca_string_slice",      roca_string_slice,      [types::I64, types::I64, types::I64],        [types::I64]),
    (string_split,      "roca_string_split",      roca_string_split,      [types::I64, types::I64],                    [types::I64]),
    (string_char_at,    "roca_string_char_at",    roca_string_char_at,    [types::I64, types::I64],                    [types::I64]),
    (string_index_of,   "roca_string_index_of",   roca_string_index_of,   [types::I64, types::I64],                    [types::F64]),

    // Arrays
    (array_new,         "roca_array_new",         roca_array_new,         [],                                          [types::I64]),
    (array_push_f64,    "roca_array_push_f64",    roca_array_push_f64,    [types::I64, types::F64],                    []),
    (array_get_f64,     "roca_array_get_f64",     roca_array_get_f64,     [types::I64, types::I64],                    [types::F64]),
    (array_len,         "roca_array_len",         roca_array_len,         [types::I64],                                [types::I64]),
    (array_push_str,    "roca_array_push_str",    roca_array_push_str,    [types::I64, types::I64],                    []),
    (array_get_str,     "roca_array_get_str",     roca_array_get_str,     [types::I64, types::I64],                    [types::I64]),
    (array_join,        "roca_array_join",        roca_array_join,        [types::I64, types::I64],                    [types::I64]),

    // Structs
    (struct_alloc,      "roca_struct_alloc",      roca_struct_alloc,      [types::I64],                                [types::I64]),
    (struct_set_f64,    "roca_struct_set_f64",    roca_struct_set_f64,    [types::I64, types::I64, types::F64],        []),
    (struct_get_f64,    "roca_struct_get_f64",    roca_struct_get_f64,    [types::I64, types::I64],                    [types::F64]),
    (struct_set_ptr,    "roca_struct_set_ptr",    roca_struct_set_ptr,    [types::I64, types::I64, types::I64],        []),
    (struct_get_ptr,    "roca_struct_get_ptr",    roca_struct_get_ptr,    [types::I64, types::I64],                    [types::I64]),

    // Conversion
    (f64_to_bool,       "roca_f64_to_bool",       roca_f64_to_bool,       [types::F64],                                [types::I8]),

    // Memory management
    (string_new,        "roca_string_new",        roca_string_new,        [types::I64],                                [types::I64]),
    (rc_alloc,          "roca_rc_alloc",          roca_rc_alloc,          [types::I64],                                [types::I64]),
    (rc_retain,         "roca_rc_retain",         roca_rc_retain,         [types::I64],                                []),
    (rc_release,        "roca_rc_release",        roca_rc_release,        [types::I64],                                []),
    (free_array,        "roca_free_array",        roca_free_array,        [types::I64],                                []),
    (free_struct,       "roca_free_struct",       roca_free_struct,       [types::I64, types::I64],                    []),
}

fn declare_fn<M: Module>(module: &mut M, name: &str, params: &[cranelift_codegen::ir::Type], returns: &[cranelift_codegen::ir::Type]) -> FuncId {
    let mut sig = module.make_signature();
    for &p in params { sig.params.push(cranelift_codegen::ir::AbiParam::new(p)); }
    for &r in returns { sig.returns.push(cranelift_codegen::ir::AbiParam::new(r)); }
    module.declare_function(name, Linkage::Import, &sig).unwrap()
}

// ─── Helpers ──────────────────────────────────────────

fn read_cstr(ptr: i64) -> &'static str {
    if ptr == 0 { return ""; }
    unsafe { CStr::from_ptr(ptr as *const i8) }.to_str().unwrap_or("")
}

fn alloc_str(s: &str) -> i64 {
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

// ─── Implementations ─────────────────────────────────

extern "C" fn roca_print(s: i64) {
    if s == 0 { println!("null"); return; }
    let cstr = unsafe { CStr::from_ptr(s as *const u8 as *const i8) };
    if let Ok(s) = cstr.to_str() { println!("{}", s); }
}

extern "C" fn roca_print_f64(n: f64) {
    if n.fract() == 0.0 && n.abs() < 1e15 { println!("{}", n as i64); }
    else { println!("{}", n); }
}

extern "C" fn roca_print_bool(v: u8) {
    println!("{}", if v != 0 { "true" } else { "false" });
}

extern "C" fn roca_string_eq(a: i64, b: i64) -> u8 {
    if a == b { return 1; }
    if a == 0 || b == 0 { return 0; }
    let a_str = unsafe { CStr::from_ptr(a as *const i8) };
    let b_str = unsafe { CStr::from_ptr(b as *const i8) };
    if a_str == b_str { 1 } else { 0 }
}

extern "C" fn roca_string_concat(a: i64, b: i64) -> i64 {
    let combined = format!("{}{}", read_cstr(a), read_cstr(b));
    alloc_str(&combined)
}

extern "C" fn roca_string_len(s: i64) -> i64 {
    if s == 0 { return 0; }
    unsafe { CStr::from_ptr(s as *const i8) }.to_bytes().len() as i64
}

extern "C" fn roca_string_from_f64(n: f64) -> i64 {
    if n.fract() == 0.0 && n.abs() < 1e15 { alloc_str(&format!("{}", n as i64)) }
    else { alloc_str(&format!("{}", n)) }
}

extern "C" fn roca_f64_to_bool(n: f64) -> u8 {
    if n == 0.0 { 0 } else { 1 }
}

extern "C" fn roca_string_includes(haystack: i64, needle: i64) -> u8 {
    if read_cstr(haystack).contains(read_cstr(needle)) { 1 } else { 0 }
}

extern "C" fn roca_string_starts_with(s: i64, prefix: i64) -> u8 {
    if read_cstr(s).starts_with(read_cstr(prefix)) { 1 } else { 0 }
}

extern "C" fn roca_string_ends_with(s: i64, suffix: i64) -> u8 {
    if read_cstr(s).ends_with(read_cstr(suffix)) { 1 } else { 0 }
}

extern "C" fn roca_string_trim(s: i64) -> i64 { alloc_str(read_cstr(s).trim()) }
extern "C" fn roca_string_to_upper(s: i64) -> i64 { alloc_str(&read_cstr(s).to_uppercase()) }
extern "C" fn roca_string_to_lower(s: i64) -> i64 { alloc_str(&read_cstr(s).to_lowercase()) }

extern "C" fn roca_string_slice(s: i64, start: i64, end: i64) -> i64 {
    let text = read_cstr(s);
    let start = (start as usize).min(text.len());
    let end = (end as usize).min(text.len());
    if start >= end { return alloc_str(""); }
    alloc_str(&text[start..end])
}

extern "C" fn roca_string_split(s: i64, delim: i64) -> i64 {
    let parts: Vec<i64> = read_cstr(s).split(read_cstr(delim)).map(|p| alloc_str(p)).collect();
    Box::into_raw(Box::new(parts)) as i64
}

extern "C" fn roca_string_char_at(s: i64, idx: i64) -> i64 {
    read_cstr(s).chars().nth(idx as usize)
        .map(|c| alloc_str(&c.to_string()))
        .unwrap_or_else(|| alloc_str(""))
}

extern "C" fn roca_string_index_of(haystack: i64, needle: i64) -> f64 {
    read_cstr(haystack).find(read_cstr(needle)).map(|i| i as f64).unwrap_or(-1.0)
}

extern "C" fn roca_array_new() -> i64 {
    let ptr = Box::into_raw(Box::new(Vec::<i64>::new())) as i64;
    MEM.allocs.fetch_add(1, Ordering::SeqCst);
    MEM.live_bytes.fetch_add(32, Ordering::SeqCst); // approximate Vec overhead
    ptr
}

extern "C" fn roca_array_push_f64(arr: i64, val: f64) {
    if arr == 0 { return; }
    unsafe { &mut *(arr as *mut Vec<i64>) }.push(val.to_bits() as i64);
}

extern "C" fn roca_array_get_f64(arr: i64, idx: i64) -> f64 {
    if arr == 0 { return 0.0; }
    unsafe { &*(arr as *const Vec<i64>) }.get(idx as usize).map(|&b| f64::from_bits(b as u64)).unwrap_or(0.0)
}

extern "C" fn roca_array_len(arr: i64) -> i64 {
    if arr == 0 { return 0; }
    unsafe { &*(arr as *const Vec<i64>) }.len() as i64
}

extern "C" fn roca_array_push_str(arr: i64, val: i64) {
    if arr == 0 { return; }
    unsafe { &mut *(arr as *mut Vec<i64>) }.push(val);
}

extern "C" fn roca_array_get_str(arr: i64, idx: i64) -> i64 {
    if arr == 0 { return 0; }
    unsafe { &*(arr as *const Vec<i64>) }.get(idx as usize).copied().unwrap_or(0)
}

extern "C" fn roca_array_join(arr: i64, sep: i64) -> i64 {
    if arr == 0 { return alloc_str(""); }
    let v = unsafe { &*(arr as *const Vec<i64>) };
    let parts: Vec<&str> = v.iter().map(|&ptr| read_cstr(ptr)).collect();
    alloc_str(&parts.join(read_cstr(sep)))
}

extern "C" fn roca_struct_alloc(num_fields: i64) -> i64 {
    let size = 24 + num_fields as i64 * 8; // Vec overhead + fields
    let ptr = Box::into_raw(Box::new(vec![0i64; num_fields as usize])) as i64;
    MEM.allocs.fetch_add(1, Ordering::SeqCst);
    MEM.live_bytes.fetch_add(size, Ordering::SeqCst);
    ptr
}

extern "C" fn roca_struct_set_f64(ptr: i64, idx: i64, val: f64) {
    if ptr == 0 { return; }
    if let Some(slot) = unsafe { &mut *(ptr as *mut Vec<i64>) }.get_mut(idx as usize) {
        *slot = val.to_bits() as i64;
    }
}

extern "C" fn roca_struct_get_f64(ptr: i64, idx: i64) -> f64 {
    if ptr == 0 { return 0.0; }
    unsafe { &*(ptr as *const Vec<i64>) }.get(idx as usize).map(|&b| f64::from_bits(b as u64)).unwrap_or(0.0)
}

extern "C" fn roca_struct_set_ptr(ptr: i64, idx: i64, val: i64) {
    if ptr == 0 { return; }
    if let Some(slot) = unsafe { &mut *(ptr as *mut Vec<i64>) }.get_mut(idx as usize) {
        *slot = val;
    }
}

extern "C" fn roca_struct_get_ptr(ptr: i64, idx: i64) -> i64 {
    if ptr == 0 { return 0; }
    unsafe { &*(ptr as *const Vec<i64>) }.get(idx as usize).copied().unwrap_or(0)
}

// ─── Memory management ───────────────────────────────
//
// RC heap layout:
//   [refcount: i64][payload bytes...]
//   ^ptr points here
//
// Payload is accessed at ptr + 8.

/// Global memory tracker for testing — counts allocs, frees, retains, releases.
/// Set `debug` to true to print every memory operation.
pub struct MemTracker {
    pub allocs: AtomicU64,
    pub frees: AtomicU64,
    pub retains: AtomicU64,
    pub releases: AtomicU64,
    pub live_bytes: AtomicI64,
    pub debug: std::sync::atomic::AtomicBool,
}

pub static MEM: MemTracker = MemTracker {
    allocs: AtomicU64::new(0),
    frees: AtomicU64::new(0),
    retains: AtomicU64::new(0),
    releases: AtomicU64::new(0),
    live_bytes: AtomicI64::new(0),
    debug: std::sync::atomic::AtomicBool::new(false),
};

/// Lock for serializing memory tests. Hold this while running a test that checks MEM.
pub static MEM_TEST_LOCK: Mutex<()> = Mutex::new(());

impl MemTracker {
    pub fn set_debug(&self, on: bool) {
        self.debug.store(on, Ordering::SeqCst);
    }

    fn is_debug(&self) -> bool {
        self.debug.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.allocs.store(0, Ordering::SeqCst);
        self.frees.store(0, Ordering::SeqCst);
        self.retains.store(0, Ordering::SeqCst);
        self.releases.store(0, Ordering::SeqCst);
        self.live_bytes.store(0, Ordering::SeqCst);
    }

    pub fn stats(&self) -> (u64, u64, u64, u64, i64) {
        (
            self.allocs.load(Ordering::SeqCst),
            self.frees.load(Ordering::SeqCst),
            self.retains.load(Ordering::SeqCst),
            self.releases.load(Ordering::SeqCst),
            self.live_bytes.load(Ordering::SeqCst),
        )
    }

    pub fn assert_clean(&self) {
        let (allocs, frees, ..) = self.stats();
        assert_eq!(allocs, frees, "memory leak: {} allocs but only {} frees", allocs, frees);
        let live = self.live_bytes.load(Ordering::SeqCst);
        assert_eq!(live, 0, "live bytes should be 0 but got {}", live);
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

/// Create an RC'd string from a static C string pointer.
/// Copies the bytes into an RC-managed allocation.
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
    MEM.allocs.fetch_add(1, Ordering::SeqCst);
    MEM.live_bytes.fetch_add(total as i64, Ordering::SeqCst);
    // Return pointer to payload, not header
    unsafe { base.add(RC_HEADER_SIZE) as i64 }
}

/// Increment refcount (for const sharing). Takes payload pointer.
pub extern "C" fn roca_rc_retain(ptr: i64) {
    if ptr == 0 { return; }
    let header = rc_header(ptr);
    unsafe { *header += 1; }
    let new_rc = unsafe { *header };
    MEM.retains.fetch_add(1, Ordering::SeqCst);
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
    MEM.frees.fetch_add(1, Ordering::SeqCst);
    MEM.live_bytes.fetch_sub(32, Ordering::SeqCst);
}

/// Free a struct (Vec<i64>). Releases the first n_heap_fields slots as RC pointers.
pub extern "C" fn roca_free_struct(ptr: i64, n_heap_fields: i64) {
    if ptr == 0 { return; }
    if MEM.is_debug() { eprintln!("  [mem] free_struct {:#x} (heap_fields={})", ptr, n_heap_fields); }
    let v = unsafe { &*(ptr as *const Vec<i64>) };
    // Cascade-release: free heap fields before freeing the struct
    for i in 0..(n_heap_fields as usize).min(v.len()) {
        let field_ptr = v[i];
        if field_ptr != 0 {
            roca_rc_release(field_ptr);
        }
    }
    let v = unsafe { Box::from_raw(ptr as *mut Vec<i64>) };
    let size = 24 + v.len() as i64 * 8;
    drop(v);
    MEM.frees.fetch_add(1, Ordering::SeqCst);
    MEM.live_bytes.fetch_sub(size, Ordering::SeqCst);
}

/// Decrement refcount. Free if zero. Takes payload pointer.
pub extern "C" fn roca_rc_release(ptr: i64) {
    if ptr == 0 { return; }
    MEM.releases.fetch_add(1, Ordering::SeqCst);
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
            // Corrupted header — skip dealloc to avoid UB, but still track
            MEM.frees.fetch_add(1, Ordering::SeqCst);
            return;
        }
        MEM.frees.fetch_add(1, Ordering::SeqCst);
        MEM.live_bytes.fetch_sub(total_size as i64, Ordering::SeqCst);
        let base = unsafe { (ptr as *mut u8).sub(RC_HEADER_SIZE) };
        let layout = std::alloc::Layout::from_size_align(total_size, 8).unwrap();
        unsafe { std::alloc::dealloc(base, layout); }
    }
}
