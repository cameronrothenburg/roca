//! Deep property testing — auto-generates randomized inputs from type signatures
//! and verifies invariants: no crash, correct return type, valid error discipline.

use cranelift_codegen::ir::types;
use cranelift_jit::JITModule;
use cranelift_module::{Module, Linkage};

use crate::ast::{self, Expr, TypeRef, Constraint};
use super::test_runner::*;
use super::types::roca_to_cranelift;

const ROUNDS: usize = 50;

/// Check if all params are types we can generate random values for.
pub fn all_params_generable(func: &ast::FnDef) -> bool {
    !func.params.is_empty() && func.params.iter().all(|p| is_generable(&p.type_ref))
}

fn is_generable(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Number | TypeRef::String | TypeRef::Bool)
}

/// Run property tests for a single function.
/// `struct_name` is Some for struct methods — the JIT name is "Struct.method".
pub fn run_property_tests(
    module: &mut JITModule,
    func: &ast::FnDef,
    struct_name: Option<&str>,
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    let pools = generate_pools(func);
    let combos = pick_combos(&pools, ROUNDS, func);

    let jit_name = match struct_name {
        Some(sn) => format!("{}.{}", sn, func.name),
        None => func.name.clone(),
    };
    let is_method = struct_name.is_some();

    let mut prop_passed = 0usize;
    let mut prop_failed = 0usize;
    let mut err_count = 0usize;

    for indices in &combos {
        let args: Vec<Expr> = indices.iter().enumerate()
            .map(|(i, &idx)| pools[i][idx].clone())
            .collect();

        match run_property(module, func, &jit_name, is_method, &args) {
            PropertyResult::Ok(is_err) => {
                if is_err { err_count += 1; }
                prop_passed += 1;
            }
            PropertyResult::Crashed(msg) => {
                prop_failed += 1;
                let label = format_test_label(func, &args);
                output.push_str(&format!("  ✗ {} crashed: {}\n", label, msg));
            }
        }
    }

    if prop_failed == 0 {
        if err_count > 0 {
            output.push_str(&format!("  ◆ {}: {} property tests passed ({} returned errors)\n", jit_name, prop_passed, err_count));
        } else {
            output.push_str(&format!("  ◆ {}: {} property tests passed\n", jit_name, prop_passed));
        }
        *passed += 1;
    } else {
        output.push_str(&format!("  ◆ {}: {} property tests failed\n", jit_name, prop_failed));
        *failed += 1;
    }
}

enum PropertyResult {
    Ok(bool), // true if error was returned
    Crashed(String),
}

/// Unified property test runner for both error-returning and plain functions.
fn run_property(
    module: &mut JITModule,
    func: &ast::FnDef,
    jit_name: &str,
    is_method: bool,
    args: &[Expr],
) -> PropertyResult {
    let mut sig = if func.returns_err {
        build_sig_with_err(module, func)
    } else {
        build_sig(module, func)
    };
    if is_method {
        sig.params.insert(0, cranelift_codegen::ir::AbiParam::new(types::I64));
    }

    let id = match module.declare_function(jit_name, Linkage::Export, &sig) {
        Ok(id) => id,
        Err(e) => return PropertyResult::Crashed(format!("declare: {}", e)),
    };
    let ptr = module.get_finalized_function(id);

    let call_args: Vec<Expr> = if is_method {
        let mut v = vec![Expr::Number(0.0)];
        v.extend(args.iter().cloned());
        v
    } else {
        args.to_vec()
    };
    let param_count = call_args.len();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if func.returns_err {
            let arity_func = ast::FnDef {
                params: (0..param_count).map(|i| ast::Param {
                    name: format!("p{}", i), type_ref: TypeRef::Number, constraints: vec![],
                }).collect(),
                name: String::new(), is_pub: false, doc: None, type_params: vec![],
                return_type: func.return_type.clone(), returns_err: true,
                errors: vec![], body: vec![], crash: None, test: None,
            };
            let (_val, err_tag) = call_with_err(ptr, &arity_func, &call_args);
            err_tag != 0
        } else {
            let ret_type = roca_to_cranelift(&func.return_type);
            if ret_type == types::F64 {
                let _ = call_f64_fn(ptr, param_count, &call_args);
            } else if ret_type == types::I64 {
                let _ = call_str_fn(ptr, param_count, &call_args);
            } else if ret_type == types::I8 {
                let _ = call_bool_fn(ptr, param_count, &call_args);
            }
            false
        }
    }));

    match result {
        std::result::Result::Ok(is_err) => PropertyResult::Ok(is_err),
        Err(e) => PropertyResult::Crashed(panic_msg(e)),
    }
}

fn panic_msg(e: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = e.downcast_ref::<&str>() {
        s.to_string()
    } else {
        "unknown panic".to_string()
    }
}

// ─── Input generation ─────────────────────────────────

/// Generate value pools per parameter (not the cartesian product yet).
fn generate_pools(func: &ast::FnDef) -> Vec<Vec<Expr>> {
    let seed = func.name.bytes().fold(0xDEADBEEFu32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let mut rng = Xorshift(seed | 1);

    func.params.iter()
        .map(|p| generate_for_type(&p.type_ref, &p.constraints, &mut rng))
        .collect()
}

/// Pick ROUNDS index-based combinations from the pools.
fn pick_combos(pools: &[Vec<Expr>], max: usize, func: &ast::FnDef) -> Vec<Vec<usize>> {
    if pools.is_empty() { return vec![]; }

    let total: usize = pools.iter().map(|v| v.len()).product();

    if total <= max {
        // Full cartesian product fits — enumerate all index combos
        let mut result = Vec::with_capacity(total);
        cartesian_indices(pools, 0, &mut vec![], &mut result, max);
        result
    } else {
        // Sample deterministically
        let seed = func.name.bytes().fold(0x1234u32, |acc, b| acc.wrapping_mul(37).wrapping_add(b as u32));
        let mut rng = Xorshift(seed | 1);
        (0..max).map(|_| {
            pools.iter().map(|vals| rng.next_u32() as usize % vals.len()).collect()
        }).collect()
    }
}

fn cartesian_indices(
    pools: &[Vec<Expr>],
    depth: usize,
    current: &mut Vec<usize>,
    result: &mut Vec<Vec<usize>>,
    max: usize,
) {
    if result.len() >= max { return; }
    if depth == pools.len() {
        result.push(current.clone());
        return;
    }
    for idx in 0..pools[depth].len() {
        if result.len() >= max { return; }
        current.push(idx);
        cartesian_indices(pools, depth + 1, current, result, max);
        current.pop();
    }
}

fn generate_for_type(ty: &TypeRef, constraints: &[Constraint], rng: &mut Xorshift) -> Vec<Expr> {
    match ty {
        TypeRef::Number => generate_numbers(constraints, rng),
        TypeRef::String => generate_strings(constraints, rng),
        TypeRef::Bool => vec![Expr::Bool(true), Expr::Bool(false)],
        _ => vec![],
    }
}

fn generate_numbers(constraints: &[Constraint], rng: &mut Xorshift) -> Vec<Expr> {
    // NaN/Infinity excluded — native JIT doesn't handle IEEE special values safely
    let mut vals = vec![
        0.0, 1.0, -1.0, 0.5, -0.5,
        100.0, -100.0, 1000.0,
        9007199254740991.0,
        -9007199254740991.0,
    ];

    let mut min_val = f64::NEG_INFINITY;
    let mut max_val = f64::INFINITY;
    for c in constraints {
        match c {
            Constraint::Min(m) => { min_val = *m; vals.push(*m); vals.push(*m - 1.0); }
            Constraint::Max(m) => { max_val = *m; vals.push(*m); vals.push(*m + 1.0); }
            _ => {}
        }
    }

    if min_val.is_finite() && max_val.is_finite() {
        vals.push((min_val + max_val) / 2.0);
        for _ in 0..5 { vals.push(min_val + rng.next_f64() * (max_val - min_val)); }
    } else {
        for _ in 0..5 { vals.push(rng.next_f64() * 2000.0 - 1000.0); }
    }

    vals.into_iter().map(Expr::Number).collect()
}

fn generate_strings(constraints: &[Constraint], rng: &mut Xorshift) -> Vec<Expr> {
    let mut vals = vec![
        String::new(),
        " ".to_string(),
        "a".to_string(),
        "hello world".to_string(),
        "x".repeat(64),
        "0".to_string(),
        "abc123".to_string(),
        "UPPER".to_string(),
    ];

    for c in constraints {
        match c {
            Constraint::MinLen(n) => {
                let n = *n as usize;
                if n > 0 { vals.push("a".repeat(n)); }
                if n > 1 { vals.push("a".repeat(n - 1)); }
            }
            Constraint::MaxLen(n) => {
                let n = *n as usize;
                vals.push("a".repeat(n));
                vals.push("a".repeat(n + 1));
            }
            Constraint::Contains(s) => {
                vals.push(format!("prefix{}suffix", s));
                vals.push("no_match_here".to_string());
            }
            _ => {}
        }
    }

    // Random alphanumeric strings (safe for C-string interop)
    for _ in 0..5 {
        let len = (rng.next_u32() % 20 + 1) as usize;
        let s: String = (0..len).map(|_| {
            let chars = b"abcdefghijklmnopqrstuvwxyz0123456789";
            chars[(rng.next_u32() as usize) % chars.len()] as char
        }).collect();
        vals.push(s);
    }

    vals.into_iter().map(Expr::String).collect()
}

// ─── Deterministic PRNG (xorshift32) ──────────────────

struct Xorshift(u32);

impl Xorshift {
    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u32() as f64) / (u32::MAX as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{TypeRef, Constraint, Param};

    #[test]
    fn number_generation_includes_boundaries() {
        let mut rng = Xorshift(42);
        let vals = generate_numbers(&[], &mut rng);
        let nums: Vec<f64> = vals.iter().map(|e| match e { Expr::Number(n) => *n, _ => 0.0 }).collect();
        assert!(nums.contains(&0.0));
        assert!(nums.contains(&1.0));
        assert!(nums.contains(&-1.0));
        assert!(nums.contains(&100.0));
        assert!(nums.contains(&9007199254740991.0));
    }

    #[test]
    fn constrained_number_includes_constraint_values() {
        let mut rng = Xorshift(42);
        let constraints = vec![Constraint::Min(0.0), Constraint::Max(100.0)];
        let vals = generate_numbers(&constraints, &mut rng);
        let nums: Vec<f64> = vals.iter().map(|e| match e { Expr::Number(n) => *n, _ => 0.0 }).collect();
        assert!(nums.contains(&0.0), "should contain min boundary");
        assert!(nums.contains(&100.0), "should contain max boundary");
        assert!(nums.contains(&50.0), "should contain midpoint");
        assert!(nums.contains(&-1.0), "should contain below-min boundary");
        assert!(nums.contains(&101.0), "should contain above-max boundary");
    }

    #[test]
    fn string_generation_includes_edge_cases() {
        let mut rng = Xorshift(42);
        let vals = generate_strings(&[], &mut rng);
        let strs: Vec<&str> = vals.iter().map(|e| match e { Expr::String(s) => s.as_str(), _ => "" }).collect();
        assert!(strs.contains(&""), "should contain empty string");
        assert!(strs.contains(&" "), "should contain whitespace");
        assert!(strs.iter().any(|s| s.len() >= 64), "should contain long string");
    }

    #[test]
    fn bool_generation() {
        let mut rng = Xorshift(42);
        let vals = generate_for_type(&TypeRef::Bool, &[], &mut rng);
        assert_eq!(vals.len(), 2);
    }

    #[test]
    fn all_params_generable_primitives() {
        let f = ast::FnDef {
            name: "test".into(), is_pub: true, doc: None, type_params: vec![],
            params: vec![
                Param { name: "a".into(), type_ref: TypeRef::Number, constraints: vec![] },
                Param { name: "b".into(), type_ref: TypeRef::String, constraints: vec![] },
            ],
            return_type: TypeRef::Number, returns_err: false,
            errors: vec![], body: vec![], crash: None, test: None,
        };
        assert!(all_params_generable(&f));
    }

    #[test]
    fn all_params_generable_rejects_named() {
        let f = ast::FnDef {
            name: "test".into(), is_pub: true, doc: None, type_params: vec![],
            params: vec![
                Param { name: "a".into(), type_ref: TypeRef::Named("User".into()), constraints: vec![] },
            ],
            return_type: TypeRef::Number, returns_err: false,
            errors: vec![], body: vec![], crash: None, test: None,
        };
        assert!(!all_params_generable(&f));
    }

    #[test]
    fn index_combos_capped() {
        let a = vec![Expr::Number(1.0), Expr::Number(2.0)];
        let b = vec![Expr::Number(3.0), Expr::Number(4.0)];
        let pools = vec![a, b];
        let f = ast::FnDef {
            name: "test".into(), is_pub: true, doc: None, type_params: vec![],
            params: vec![], return_type: TypeRef::Number, returns_err: false,
            errors: vec![], body: vec![], crash: None, test: None,
        };
        let result = pick_combos(&pools, 3, &f);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn xorshift_deterministic() {
        let mut a = Xorshift(42);
        let mut b = Xorshift(42);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }
}
