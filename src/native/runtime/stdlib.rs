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

/// Shared tokio runtime for wait operations — created once, reused across calls.
fn tokio_rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
        tokio::runtime::Runtime::new().expect("failed to create tokio runtime")
    });
    &RT
}

/// Spawn N function pointers as tokio tasks, wait for all to complete.
pub extern "C" fn roca_wait_all(fn_ptrs: i64, count: i64) -> i64 {
    let arr = roca_array_new();
    if fn_ptrs == 0 || count <= 0 { return arr; }

    let ptrs: Vec<i64> = (0..count as usize)
        .map(|i| roca_array_get_str(fn_ptrs, i as i64))
        .collect();

    tokio_rt().block_on(async {
        let mut handles = Vec::new();
        for fp in ptrs {
            handles.push(tokio::task::spawn_blocking(move || {
                if fp == 0 { return 0.0; }
                let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(fp) };
                f()
            }));
        }
        for handle in handles {
            let result = handle.await.unwrap_or(0.0);
            roca_array_push_f64(arr, result);
        }
    });
    arr
}

/// Spawn N function pointers as tokio tasks, return first result.
pub extern "C" fn roca_wait_first(fn_ptrs: i64, count: i64) -> f64 {
    if fn_ptrs == 0 || count <= 0 { return 0.0; }

    let ptrs: Vec<i64> = (0..count as usize)
        .map(|i| roca_array_get_str(fn_ptrs, i as i64))
        .collect();

    tokio_rt().block_on(async {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        for fp in ptrs {
            let tx = tx.clone();
            tokio::task::spawn_blocking(move || {
                if fp == 0 { let _ = tx.blocking_send(0.0); return; }
                let f: extern "C" fn() -> f64 = unsafe { std::mem::transmute(fp) };
                let _ = tx.blocking_send(f());
            });
        }
        drop(tx);
        rx.recv().await.unwrap_or(0.0)
    })
}

// ─── File I/O ────────────────────────────────────────

/// Read a file to string. Returns (content_ptr, err_tag).
/// err_tag: 0 = OK, 1 = not_found, 2 = permission, 3 = io
#[allow(improper_ctypes_definitions)]
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
#[allow(improper_ctypes_definitions)]
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

// ─── Crypto ─────────────────────────────────────────

pub extern "C" fn roca_crypto_random_uuid() -> i64 {
    alloc_str(&uuid::Uuid::new_v4().to_string())
}

pub extern "C" fn roca_crypto_sha256(data: i64) -> i64 {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(read_cstr(data).as_bytes());
    alloc_str(&to_hex(&hash))
}

pub extern "C" fn roca_crypto_sha512(data: i64) -> i64 {
    use sha2::Digest;
    let hash = sha2::Sha512::digest(read_cstr(data).as_bytes());
    alloc_str(&to_hex(&hash))
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for &b in bytes { let _ = write!(hex, "{:02x}", b); }
    hex
}

// ─── Url ────────────────────────────────────────────

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_url_parse(raw: i64) -> (i64, u8) {
    match url::Url::parse(read_cstr(raw)) {
        Ok(parsed) => (box_value(parsed), 0),
        Err(_) => (0, 1),
    }
}

// ─── Box allocator ─────────────────────────────────
// Layout: [drop_fn: fn(*mut u8) | 0][alloc_size: u64][payload...]
// roca_box_alloc returns pointer to payload (header is 16 bytes behind it).
// roca_box_free calls drop_fn (if set) then deallocates.

const BOX_HEADER: usize = 16;

pub extern "C" fn roca_box_alloc(size: i64) -> i64 {
    if size <= 0 { return 0; }
    let total = BOX_HEADER + size as usize;
    let layout = std::alloc::Layout::from_size_align(total, 8).unwrap();
    unsafe {
        let base = std::alloc::alloc(layout);
        if base.is_null() { return 0; }
        *(base as *mut u64) = 0;
        *((base as *mut u64).add(1)) = total as u64;
        MEM.track_alloc(total as i64);
        base.add(BOX_HEADER) as i64
    }
}

pub extern "C" fn roca_box_free(ptr: i64) {
    if ptr == 0 { return; }
    unsafe {
        let base = (ptr as *mut u8).sub(BOX_HEADER);
        let drop_fn = *(base as *const u64);
        let total = *((base as *const u64).add(1)) as usize;
        if drop_fn != 0 {
            let dropper: fn(*mut u8) = std::mem::transmute(drop_fn);
            dropper(ptr as *mut u8);
        }
        let layout = std::alloc::Layout::from_size_align_unchecked(total, 8);
        MEM.track_free(total as i64);
        std::alloc::dealloc(base, layout);
    }
}

fn drop_trampoline<T>(ptr: *mut u8) {
    unsafe { std::ptr::drop_in_place(ptr as *mut T); }
}

/// Allocate via `roca_box_alloc`, write `value`, and register its destructor.
fn box_value<T>(value: T) -> i64 {
    let ptr = roca_box_alloc(std::mem::size_of::<T>() as i64);
    if ptr == 0 { return 0; }
    unsafe {
        let base = (ptr as *mut u8).sub(BOX_HEADER);
        *(base as *mut u64) = drop_trampoline::<T> as u64;
        std::ptr::write(ptr as *mut T, value);
    }
    ptr
}

pub extern "C" fn roca_url_is_valid(raw: i64) -> u8 {
    if url::Url::parse(read_cstr(raw)).is_ok() { 1 } else { 0 }
}

fn with_url<F: FnOnce(&url::Url) -> i64>(ptr: i64, f: F) -> i64 {
    if ptr == 0 { return alloc_str(""); }
    let url = unsafe { &*(ptr as *const url::Url) };
    f(url)
}

pub extern "C" fn roca_url_hostname(ptr: i64) -> i64 {
    with_url(ptr, |u| alloc_str(u.host_str().unwrap_or("")))
}

pub extern "C" fn roca_url_protocol(ptr: i64) -> i64 {
    with_url(ptr, |u| alloc_str(&format!("{}:", u.scheme())))
}

pub extern "C" fn roca_url_pathname(ptr: i64) -> i64 {
    with_url(ptr, |u| alloc_str(u.path()))
}

pub extern "C" fn roca_url_search(ptr: i64) -> i64 {
    with_url(ptr, |u| alloc_str(&u.query().map_or(String::new(), |q| format!("?{}", q))))
}

pub extern "C" fn roca_url_hash(ptr: i64) -> i64 {
    with_url(ptr, |u| alloc_str(&u.fragment().map_or(String::new(), |f| format!("#{}", f))))
}

pub extern "C" fn roca_url_host(ptr: i64) -> i64 {
    with_url(ptr, |u| {
        let host = u.host_str().unwrap_or("");
        match u.port() {
            Some(p) => alloc_str(&format!("{}:{}", host, p)),
            None => alloc_str(host),
        }
    })
}

pub extern "C" fn roca_url_port(ptr: i64) -> i64 {
    with_url(ptr, |u| alloc_str(&u.port().map_or(String::new(), |p| p.to_string())))
}

pub extern "C" fn roca_url_origin(ptr: i64) -> i64 {
    with_url(ptr, |u| alloc_str(&u.origin().ascii_serialization()))
}

pub extern "C" fn roca_url_href(ptr: i64) -> i64 {
    with_url(ptr, |u| alloc_str(u.as_str()))
}

pub extern "C" fn roca_url_to_string(ptr: i64) -> i64 {
    roca_url_href(ptr)
}

pub extern "C" fn roca_url_get_param(ptr: i64, name: i64) -> i64 {
    with_url(ptr, |u| match u.query_pairs().find(|(k, _)| k == read_cstr(name)) {
        Some((_, v)) => alloc_str(&v),
        None => 0,
    })
}

pub extern "C" fn roca_url_has_param(ptr: i64, name: i64) -> u8 {
    if ptr == 0 { return 0; }
    let url = unsafe { &*(ptr as *const url::Url) };
    if url.query_pairs().any(|(k, _)| k == read_cstr(name)) { 1 } else { 0 }
}

// ─── Encoding ───────────────────────────────────────

use base64::Engine;

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_encoding_btoa(input: i64) -> (i64, u8) {
    let encoded = base64::engine::general_purpose::STANDARD.encode(read_cstr(input).as_bytes());
    (alloc_str(&encoded), 0)
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_encoding_atob(input: i64) -> (i64, u8) {
    match base64::engine::general_purpose::STANDARD.decode(read_cstr(input).as_bytes()) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(decoded) => (alloc_str(&decoded), 0),
            Err(_) => (0, 1),
        },
        Err(_) => (0, 1),
    }
}

/// Native strings are already UTF-8 — encode is identity.
pub extern "C" fn roca_encoding_encode(input: i64) -> i64 {
    input
}

/// Native strings are already UTF-8 — decode is identity with null check.
#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_encoding_decode(bytes: i64) -> (i64, u8) {
    if bytes == 0 { return (0, 1); }
    (bytes, 0)
}

// ─── JSON ───────────────────────────────────────────

fn with_json<T, F: FnOnce(&serde_json::Value) -> T>(ptr: i64, default: T, f: F) -> T {
    if ptr == 0 { return default; }
    let value = unsafe { &*(ptr as *const serde_json::Value) };
    f(value)
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_json_parse(text: i64) -> (i64, u8) {
    match serde_json::from_str::<serde_json::Value>(read_cstr(text)) {
        Ok(value) => (box_value(value), 0),
        Err(_) => (0, 1),
    }
}

pub extern "C" fn roca_json_stringify(json: i64) -> i64 {
    with_json(json, alloc_str("null"), |v| alloc_str(&v.to_string()))
}

pub extern "C" fn roca_json_get(json: i64, key: i64) -> i64 {
    with_json(json, 0, |v| match v.get(read_cstr(key)) {
        Some(inner) => box_value(inner.clone()),
        None => 0,
    })
}

pub extern "C" fn roca_json_get_string(json: i64, key: i64) -> i64 {
    with_json(json, 0, |v| match v.get(read_cstr(key)).and_then(|v| v.as_str()) {
        Some(s) => alloc_str(s),
        None => 0,
    })
}

pub extern "C" fn roca_json_get_number(json: i64, key: i64) -> f64 {
    with_json(json, f64::NAN, |v| v.get(read_cstr(key)).and_then(|v| v.as_f64()).unwrap_or(f64::NAN))
}

pub extern "C" fn roca_json_get_bool(json: i64, key: i64) -> u8 {
    with_json(json, 0, |v| match v.get(read_cstr(key)).and_then(|v| v.as_bool()) {
        Some(true) => 1,
        _ => 0,
    })
}

pub extern "C" fn roca_json_get_array(json: i64, key: i64) -> i64 {
    with_json(json, 0, |v| match v.get(read_cstr(key)).and_then(|v| v.as_array()) {
        Some(arr) => {
            let result = roca_array_new();
            for item in arr {
                let ptr = box_value(item.clone());
                roca_array_push_str(result, ptr);
            }
            result
        }
        None => 0,
    })
}

/// Free an array of boxed JSON values: roca_box_free each element, then drop the Vec.
pub extern "C" fn roca_free_json_array(ptr: i64) {
    if ptr == 0 { return; }
    let v = unsafe { &*(ptr as *const Vec<i64>) };
    for &elem in v.iter() {
        roca_box_free(elem);
    }
    // Drop the Vec itself (same as roca_free_array)
    unsafe { drop(Box::from_raw(ptr as *mut Vec<i64>)); }
    MEM.track_free(32);
}

pub extern "C" fn roca_json_to_string(json: i64) -> i64 {
    roca_json_stringify(json)
}

// ─── Http ───────────────────────────────────────────

struct HttpResponse {
    status: u16,
    headers: std::collections::HashMap<String, String>,
    body: String,
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default()
    });
    &CLIENT
}

fn http_request(method: &str, url_str: &str, body: Option<&str>) -> Result<HttpResponse, String> {
    tokio_rt().block_on(async {
        let client = http_client();
        let mut req = match method {
            "GET" => client.get(url_str),
            "POST" => client.post(url_str),
            "PUT" => client.put(url_str),
            "PATCH" => client.patch(url_str),
            "DELETE" => client.delete(url_str),
            _ => return Err("unsupported method".into()),
        };
        if let Some(b) = body { req = req.body(b.to_string()); }
        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                // reqwest normalises header names to lowercase
                let headers = resp.headers().iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = resp.text().await.unwrap_or_default();
                Ok(HttpResponse { status, headers, body })
            }
            Err(e) => Err(e.to_string()),
        }
    })
}

fn box_response(result: Result<HttpResponse, String>) -> (i64, u8) {
    match result {
        Ok(resp) => (box_value(resp), 0),
        Err(_) => (0, 1),
    }
}

fn with_resp<T, F: FnOnce(&HttpResponse) -> T>(ptr: i64, default: T, f: F) -> T {
    if ptr == 0 { return default; }
    let r = unsafe { &*(ptr as *const HttpResponse) };
    f(r)
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_http_get(url: i64) -> (i64, u8) {
    box_response(http_request("GET", read_cstr(url), None))
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_http_post(url: i64, body: i64) -> (i64, u8) {
    box_response(http_request("POST", read_cstr(url), Some(read_cstr(body))))
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_http_put(url: i64, body: i64) -> (i64, u8) {
    box_response(http_request("PUT", read_cstr(url), Some(read_cstr(body))))
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_http_patch(url: i64, body: i64) -> (i64, u8) {
    box_response(http_request("PATCH", read_cstr(url), Some(read_cstr(body))))
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_http_delete(url: i64) -> (i64, u8) {
    box_response(http_request("DELETE", read_cstr(url), None))
}

pub extern "C" fn roca_http_status(resp: i64) -> f64 {
    with_resp(resp, 0.0, |r| r.status as f64)
}

pub extern "C" fn roca_http_ok(resp: i64) -> u8 {
    with_resp(resp, 0, |r| if (200..300).contains(&r.status) { 1 } else { 0 })
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_http_text(resp: i64) -> (i64, u8) {
    with_resp(resp, (0, 1), |r| (alloc_str(&r.body), 0))
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn roca_http_json(resp: i64) -> (i64, u8) {
    with_resp(resp, (0, 1), |r| {
        match serde_json::from_str::<serde_json::Value>(&r.body) {
            Ok(value) => (box_value(value), 0),
            Err(_) => (0, 1),
        }
    })
}

pub extern "C" fn roca_http_header(resp: i64, name: i64) -> i64 {
    with_resp(resp, 0, |r| match r.headers.get(&read_cstr(name).to_lowercase()) {
        Some(v) => alloc_str(v),
        None => 0,
    })
}
