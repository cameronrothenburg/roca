//! Standard library runtime functions: string ops, array ops, math,
//! path, process, timing/async, and file I/O.

use std::ffi::CStr;
use super::{read_cstr, alloc_str, MEM};

// ─── I/O ─────────────────────────────────────────────

pub extern "C" fn roca_print(s: i64) {
    if s == 0 { println!("null"); return; }
    let cstr = unsafe { CStr::from_ptr(s as *const u8 as *const i8) };
    if let Ok(s) = cstr.to_str() { println!("{}", s); }
}

pub extern "C" fn roca_print_f64(n: f64) {
    if n.fract() == 0.0 && n.abs() < 1e15 { println!("{}", n as i64); }
    else { println!("{}", n); }
}

pub extern "C" fn roca_print_bool(v: u8) {
    println!("{}", if v != 0 { "true" } else { "false" });
}

// ─── String core ─────────────────────────────────────

pub extern "C" fn roca_string_eq(a: i64, b: i64) -> u8 {
    if a == b { return 1; }
    if a == 0 || b == 0 { return 0; }
    let a_str = unsafe { CStr::from_ptr(a as *const i8) };
    let b_str = unsafe { CStr::from_ptr(b as *const i8) };
    if a_str == b_str { 1 } else { 0 }
}

pub extern "C" fn roca_string_concat(a: i64, b: i64) -> i64 {
    let combined = format!("{}{}", read_cstr(a), read_cstr(b));
    alloc_str(&combined)
}

pub extern "C" fn roca_string_len(s: i64) -> i64 {
    if s == 0 { return 0; }
    unsafe { CStr::from_ptr(s as *const i8) }.to_bytes().len() as i64
}

pub extern "C" fn roca_string_from_f64(n: f64) -> i64 {
    if n.fract() == 0.0 && n.abs() < 1e15 { alloc_str(&format!("{}", n as i64)) }
    else { alloc_str(&format!("{}", n)) }
}

// ─── String methods ──────────────────────────────────

pub extern "C" fn roca_string_includes(haystack: i64, needle: i64) -> u8 {
    if read_cstr(haystack).contains(read_cstr(needle)) { 1 } else { 0 }
}

pub extern "C" fn roca_string_starts_with(s: i64, prefix: i64) -> u8 {
    if read_cstr(s).starts_with(read_cstr(prefix)) { 1 } else { 0 }
}

pub extern "C" fn roca_string_ends_with(s: i64, suffix: i64) -> u8 {
    if read_cstr(s).ends_with(read_cstr(suffix)) { 1 } else { 0 }
}

pub extern "C" fn roca_string_trim(s: i64) -> i64 { alloc_str(read_cstr(s).trim()) }
pub extern "C" fn roca_string_to_upper(s: i64) -> i64 { alloc_str(&read_cstr(s).to_uppercase()) }
pub extern "C" fn roca_string_to_lower(s: i64) -> i64 { alloc_str(&read_cstr(s).to_lowercase()) }

pub extern "C" fn roca_string_slice(s: i64, start: i64, end: i64) -> i64 {
    let text = read_cstr(s);
    let start = (start as usize).min(text.len());
    let end = (end as usize).min(text.len());
    if start >= end { return alloc_str(""); }
    alloc_str(&text[start..end])
}

pub extern "C" fn roca_string_split(s: i64, delim: i64) -> i64 {
    let parts: Vec<i64> = read_cstr(s).split(read_cstr(delim)).map(|p| alloc_str(p)).collect();
    Box::into_raw(Box::new(parts)) as i64
}

pub extern "C" fn roca_string_char_at(s: i64, idx: i64) -> i64 {
    read_cstr(s).chars().nth(idx as usize)
        .map(|c| alloc_str(&c.to_string()))
        .unwrap_or_else(|| alloc_str(""))
}

pub extern "C" fn roca_string_index_of(haystack: i64, needle: i64) -> f64 {
    read_cstr(haystack).find(read_cstr(needle)).map(|i| i as f64).unwrap_or(-1.0)
}

pub extern "C" fn roca_string_char_code_at(s: i64, idx: i64) -> f64 {
    read_cstr(s).chars().nth(idx as usize).map(|c| c as u32 as f64).unwrap_or(f64::NAN)
}

pub extern "C" fn roca_char_from_code(code: f64) -> i64 {
    let c = char::from_u32(code as u32).unwrap_or('\0');
    alloc_str(&c.to_string())
}

pub extern "C" fn roca_char_is_digit(ch: i64) -> u8 {
    read_cstr(ch).chars().next().map(|c| c.is_ascii_digit() as u8).unwrap_or(0)
}

pub extern "C" fn roca_char_is_letter(ch: i64) -> u8 {
    read_cstr(ch).chars().next().map(|c| c.is_ascii_alphabetic() as u8).unwrap_or(0)
}

pub extern "C" fn roca_char_is_whitespace(ch: i64) -> u8 {
    read_cstr(ch).chars().next().map(|c| c.is_ascii_whitespace() as u8).unwrap_or(0)
}

pub extern "C" fn roca_char_is_alphanumeric(ch: i64) -> u8 {
    read_cstr(ch).chars().next().map(|c| c.is_ascii_alphanumeric() as u8).unwrap_or(0)
}

pub extern "C" fn roca_number_parse(s: i64) -> f64 {
    read_cstr(s).trim().parse::<f64>().unwrap_or(f64::NAN)
}

// ─── Map (String → i64 value pointers) ──────

type RocaMap = std::collections::HashMap<String, i64>;

pub extern "C" fn roca_map_new() -> i64 {
    let ptr = Box::into_raw(Box::new(RocaMap::new())) as i64;
    MEM.track_alloc(64);
    ptr
}

pub extern "C" fn roca_map_free(map_ptr: i64) {
    if map_ptr == 0 { return; }
    unsafe { drop(Box::from_raw(map_ptr as *mut RocaMap)); }
    MEM.track_free(64);
}

pub extern "C" fn roca_map_set(map_ptr: i64, key: i64, value: i64) -> i64 {
    if map_ptr == 0 { return 0; }
    let map = unsafe { &mut *(map_ptr as *mut RocaMap) };
    map.insert(read_cstr(key).to_string(), value);
    map_ptr
}

pub extern "C" fn roca_map_get(map_ptr: i64, key: i64) -> i64 {
    if map_ptr == 0 { return 0; }
    let map = unsafe { &*(map_ptr as *const RocaMap) };
    *map.get(read_cstr(key)).unwrap_or(&0)
}

pub extern "C" fn roca_map_has(map_ptr: i64, key: i64) -> u8 {
    if map_ptr == 0 { return 0; }
    let map = unsafe { &*(map_ptr as *const RocaMap) };
    map.contains_key(read_cstr(key)) as u8
}

pub extern "C" fn roca_map_delete(map_ptr: i64, key: i64) -> u8 {
    if map_ptr == 0 { return 0; }
    let map = unsafe { &mut *(map_ptr as *mut RocaMap) };
    map.remove(read_cstr(key)).is_some() as u8
}

pub extern "C" fn roca_map_size(map_ptr: i64) -> f64 {
    if map_ptr == 0 { return 0.0; }
    let map = unsafe { &*(map_ptr as *const RocaMap) };
    map.len() as f64
}

pub extern "C" fn roca_map_keys(map_ptr: i64) -> i64 {
    if map_ptr == 0 { return 0; }
    let map = unsafe { &*(map_ptr as *const RocaMap) };
    let keys: Vec<i64> = map.keys().map(|k| alloc_str(k)).collect();
    Box::into_raw(Box::new(keys)) as i64
}

pub extern "C" fn roca_map_values(map_ptr: i64) -> i64 {
    if map_ptr == 0 { return 0; }
    let map = unsafe { &*(map_ptr as *const RocaMap) };
    let vals: Vec<i64> = map.values().copied().collect();
    Box::into_raw(Box::new(vals)) as i64
}

// ─── Array operations ────────────────────────────────

pub extern "C" fn roca_array_new() -> i64 {
    let ptr = Box::into_raw(Box::new(Vec::<i64>::new())) as i64;
    MEM.track_alloc(32);
    ptr
}

pub extern "C" fn roca_array_push_f64(arr: i64, val: f64) {
    if arr == 0 { return; }
    unsafe { &mut *(arr as *mut Vec<i64>) }.push(val.to_bits() as i64);
}

pub extern "C" fn roca_array_get_f64(arr: i64, idx: i64) -> f64 {
    if arr == 0 { return 0.0; }
    unsafe { &*(arr as *const Vec<i64>) }.get(idx as usize).map(|&b| f64::from_bits(b as u64)).unwrap_or(0.0)
}

pub extern "C" fn roca_array_len(arr: i64) -> i64 {
    if arr == 0 { return 0; }
    unsafe { &*(arr as *const Vec<i64>) }.len() as i64
}

pub extern "C" fn roca_array_push_str(arr: i64, val: i64) {
    if arr == 0 { return; }
    unsafe { &mut *(arr as *mut Vec<i64>) }.push(val);
}

pub extern "C" fn roca_array_get_str(arr: i64, idx: i64) -> i64 {
    if arr == 0 { return 0; }
    unsafe { &*(arr as *const Vec<i64>) }.get(idx as usize).copied().unwrap_or(0)
}

pub extern "C" fn roca_array_join(arr: i64, sep: i64) -> i64 {
    if arr == 0 { return alloc_str(""); }
    let v = unsafe { &*(arr as *const Vec<i64>) };
    let parts: Vec<&str> = v.iter().map(|&ptr| read_cstr(ptr)).collect();
    alloc_str(&parts.join(read_cstr(sep)))
}

// ─── Math ────────────────────────────────────────────

pub extern "C" fn roca_math_floor(n: f64) -> f64 { n.floor() }
pub extern "C" fn roca_math_ceil(n: f64) -> f64 { n.ceil() }
pub extern "C" fn roca_math_round(n: f64) -> f64 { n.round() }
pub extern "C" fn roca_math_abs(n: f64) -> f64 { n.abs() }
pub extern "C" fn roca_math_sqrt(n: f64) -> f64 { n.sqrt() }
pub extern "C" fn roca_math_pow(base: f64, exp: f64) -> f64 { base.powf(exp) }
pub extern "C" fn roca_math_min(a: f64, b: f64) -> f64 { a.min(b) }
pub extern "C" fn roca_math_max(a: f64, b: f64) -> f64 { a.max(b) }

// ─── Path ────────────────────────────────────────────

pub extern "C" fn roca_path_join(base: i64, segment: i64) -> i64 {
    let b = read_cstr(base);
    let s = read_cstr(segment);
    let joined = std::path::Path::new(b).join(s);
    alloc_str(&joined.to_string_lossy())
}

pub extern "C" fn roca_path_dirname(path: i64) -> i64 {
    let p = std::path::Path::new(read_cstr(path));
    alloc_str(&p.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".into()))
}

pub extern "C" fn roca_path_basename(path: i64) -> i64 {
    let p = std::path::Path::new(read_cstr(path));
    alloc_str(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

pub extern "C" fn roca_path_extension(path: i64) -> i64 {
    let p = std::path::Path::new(read_cstr(path));
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    alloc_str(&ext)
}

// ─── Process ─────────────────────────────────────────

pub extern "C" fn roca_process_cwd() -> i64 {
    alloc_str(&std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| ".".into()))
}

pub extern "C" fn roca_process_exit(code: f64) {
    std::process::exit(code as i32);
}

// ─── Async / Timing ──────────────────────────────────

/// Sleep for the given number of milliseconds. Blocks the current thread.
pub extern "C" fn roca_sleep(ms: f64) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
}

/// Get current time in milliseconds since epoch.
pub extern "C" fn roca_time_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Call a function pointer on a new thread and return the f64 result.
/// Used by waitAll/waitFirst to run JIT functions in parallel.
/// fn_ptr is a JIT function pointer (extern "C" fn() -> f64).
pub extern "C" fn roca_thread_call_f64(fn_ptr: i64) -> f64 {
    if fn_ptr == 0 { return 0.0; }
    let fp: extern "C" fn() -> f64 = unsafe { std::mem::transmute(fn_ptr) };
    fp()
}

/// Spawn N function pointers as threads, wait for all to complete.
/// fn_ptrs is an array of I64 function pointers. Returns array of f64 results.
pub extern "C" fn roca_wait_all(fn_ptrs: i64, count: i64) -> i64 {
    let arr = roca_array_new();
    if fn_ptrs == 0 || count <= 0 { return arr; }

    let ptrs: Vec<i64> = (0..count as usize)
        .map(|i| roca_array_get_str(fn_ptrs, i as i64))
        .collect();

    let handles: Vec<_> = ptrs.into_iter().map(|fp| {
        std::thread::spawn(move || {
            if fp == 0 { return 0.0; }
            let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(fp) };
            f()
        })
    }).collect();

    for handle in handles {
        let result = handle.join().unwrap_or(0.0);
        roca_array_push_f64(arr, result);
    }
    arr
}

/// Spawn N function pointers as threads, return first result.
pub extern "C" fn roca_wait_first(fn_ptrs: i64, count: i64) -> f64 {
    if fn_ptrs == 0 || count <= 0 { return 0.0; }

    let ptrs: Vec<i64> = (0..count as usize)
        .map(|i| roca_array_get_str(fn_ptrs, i as i64))
        .collect();

    let (tx, rx) = std::sync::mpsc::channel();

    for fp in ptrs {
        let tx = tx.clone();
        std::thread::spawn(move || {
            if fp == 0 { let _ = tx.send(0.0); return; }
            let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(fp) };
            let _ = tx.send(f());
        });
    }

    rx.recv().unwrap_or(0.0)
}

// ─── File I/O ────────────────────────────────────────

/// Read a file to string. Returns (content_ptr, err_tag).
/// err_tag: 0 = OK, 1 = not_found, 2 = permission, 3 = io
pub extern "C" fn roca_fs_read_file(path: i64) -> (i64, u8) {
    let path_str = read_cstr(path);
    match std::fs::read_to_string(path_str) {
        Ok(content) => (alloc_str(&content), 0),
        Err(e) => {
            let tag = match e.kind() {
                std::io::ErrorKind::NotFound => 1,
                std::io::ErrorKind::PermissionDenied => 2,
                _ => 3,
            };
            (0, tag) // null on error — no allocation to leak
        }
    }
}

/// Write string content to a file. Returns err_tag.
/// err_tag: 0 = OK, 1 = permission, 2 = io
pub extern "C" fn roca_fs_write_file(path: i64, content: i64) -> u8 {
    let path_str = read_cstr(path);
    let content_str = read_cstr(content);
    match std::fs::write(path_str, content_str) {
        Ok(()) => 0,
        Err(e) => match e.kind() {
            std::io::ErrorKind::PermissionDenied => 1,
            _ => 2,
        },
    }
}

/// Check if a path exists. Returns 1 (true) or 0 (false).
pub extern "C" fn roca_fs_exists(path: i64) -> u8 {
    if std::path::Path::new(read_cstr(path)).exists() { 1 } else { 0 }
}

/// Read directory entries. Returns (array_ptr, err_tag).
/// The array contains string pointers to entry names.
pub extern "C" fn roca_fs_read_dir(path: i64) -> (i64, u8) {
    let path_str = read_cstr(path);
    match std::fs::read_dir(path_str) {
        Ok(entries) => {
            let arr = roca_array_new();
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    let name_ptr = alloc_str(name);
                    roca_array_push_str(arr, name_ptr);
                }
            }
            (arr, 0)
        }
        Err(e) => {
            let tag = match e.kind() {
                std::io::ErrorKind::NotFound => 1,
                std::io::ErrorKind::PermissionDenied => 2,
                _ => 3,
            };
            (0, tag) // null on error — no allocation to leak
        }
    }
}
