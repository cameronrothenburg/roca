//! Runtime function registry — declares and imports runtime functions for Cranelift.
//! Host implementations live in `roca-runtime`. This module wires them into the JIT.

// Re-export everything from roca-runtime so existing code keeps working
pub use roca_runtime::*;

use cranelift_codegen::ir::types;
use cranelift_jit::JITBuilder;
use cranelift_module::{Module, Linkage, FuncId};

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
                compiled: &crate::context::CompiledFuncs,
            ) -> std::collections::HashMap<String, cranelift_codegen::ir::FuncRef> {
                let mut refs = std::collections::HashMap::new();
                $( refs.insert(
                    concat!("__", stringify!($key)).to_string(),
                    module.declare_func_in_func(self.$key, func),
                ); )*
                // Register stdlib functions under Contract.method names
                let aliases: Vec<_> = refs.iter()
                    .filter_map(|(k, v)| stdlib_key_to_contract(k).map(|name| (name, *v)))
                    .collect();
                for (name, func_ref) in aliases {
                    refs.insert(name, func_ref);
                }
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
    (string_char_code_at, "roca_string_char_code_at", roca_string_char_code_at, [types::I64, types::I64],                [types::F64]),

    // Character utilities
    (char_from_code,    "roca_char_from_code",    roca_char_from_code,    [types::F64],                                [types::I64]),
    (char_is_digit,     "roca_char_is_digit",     roca_char_is_digit,     [types::I64],                                [types::I8]),
    (char_is_letter,    "roca_char_is_letter",    roca_char_is_letter,    [types::I64],                                [types::I8]),
    (char_is_whitespace,"roca_char_is_whitespace",roca_char_is_whitespace,[types::I64],                                [types::I8]),
    (char_is_alphanumeric,"roca_char_is_alphanumeric",roca_char_is_alphanumeric,[types::I64],                           [types::I8]),

    // Number parsing
    (number_parse,      "roca_number_parse",      roca_number_parse,      [types::I64],                                [types::F64]),

    // Arrays
    (array_new,         "roca_array_new",         roca_array_new,         [],                                          [types::I64]),
    (array_push_f64,    "roca_array_push_f64",    roca_array_push_f64,    [types::I64, types::F64],                    []),
    (array_get_f64,     "roca_array_get_f64",     roca_array_get_f64,     [types::I64, types::I64],                    [types::F64]),
    (array_len,         "roca_array_len",         roca_array_len,         [types::I64],                                [types::I64]),
    (array_push_str,    "roca_array_push_str",    roca_array_push_str,    [types::I64, types::I64],                    []),
    (array_get_str,     "roca_array_get_str",     roca_array_get_str,     [types::I64, types::I64],                    [types::I64]),
    (array_join,        "roca_array_join",        roca_array_join,        [types::I64, types::I64],                    [types::I64]),

    // Maps
    (map_new,           "roca_map_new",           roca_map_new,           [],                                          [types::I64]),
    (map_free,          "roca_map_free",          roca_map_free,          [types::I64],                                []),
    (map_set,           "roca_map_set",           roca_map_set,           [types::I64, types::I64, types::I64],        [types::I64]),
    (map_get,           "roca_map_get",           roca_map_get,           [types::I64, types::I64],                    [types::I64]),
    (map_has,           "roca_map_has",           roca_map_has,           [types::I64, types::I64],                    [types::I8]),
    (map_delete,        "roca_map_delete",        roca_map_delete,        [types::I64, types::I64],                    [types::I8]),
    (map_size,          "roca_map_size",           roca_map_size,          [types::I64],                                [types::F64]),
    (map_keys,          "roca_map_keys",           roca_map_keys,          [types::I64],                                [types::I64]),
    (map_values,        "roca_map_values",         roca_map_values,        [types::I64],                                [types::I64]),

    // Structs
    (struct_alloc,      "roca_struct_alloc",      roca_struct_alloc,      [types::I64],                                [types::I64]),
    (struct_set_f64,    "roca_struct_set_f64",    roca_struct_set_f64,    [types::I64, types::I64, types::F64],        []),
    (struct_get_f64,    "roca_struct_get_f64",    roca_struct_get_f64,    [types::I64, types::I64],                    [types::F64]),
    (struct_set_ptr,    "roca_struct_set_ptr",    roca_struct_set_ptr,    [types::I64, types::I64, types::I64],        []),
    (struct_get_ptr,    "roca_struct_get_ptr",    roca_struct_get_ptr,    [types::I64, types::I64],                    [types::I64]),

    // Conversion
    (f64_to_bool,       "roca_f64_to_bool",       roca_f64_to_bool,       [types::F64],                                [types::I8]),

    // Constraint validation
    (constraint_panic, "roca_constraint_panic", roca_constraint_panic, [types::I64], []),

    // Math
    (math_floor,        "roca_math_floor",        roca_math_floor,        [types::F64],                                [types::F64]),
    (math_ceil,         "roca_math_ceil",          roca_math_ceil,         [types::F64],                                [types::F64]),
    (math_round,        "roca_math_round",         roca_math_round,        [types::F64],                                [types::F64]),
    (math_abs,          "roca_math_abs",            roca_math_abs,          [types::F64],                                [types::F64]),
    (math_sqrt,         "roca_math_sqrt",           roca_math_sqrt,         [types::F64],                                [types::F64]),
    (math_pow,          "roca_math_pow",            roca_math_pow,          [types::F64, types::F64],                    [types::F64]),
    (math_min,          "roca_math_min",            roca_math_min,          [types::F64, types::F64],                    [types::F64]),
    (math_max,          "roca_math_max",            roca_math_max,          [types::F64, types::F64],                    [types::F64]),

    // Path
    (path_join,         "roca_path_join",           roca_path_join,         [types::I64, types::I64],                    [types::I64]),
    (path_dirname,      "roca_path_dirname",        roca_path_dirname,      [types::I64],                                [types::I64]),
    (path_basename,     "roca_path_basename",       roca_path_basename,     [types::I64],                                [types::I64]),
    (path_extension,    "roca_path_extension",      roca_path_extension,    [types::I64],                                [types::I64]),

    // Process
    (process_cwd,       "roca_process_cwd",         roca_process_cwd,       [],                                          [types::I64]),
    (process_exit,      "roca_process_exit",        roca_process_exit,      [types::F64],                                []),

    // Async / timing / concurrency
    (sleep,             "roca_sleep",             roca_sleep,             [types::F64],                                []),
    (time_now,          "roca_time_now",          roca_time_now,          [],                                          [types::F64]),
    (wait_all,          "roca_wait_all",          roca_wait_all,          [types::I64, types::I64],                    [types::I64]),
    (wait_first,        "roca_wait_first",        roca_wait_first,        [types::I64, types::I64],                    [types::F64]),

    // File I/O
    (fs_read_file,      "roca_fs_read_file",      roca_fs_read_file,      [types::I64],                                [types::I64, types::I8]),
    (fs_write_file,     "roca_fs_write_file",     roca_fs_write_file,     [types::I64, types::I64],                    [types::I8]),
    (fs_exists,         "roca_fs_exists",         roca_fs_exists,         [types::I64],                                [types::I8]),
    (fs_read_dir,       "roca_fs_read_dir",       roca_fs_read_dir,       [types::I64],                                [types::I64, types::I8]),

    // Crypto
    (crypto_random_uuid, "roca_crypto_random_uuid", roca_crypto_random_uuid, [],                                        [types::I64]),
    (crypto_sha256,     "roca_crypto_sha256",     roca_crypto_sha256,     [types::I64],                                [types::I64]),
    (crypto_sha512,     "roca_crypto_sha512",     roca_crypto_sha512,     [types::I64],                                [types::I64]),

    // Url
    (url_parse,         "roca_url_parse",         roca_url_parse,         [types::I64],                                [types::I64, types::I8]),
    (url_is_valid,      "roca_url_is_valid",      roca_url_is_valid,      [types::I64],                                [types::I8]),
    (url_hostname,      "roca_url_hostname",      roca_url_hostname,      [types::I64],                                [types::I64]),
    (url_protocol,      "roca_url_protocol",      roca_url_protocol,      [types::I64],                                [types::I64]),
    (url_pathname,      "roca_url_pathname",      roca_url_pathname,      [types::I64],                                [types::I64]),
    (url_search,        "roca_url_search",        roca_url_search,        [types::I64],                                [types::I64]),
    (url_hash,          "roca_url_hash",          roca_url_hash,          [types::I64],                                [types::I64]),
    (url_host,          "roca_url_host",          roca_url_host,          [types::I64],                                [types::I64]),
    (url_port,          "roca_url_port",          roca_url_port,          [types::I64],                                [types::I64]),
    (url_origin,        "roca_url_origin",        roca_url_origin,        [types::I64],                                [types::I64]),
    (url_href,          "roca_url_href",          roca_url_href,          [types::I64],                                [types::I64]),
    (url_to_string,     "roca_url_to_string",     roca_url_to_string,     [types::I64],                                [types::I64]),
    (url_get_param,     "roca_url_get_param",     roca_url_get_param,     [types::I64, types::I64],                    [types::I64]),
    (url_has_param,     "roca_url_has_param",     roca_url_has_param,     [types::I64, types::I64],                    [types::I8]),

    // Encoding
    (encoding_btoa,     "roca_encoding_btoa",     roca_encoding_btoa,     [types::I64],                                [types::I64, types::I8]),
    (encoding_atob,     "roca_encoding_atob",     roca_encoding_atob,     [types::I64],                                [types::I64, types::I8]),
    (encoding_encode,   "roca_encoding_encode",   roca_encoding_encode,   [types::I64],                                [types::I64]),
    (encoding_decode,   "roca_encoding_decode",   roca_encoding_decode,   [types::I64],                                [types::I64, types::I8]),

    // JSON
    (json_parse,        "roca_json_parse",        roca_json_parse,        [types::I64],                                [types::I64, types::I8]),
    (json_stringify,    "roca_json_stringify",     roca_json_stringify,    [types::I64],                                [types::I64]),
    (json_get,          "roca_json_get",          roca_json_get,          [types::I64, types::I64],                    [types::I64]),
    (json_get_string,   "roca_json_get_string",   roca_json_get_string,   [types::I64, types::I64],                    [types::I64]),
    (json_get_number,   "roca_json_get_number",   roca_json_get_number,   [types::I64, types::I64],                    [types::F64]),
    (json_get_bool,     "roca_json_get_bool",     roca_json_get_bool,     [types::I64, types::I64],                    [types::I8]),
    (json_get_array,    "roca_json_get_array",    roca_json_get_array,    [types::I64, types::I64],                    [types::I64]),
    (json_to_string,    "roca_json_to_string",    roca_json_to_string,    [types::I64],                                [types::I64]),

    // Http
    (http_get,          "roca_http_get",          roca_http_get,          [types::I64],                                [types::I64, types::I8]),
    (http_post,         "roca_http_post",         roca_http_post,         [types::I64, types::I64],                    [types::I64, types::I8]),
    (http_put,          "roca_http_put",          roca_http_put,          [types::I64, types::I64],                    [types::I64, types::I8]),
    (http_patch,        "roca_http_patch",        roca_http_patch,        [types::I64, types::I64],                    [types::I64, types::I8]),
    (http_delete,       "roca_http_delete",       roca_http_delete,       [types::I64],                                [types::I64, types::I8]),
    (http_status,       "roca_http_status",       roca_http_status,       [types::I64],                                [types::F64]),
    (http_ok,           "roca_http_ok",           roca_http_ok,           [types::I64],                                [types::I8]),
    (http_text,         "roca_http_text",         roca_http_text,         [types::I64],                                [types::I64, types::I8]),
    (http_json,         "roca_http_json",         roca_http_json,         [types::I64],                                [types::I64, types::I8]),
    (http_header,       "roca_http_header",       roca_http_header,       [types::I64, types::I64],                    [types::I64]),

    // Memory management
    (free,              "roca_free",              roca_free,              [types::I64],                                []),
    (box_alloc,         "roca_box_alloc",         roca_box_alloc,         [types::I64],                                [types::I64]),
    (string_new,        "roca_string_new",        roca_string_new,        [types::I64],                                [types::I64]),
    (free_json_array,   "roca_free_json_array",   roca_free_json_array,   [types::I64],                                []),
}

/// Map internal runtime key (e.g. `__math_floor`) to Contract.method name (e.g. `Math.floor`).
fn stdlib_key_to_contract(key: &str) -> Option<String> {
    let key = key.strip_prefix("__")?;
    let mappings: &[(&str, &str)] = &[
        // Math
        ("math_floor", "Math.floor"), ("math_ceil", "Math.ceil"), ("math_round", "Math.round"),
        ("math_abs", "Math.abs"), ("math_sqrt", "Math.sqrt"), ("math_pow", "Math.pow"),
        ("math_min", "Math.min"), ("math_max", "Math.max"),
        // Path
        ("path_join", "Path.join"), ("path_dirname", "Path.dirname"),
        ("path_basename", "Path.basename"), ("path_extension", "Path.extension"),
        // Char
        ("char_from_code", "Char.fromCode"), ("char_is_digit", "Char.isDigit"),
        ("char_is_letter", "Char.isLetter"), ("char_is_whitespace", "Char.isWhitespace"),
        ("char_is_alphanumeric", "Char.isAlphanumeric"),
        // NumberParse
        ("number_parse", "NumberParse.parse"),
        // Process
        ("process_cwd", "Process.cwd"), ("process_exit", "Process.exit"),
        // Time
        ("time_now", "Time.now"),
        // Fs
        ("fs_read_file", "Fs.readFile"), ("fs_write_file", "Fs.writeFile"),
        ("fs_exists", "Fs.exists"), ("fs_read_dir", "Fs.readDir"),
        // Crypto
        ("crypto_random_uuid", "Crypto.randomUUID"),
        ("crypto_sha256", "Crypto.sha256"), ("crypto_sha512", "Crypto.sha512"),
        // Url
        ("url_parse", "Url.parse"), ("url_is_valid", "Url.isValid"),
        ("url_hostname", "Url.hostname"), ("url_protocol", "Url.protocol"),
        ("url_pathname", "Url.pathname"), ("url_search", "Url.search"),
        ("url_hash", "Url.hash"), ("url_host", "Url.host"), ("url_port", "Url.port"),
        ("url_origin", "Url.origin"), ("url_href", "Url.href"),
        ("url_to_string", "Url.toString"),
        ("url_get_param", "Url.getParam"), ("url_has_param", "Url.hasParam"),
        // Encoding
        ("encoding_btoa", "Encoding.btoa"), ("encoding_atob", "Encoding.atob"),
        ("encoding_encode", "Encoding.encode"), ("encoding_decode", "Encoding.decode"),
        // JSON
        ("json_parse", "JSON.parse"), ("json_stringify", "JSON.stringify"),
        ("json_get", "JSON.get"), ("json_get_string", "JSON.getString"),
        ("json_get_number", "JSON.getNumber"), ("json_get_bool", "JSON.getBool"),
        ("json_get_array", "JSON.getArray"), ("json_to_string", "JSON.toString"),
        // Http
        ("http_get", "Http.get"), ("http_post", "Http.post"),
        ("http_put", "Http.put"), ("http_patch", "Http.patch"),
        ("http_delete", "Http.delete"), ("http_status", "Http.status"),
        ("http_ok", "Http.ok"), ("http_text", "Http.text"),
        ("http_json", "Http.json"), ("http_header", "Http.header"),
    ];
    mappings.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
}

fn declare_fn<M: Module>(module: &mut M, name: &str, params: &[cranelift_codegen::ir::Type], returns: &[cranelift_codegen::ir::Type]) -> FuncId {
    let mut sig = module.make_signature();
    for &p in params { sig.params.push(cranelift_codegen::ir::AbiParam::new(p)); }
    for &r in returns { sig.returns.push(cranelift_codegen::ir::AbiParam::new(r)); }
    module.declare_function(name, Linkage::Import, &sig).unwrap()
}
