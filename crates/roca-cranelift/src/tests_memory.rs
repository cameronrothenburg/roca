//! Generic memory tests — verifies scope cleanup, variable lifecycle, and
//! heap management using the Body API directly. No AST, no Roca source.
//!
//! Each test builds a JIT function via Function::new().build(), runs it,
//! and asserts allocs == frees via the MemTracker.

use roca_types::RocaType;
use crate::api::{Function, Body, MutRef};
use crate::module::JitModule;
use crate::{CompiledFuncs, RuntimeFuncs, register_symbols, declare_runtime, MEM};

// ─── Test helpers ────────────────────────────────────

fn jit_module() -> (JitModule, RuntimeFuncs, CompiledFuncs) {
    let mut module = JitModule::new(register_symbols);
    let rt = declare_runtime(&mut *module);
    let compiled = CompiledFuncs::new();
    (module, rt, compiled)
}

fn build_f64(
    module: &mut JitModule,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    name: &str,
    body_fn: impl FnOnce(&mut Body),
) {
    Function::new(name)
        .returns(RocaType::Number)
        .build(&mut **module, rt, compiled, body_fn)
        .unwrap();
}

fn build_f64_param(
    module: &mut JitModule,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    name: &str,
    param: &str,
    body_fn: impl FnOnce(&mut Body),
) {
    Function::new(name)
        .param(param, RocaType::Number)
        .returns(RocaType::Number)
        .build(&mut **module, rt, compiled, body_fn)
        .unwrap();
}

fn finalize_and_get(module: &mut JitModule, name: &str) -> *const u8 {
    module.finalize().unwrap();
    module.get_function_ptr(name).unwrap()
}

macro_rules! mem_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            MEM.reset();
            $body
        }
    };
}

// ─── Scope cleanup ──────────────────────────────────

mem_test!(let_string_cleaned_at_scope_exit, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.string("hello");
        body.let_var_typed("s", s, RocaType::String);
        let r = body.number(42.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 42.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert!(allocs >= 1, "should allocate string");
    assert_eq!(allocs, frees, "scope cleanup: {} allocs, {} frees", allocs, frees);
});

mem_test!(const_string_cleaned_at_scope_exit, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.string("constant");
        body.const_var("s", s);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert!(allocs >= 1, "should allocate string");
    assert_eq!(allocs, frees, "const freed: {} allocs, {} frees", allocs, frees);
});

// ─── Reassignment ───────────────────────────────────

mem_test!(let_reassign_cleans_old_value, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s1 = body.string("first");
        let var = body.let_var("s", s1);
        let s2 = body.string("second");
        body.assign(&var, s2);
        let s3 = body.string("third");
        body.assign(&var, s3);
        let r = body.number(42.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 42.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 3, "should allocate 3 strings");
    assert_eq!(allocs, frees, "reassign freed: {} allocs, {} frees", allocs, frees);
});

// ─── If/else branch cleanup ─────────────────────────

mem_test!(if_else_cleans_branch_locals, {
    let (mut m, rt, mut c) = jit_module();
    build_f64_param(&mut m, &rt, &mut c, "test", "n", |body| {
        let a = body.string("always");
        body.const_var("a", a);
        let n = body.var("n");
        let zero = body.number(0.0);
        let cond = body.gt(n, zero);
        body.if_else(cond,
            |b| {
                let s = b.string("positive");
                b.const_var("branch", s);
                let r = b.number(1.0);
                b.return_val(r);
            },
            |b| {
                let s = b.string("negative");
                b.const_var("branch", s);
                let r = b.number(0.0);
                b.return_val(r);
            },
        );
    });
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(finalize_and_get(&mut m, "test")) };

    MEM.reset();
    assert_eq!(f(5.0), 1.0);
    let (a1, f1, _, _, _) = MEM.stats();
    assert_eq!(a1, f1, "positive branch: {} allocs, {} frees", a1, f1);

    MEM.reset();
    assert_eq!(f(-5.0), 0.0);
    let (a2, f2, _, _, _) = MEM.stats();
    assert_eq!(a2, f2, "negative branch: {} allocs, {} frees", a2, f2);
});

mem_test!(early_return_cleans_scope, {
    let (mut m, rt, mut c) = jit_module();
    build_f64_param(&mut m, &rt, &mut c, "test", "n", |body| {
        let s = body.string("local");
        body.const_var("s", s);
        let n = body.var("n");
        let zero = body.number(0.0);
        let cond = body.gt(n, zero);
        body.if_else(cond,
            |b| {
                let r = b.number(1.0);
                b.return_val(r);
            },
            |b| {
                let r = b.number(0.0);
                b.return_val(r);
            },
        );
    });
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(finalize_and_get(&mut m, "test")) };

    MEM.reset();
    assert_eq!(f(1.0), 1.0);
    let (a1, f1, _, _, _) = MEM.stats();
    assert_eq!(a1, f1, "early return: {} allocs, {} frees", a1, f1);

    MEM.reset();
    assert_eq!(f(-1.0), 0.0);
    let (a2, f2, _, _, _) = MEM.stats();
    assert_eq!(a2, f2, "else return: {} allocs, {} frees", a2, f2);
});

mem_test!(nested_branches_clean_inner_vars, {
    let (mut m, rt, mut c) = jit_module();
    build_f64_param(&mut m, &rt, &mut c, "test", "n", |body| {
        let s = body.string("outer");
        body.const_var("outer", s);
        let n = body.var("n");
        let zero = body.number(0.0);
        let cond = body.gt(n, zero);
        body.if_else(cond,
            |b| {
                let s = b.string("inner_then");
                b.const_var("inner", s);
                let r = b.number(1.0);
                b.return_val(r);
            },
            |b| {
                let s = b.string("inner_else");
                b.const_var("inner", s);
                let r = b.number(0.0);
                b.return_val(r);
            },
        );
    });
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(finalize_and_get(&mut m, "test")) };

    MEM.reset();
    f(1.0);
    let (a, fr, _, _, _) = MEM.stats();
    assert_eq!(a, fr, "nested cleanup: {} allocs, {} frees", a, fr);
});

// ─── While loop cleanup ─────────────────────────────

mem_test!(while_loop_cleans_body_each_iteration, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let zero = body.number(0.0);
        body.let_var_typed("i", zero, RocaType::Number);
        body.while_loop(
            |b| {
                let i = b.var("i");
                let limit = b.number(3.0);
                b.lt(i, limit)
            },
            |b| {
                let s = b.string("iteration");
                b.const_var("msg", s);
                let i = b.var("i");
                let one = b.number(1.0);
                let next = b.add(i, one);
                b.assign_name("i", next);
            },
        );
        let i = body.var("i");
        body.return_val(i);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 3.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 3, "should allocate 3 strings (one per iteration)");
    assert_eq!(allocs, frees, "loop cleanup: {} allocs, {} frees", allocs, frees);
});

mem_test!(break_cleans_loop_locals, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let zero = body.number(0.0);
        body.let_var_typed("i", zero, RocaType::Number);
        body.while_loop(
            |b| {
                let i = b.var("i");
                let limit = b.number(100.0);
                b.lt(i, limit)
            },
            |b| {
                let s = b.string("msg");
                b.const_var("msg", s);
                let i = b.var("i");
                let five = b.number(5.0);
                let cond = b.eq(i, five);
                b.if_then(cond, |b| { b.break_loop(); });
                let i = b.var("i");
                let one = b.number(1.0);
                let next = b.add(i, one);
                b.assign_name("i", next);
            },
        );
        let i = body.var("i");
        body.return_val(i);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 5.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "break cleanup: {} allocs, {} frees", allocs, frees);
});

mem_test!(loop_reassign_cleans_old_each_iteration, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.string("init");
        let var = body.let_var("s", s);
        let zero = body.number(0.0);
        body.let_var_typed("i", zero, RocaType::Number);
        let var_clone = var.clone();
        body.while_loop(
            |b| {
                let i = b.var("i");
                let limit = b.number(3.0);
                b.lt(i, limit)
            },
            move |b| {
                let new_s = b.string("updated");
                b.assign(&var_clone, new_s);
                let i = b.var("i");
                let one = b.number(1.0);
                let next = b.add(i, one);
                b.assign_name("i", next);
            },
        );
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // 1 "init" + 3 "updated" = 4 allocs, all freed (3 reassign frees + 1 scope exit)
    assert_eq!(allocs, 4, "should allocate 4 strings");
    assert_eq!(allocs, frees, "loop reassign: {} allocs, {} frees", allocs, frees);
});

// ─── For-each cleanup ───────────────────────────────

mem_test!(for_each_cleans_body_each_iteration, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let v1 = body.number(1.0);
        let v2 = body.number(2.0);
        let v3 = body.number(3.0);
        let arr = body.array(&[v1, v2, v3]);
        body.const_var_typed("arr", arr, RocaType::Array(Box::new(RocaType::Number)));
        let arr_val = body.var("arr");
        body.for_each("item", arr_val, |b| {
            let s = b.string("inside");
            b.const_var("msg", s);
        });
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // 3 "inside" strings + 1 array = at least 4
    assert!(allocs >= 4, "should allocate array + 3 strings");
    assert_eq!(allocs, frees, "for_each cleanup: {} allocs, {} frees", allocs, frees);
});

// ─── Collection cleanup ─────────────────────────────

mem_test!(array_cleaned_at_scope_exit, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let v1 = body.number(1.0);
        let v2 = body.number(2.0);
        let arr = body.array(&[v1, v2]);
        body.const_var("arr", arr);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert!(allocs >= 1, "should allocate array");
    assert_eq!(allocs, frees, "array freed: {} allocs, {} frees", allocs, frees);
});

mem_test!(struct_cleaned_at_scope_exit, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let x = body.number(10.0);
        let y = body.number(20.0);
        let s = body.struct_lit("Point", &[("x", x), ("y", y)]);
        body.const_var_typed("p", s, RocaType::Struct("Point".into()));
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert!(allocs >= 1, "should allocate struct");
    assert_eq!(allocs, frees, "struct freed: {} allocs, {} frees", allocs, frees);
});

mem_test!(enum_variant_cleaned_at_scope_exit, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let v = body.enum_variant("Color", "Red", &[]);
        body.const_var_typed("c", v, RocaType::Enum("Color".into()));
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert!(allocs >= 1, "should allocate enum");
    assert_eq!(allocs, frees, "enum freed: {} allocs, {} frees", allocs, frees);
});

// ─── String concat intermediates ────────────────────

mem_test!(concat_result_cleaned_at_scope_exit, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let a = body.string("hello");
        body.const_var("a", a);
        let b = body.string("world");
        body.const_var("b", b);
        let a_val = body.var("a");
        let b_val = body.var("b");
        let result = body.string_concat(a_val, b_val);
        body.const_var("result", result);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // 2 input strings + 1 concat result = 3 allocs, all freed at scope exit
    assert_eq!(allocs, 3, "should allocate 3 strings");
    assert_eq!(allocs, frees, "concat result freed: {} allocs, {} frees", allocs, frees);
});

// ─── Error return cleanup ───────────────────────────

mem_test!(error_return_cleans_all_locals, {
    let (mut m, rt, mut c) = jit_module();
    Function::new("test")
        .param("n", RocaType::Number)
        .returns(RocaType::Number)
        .returns_err()
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("local");
            body.const_var("s", s);
            let n = body.var("n");
            let zero = body.number(0.0);
            let cond = body.lt(n, zero);
            body.if_then(cond, |b| {
                b.return_err("invalid");
            });
            let r = body.number(1.0);
            body.return_val(r);
        })
        .unwrap();

    let f = unsafe { std::mem::transmute::<_, fn(f64) -> (f64, u8)>(finalize_and_get(&mut m, "test")) };

    MEM.reset();
    let (val, err) = f(5.0);
    assert_eq!(val, 1.0);
    assert_eq!(err, 0);
    let (a1, f1, _, _, _) = MEM.stats();
    assert_eq!(a1, f1, "ok path: {} allocs, {} frees", a1, f1);

    MEM.reset();
    let (_val, err) = f(-5.0);
    assert_ne!(err, 0);
    let (a2, f2, _, _, _) = MEM.stats();
    assert_eq!(a2, f2, "err path: {} allocs, {} frees", a2, f2);
});

// ─── Cross-function ownership ───────────────────────

mem_test!(caller_owns_callee_return_value, {
    let (mut m, rt, mut c) = jit_module();

    // Function that returns a string
    Function::new("make_str")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("from_callee");
            body.return_val(s);
        })
        .unwrap();

    // Function that calls make_str and uses the result
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.call("make_str", &[]);
        body.const_var_typed("result", s, RocaType::String);
        let r = body.number(1.0);
        body.return_val(r);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "cross-function: {} allocs, {} frees", allocs, frees);
});

// ─── Bug 4: call_multi result bound with correct type is freed ────────

mem_test!(error_tuple_result_cleaned_by_caller, {
    let (mut m, rt, mut c) = jit_module();

    // Function that returns a string + error flag
    Function::new("make_str")
        .returns(RocaType::String)
        .returns_err()
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("result_string");
            body.return_val(s);
        })
        .unwrap();

    // Caller binds with correct type so cleanup works
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let results = body.call_multi("make_str", &[]);
        if !results.is_empty() {
            body.const_var_typed("val", results[0], RocaType::String);
        }
        let r = body.number(1.0);
        body.return_val(r);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert!(allocs >= 1, "should allocate string");
    assert_eq!(allocs, frees, "call_multi result: {} allocs, {} frees", allocs, frees);
});

// ─── Category 2: Return value ownership ─────────────

mem_test!(return_transfers_string_ownership, {
    let (mut m, rt, mut c) = jit_module();

    // Function returns a string — caller must receive a valid pointer
    Function::new("make")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("owned_by_caller");
            body.return_val(s);
        })
        .unwrap();

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> i64>(finalize_and_get(&mut m, "make")) };
    let ptr = f();
    // The returned string should be ALIVE — not freed by callee
    assert_ne!(ptr, 0, "return value should be a valid pointer");
    let result = unsafe { std::ffi::CStr::from_ptr(ptr as *const i8) }.to_str().unwrap();
    assert_eq!(result, "owned_by_caller");
    // Caller frees it
    roca_runtime::roca_free(ptr);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "return ownership: {} allocs, {} frees", allocs, frees);
});

mem_test!(return_cleans_non_returned_locals, {
    let (mut m, rt, mut c) = jit_module();

    // Function has locals AND returns a string stored in a variable
    // The local should be freed, but the return value (also a local) should survive
    Function::new("make")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let local = body.string("will_be_freed");
            body.const_var("local", local);
            let result = body.string("returned");
            body.const_var("result", result);
            let r = body.var("result");
            body.return_val(r);
        })
        .unwrap();

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> i64>(finalize_and_get(&mut m, "make")) };
    let ptr = f();
    assert_ne!(ptr, 0);
    let result = unsafe { std::ffi::CStr::from_ptr(ptr as *const i8) }.to_str().unwrap();
    assert_eq!(result, "returned");
    // At this point: 2 allocs, 1 free (local cleaned, return survived)
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 2, "2 strings allocated");
    assert_eq!(frees, 1, "1 local freed by callee");
    // Caller frees the return value
    roca_runtime::roca_free(ptr);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "all cleaned: {} allocs, {} frees", allocs, frees);
});

// ─── Category 3: Struct field ownership ─────────────

mem_test!(struct_owns_string_fields, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let name = body.string("alice");
        let age = body.number(30.0);
        let s = body.struct_lit("User", &[("name", name), ("age", age)]);
        body.const_var("u", s);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // 1 string "alice" + 1 struct = 2 allocs, both freed
    assert!(allocs >= 2, "string + struct allocated");
    assert_eq!(allocs, frees, "struct owns fields: {} allocs, {} frees", allocs, frees);
});

mem_test!(struct_with_numbers_only_cleans_struct, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let x = body.number(1.0);
        let y = body.number(2.0);
        let s = body.struct_lit("Point", &[("x", x), ("y", y)]);
        body.const_var("p", s);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // Only the struct itself — no string fields to free
    assert_eq!(allocs, 1, "just the struct");
    assert_eq!(allocs, frees, "numbers-only struct: {} allocs, {} frees", allocs, frees);
});

mem_test!(struct_owns_multiple_heap_fields, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let first = body.string("alice");
        let last = body.string("smith");
        let s = body.struct_lit("Name", &[("first", first), ("last", last)]);
        body.const_var("n", s);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // 2 strings + 1 struct = 3 allocs
    assert_eq!(allocs, 3, "2 strings + 1 struct");
    assert_eq!(allocs, frees, "multiple heap fields: {} allocs, {} frees", allocs, frees);
});

mem_test!(enum_variant_owns_data_fields, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let msg = body.string("something went wrong");
        let v = body.enum_variant("Result", "Error", &[msg]);
        body.const_var("r", v);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // tag string "Error" + data string "something went wrong" + enum struct = 3
    assert!(allocs >= 3, "tag + data + enum");
    assert_eq!(allocs, frees, "enum owns data: {} allocs, {} frees", allocs, frees);
});

// ─── Category 6 (new): Cross-function cleanup ───────

mem_test!(callee_cleans_own_locals, {
    let (mut m, rt, mut c) = jit_module();

    // Callee allocates a local string and returns a number
    build_f64(&mut m, &rt, &mut c, "callee", |body| {
        let s = body.string("callee_local");
        body.const_var("s", s);
        let r = body.number(99.0);
        body.return_val(r);
    });

    // Caller calls callee
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let result = body.call("callee", &[]);
        body.return_val(result);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 99.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "callee cleans locals: {} allocs, {} frees", allocs, frees);
});

mem_test!(call_chain_cleans_each_scope, {
    let (mut m, rt, mut c) = jit_module();

    build_f64(&mut m, &rt, &mut c, "c_func", |body| {
        let s = body.string("c_local");
        body.const_var("s", s);
        let r = body.number(1.0);
        body.return_val(r);
    });

    build_f64(&mut m, &rt, &mut c, "b_func", |body| {
        let s = body.string("b_local");
        body.const_var("s", s);
        let r = body.call("c_func", &[]);
        body.return_val(r);
    });

    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.string("a_local");
        body.const_var("s", s);
        let r = body.call("b_func", &[]);
        body.return_val(r);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 3, "3 locals across 3 functions");
    assert_eq!(allocs, frees, "call chain: {} allocs, {} frees", allocs, frees);
});

// ─── Category 9: Reassignment edge cases ────────────

mem_test!(reassign_same_literal_cleans_old, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s1 = body.string("hello");
        let var = body.let_var("s", s1);
        let s2 = body.string("hello"); // same content, new allocation
        body.assign(&var, s2);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 2, "2 strings even though same content");
    assert_eq!(allocs, frees, "reassign same literal: {} allocs, {} frees", allocs, frees);
});

// ─── Category 10: Null safety ───────────────────────

mem_test!(free_null_is_noop, {
    // Directly call roca_free(0) — should not crash or count
    roca_runtime::roca_free(0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 0, "no allocs");
    assert_eq!(frees, 0, "no frees");
});

mem_test!(default_return_is_clean, {
    let (mut m, rt, mut c) = jit_module();
    // Function with no explicit return — Body emits default return
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.string("local");
        body.const_var("s", s);
        // No return_val — Body's auto-default kicks in
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    let _result = f();
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "default return: {} allocs, {} frees", allocs, frees);
});
