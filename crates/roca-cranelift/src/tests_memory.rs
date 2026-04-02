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
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.string("local");
        body.const_var("s", s);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    let _result = f();
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "default return: {} allocs, {} frees", allocs, frees);
});

// ─── Category 2 (remaining): Return array/struct ownership ──

mem_test!(return_transfers_array_ownership, {
    let (mut m, rt, mut c) = jit_module();
    Function::new("make")
        .returns(RocaType::Array(Box::new(RocaType::Number)))
        .build(&mut *m, &rt, &mut c, |body| {
            let v1 = body.number(1.0);
            let v2 = body.number(2.0);
            let arr = body.array(&[v1, v2]);
            body.const_var("arr", arr);
            let r = body.var("arr");
            body.return_val(r);
        })
        .unwrap();

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> i64>(finalize_and_get(&mut m, "make")) };
    let ptr = f();
    assert_ne!(ptr, 0, "array should survive return");
    roca_runtime::roca_free(ptr);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "return array: {} allocs, {} frees", allocs, frees);
});

mem_test!(return_transfers_struct_ownership, {
    let (mut m, rt, mut c) = jit_module();
    Function::new("make")
        .returns(RocaType::Struct("Point".into()))
        .build(&mut *m, &rt, &mut c, |body| {
            let x = body.number(10.0);
            let y = body.number(20.0);
            let s = body.struct_lit("Point", &[("x", x), ("y", y)]);
            body.const_var("p", s);
            let r = body.var("p");
            body.return_val(r);
        })
        .unwrap();

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> i64>(finalize_and_get(&mut m, "make")) };
    let ptr = f();
    assert_ne!(ptr, 0, "struct should survive return");
    roca_runtime::roca_free(ptr);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "return struct: {} allocs, {} frees", allocs, frees);
});

// ─── Category 3 (remaining): Nested struct ──────────

mem_test!(nested_struct_cleans_recursively, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        // Inner struct
        let ix = body.number(1.0);
        let iy = body.number(2.0);
        let inner = body.struct_lit("Inner", &[("x", ix), ("y", iy)]);
        // Outer struct that contains inner
        let name = body.string("outer");
        let outer = body.struct_lit("Outer", &[("name", name), ("inner", inner)]);
        body.const_var("o", outer);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // string "outer" + inner struct + outer struct = 3
    assert_eq!(allocs, 3, "string + inner + outer");
    assert_eq!(allocs, frees, "nested struct: {} allocs, {} frees", allocs, frees);
});

// ─── Category 7 (remaining): String interp + concat chain ──

mem_test!(string_interp_cleaned_at_scope_exit, {
    let (mut m, rt, mut c) = jit_module();
    use crate::api::StringPart;
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let name = body.string("world");
        body.const_var("name", name);
        let n = body.var("name");
        let result = body.string_interp(&[
            StringPart::Lit("hello ".to_string()),
            StringPart::Expr(n),
        ]);
        body.const_var("msg", result);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "string interp: {} allocs, {} frees", allocs, frees);
});

// ─── Category 9 (remaining): Reassignment in branch ─

mem_test!(reassign_in_branch_cleans_old, {
    let (mut m, rt, mut c) = jit_module();
    build_f64_param(&mut m, &rt, &mut c, "test", "n", |body| {
        let s1 = body.string("initial");
        let var = body.let_var("s", s1);
        let n = body.var("n");
        let zero = body.number(0.0);
        let cond = body.gt(n, zero);
        let var_clone = var.clone();
        body.if_then(cond, move |b| {
            let s2 = b.string("updated");
            b.assign(&var_clone, s2);
        });
        let r = body.number(1.0);
        body.return_val(r);
    });
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(finalize_and_get(&mut m, "test")) };

    // Branch taken: old "initial" freed by reassign, "updated" freed at scope exit
    MEM.reset();
    assert_eq!(f(1.0), 1.0);
    let (a1, f1, _, _, _) = MEM.stats();
    assert_eq!(a1, f1, "branch taken: {} allocs, {} frees", a1, f1);

    // Branch not taken: "initial" freed at scope exit
    MEM.reset();
    assert_eq!(f(-1.0), 1.0);
    let (a2, f2, _, _, _) = MEM.stats();
    assert_eq!(a2, f2, "branch skipped: {} allocs, {} frees", a2, f2);
});

mem_test!(reassign_then_return_transfers_final, {
    let (mut m, rt, mut c) = jit_module();
    Function::new("make")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s1 = body.string("first");
            let var = body.let_var("s", s1);
            let s2 = body.string("second");
            body.assign(&var, s2);
            let r = body.var("s");
            body.return_val(r);
        })
        .unwrap();

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> i64>(finalize_and_get(&mut m, "make")) };
    let ptr = f();
    let result = unsafe { std::ffi::CStr::from_ptr(ptr as *const i8) }.to_str().unwrap();
    assert_eq!(result, "second");
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 2);
    assert_eq!(frees, 1, "only first freed by callee");
    roca_runtime::roca_free(ptr);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "all cleaned: {} allocs, {} frees", allocs, frees);
});

// ─── Category 8: Temporary cleanup ──────────────────

mem_test!(unbound_string_cleaned_at_statement_end, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        body.string("orphan"); // created but never stored
        body.flush_temps(); // statement boundary — free orphans
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 1, "string was allocated");
    assert_eq!(allocs, frees, "temp cleaned: {} allocs, {} frees", allocs, frees);
});

mem_test!(unbound_array_cleaned_at_statement_end, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let v1 = body.number(1.0);
        body.array(&[v1]); // created but never stored
        body.flush_temps();
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert!(allocs >= 1, "array allocated");
    assert_eq!(allocs, frees, "temp array: {} allocs, {} frees", allocs, frees);
});

mem_test!(bound_value_not_flushed_as_temp, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.string("kept");
        body.const_var("s", s); // stored — not a temp
        body.flush_temps(); // should not free "kept"
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 1, "one string");
    assert_eq!(allocs, frees, "bound not flushed: {} allocs, {} frees", allocs, frees);
});

// ─── Runtime safety: edge cases ─────────────────────

mem_test!(empty_array_cleaned, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let arr = body.array(&[]);
        body.const_var("arr", arr);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "empty array: {} allocs, {} frees", allocs, frees);
});

mem_test!(empty_struct_cleaned, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s = body.struct_lit("Empty", &[]);
        body.const_var("e", s);
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "empty struct: {} allocs, {} frees", allocs, frees);
});

mem_test!(double_free_is_safe, {
    // roca_free called twice on same pointer — second call should be a noop
    let ptr = roca_runtime::alloc_str("test");
    assert_ne!(ptr, 0);
    roca_runtime::roca_free(ptr);
    roca_runtime::roca_free(ptr); // second free — tag already removed, should be noop
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 1, "one alloc");
    assert_eq!(frees, 1, "one free (second was noop)");
});

mem_test!(param_not_freed_by_callee, {
    let (mut m, rt, mut c) = jit_module();

    // Callee receives a number param — should not free it
    build_f64_param(&mut m, &rt, &mut c, "callee", "x", |body| {
        let x = body.var("x");
        body.return_val(x);
    });

    // Caller passes a value and uses it after the call
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let val = body.number(42.0);
        let result = body.call("callee", &[val]);
        body.return_val(result);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 42.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "param safe: {} allocs, {} frees", allocs, frees);
});

mem_test!(multiple_temps_all_cleaned, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        // Create 5 strings, store none
        body.string("a");
        body.string("b");
        body.string("c");
        body.string("d");
        body.string("e");
        body.flush_temps();
        let r = body.number(1.0);
        body.return_val(r);
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 5, "5 temps");
    assert_eq!(allocs, frees, "all temps cleaned: {} allocs, {} frees", allocs, frees);
});

mem_test!(mix_bound_and_unbound_cleaned, {
    let (mut m, rt, mut c) = jit_module();
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let s1 = body.string("bound");
        body.const_var("s", s1);      // claimed
        body.string("orphan1");        // temp
        body.string("orphan2");        // temp
        body.flush_temps();            // free orphans
        let r = body.number(1.0);
        body.return_val(r);            // free "bound" at scope exit
    });
    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, 3, "1 bound + 2 temps");
    assert_eq!(allocs, frees, "mix cleaned: {} allocs, {} frees", allocs, frees);
});

// ─── Integration: functions calling functions ───────

mem_test!(function_returns_string_caller_stores_and_cleans, {
    let (mut m, rt, mut c) = jit_module();

    // make_greeting() returns a string
    Function::new("make_greeting")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("hello world");
            body.return_val(s);
        })
        .unwrap();

    // test() calls make_greeting, stores result, returns a number
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let greeting = body.call("make_greeting", &[]);
        body.const_var("g", greeting);
        let r = body.number(1.0);
        body.return_val(r);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "call result stored and cleaned: {} allocs, {} frees", allocs, frees);
});

mem_test!(function_returns_string_caller_ignores_result, {
    let (mut m, rt, mut c) = jit_module();

    Function::new("make_greeting")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("ignored");
            body.return_val(s);
        })
        .unwrap();

    // test() calls make_greeting but doesn't store the result — it's a temp
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        body.call("make_greeting", &[]);
        body.flush_temps(); // the ignored return value should be freed
        let r = body.number(1.0);
        body.return_val(r);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    assert_eq!(allocs, frees, "ignored result cleaned: {} allocs, {} frees", allocs, frees);
});

mem_test!(while_loop_calls_function_each_iteration, {
    let (mut m, rt, mut c) = jit_module();

    // factory() returns a new string each call
    Function::new("factory")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("made");
            body.return_val(s);
        })
        .unwrap();

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
                // Call factory, store result in loop-local var
                let s = b.call("factory", &[]);
                b.const_var("item", s);
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
    assert_eq!(allocs, 3, "3 iterations, 3 strings");
    assert_eq!(allocs, frees, "loop call cleanup: {} allocs, {} frees", allocs, frees);
});

mem_test!(if_else_both_branches_call_function, {
    let (mut m, rt, mut c) = jit_module();

    Function::new("make_pos")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("positive");
            body.return_val(s);
        })
        .unwrap();

    Function::new("make_neg")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("negative");
            body.return_val(s);
        })
        .unwrap();

    build_f64_param(&mut m, &rt, &mut c, "test", "n", |body| {
        let n = body.var("n");
        let zero = body.number(0.0);
        let cond = body.gt(n, zero);
        body.if_else(cond,
            |b| {
                let s = b.call("make_pos", &[]);
                b.const_var("msg", s);
                let r = b.number(1.0);
                b.return_val(r);
            },
            |b| {
                let s = b.call("make_neg", &[]);
                b.const_var("msg", s);
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

mem_test!(value_created_in_branch_stays_in_branch, {
    let (mut m, rt, mut c) = jit_module();

    build_f64_param(&mut m, &rt, &mut c, "test", "n", |body| {
        let n = body.var("n");
        let zero = body.number(0.0);
        let cond = body.gt(n, zero);
        // String created inside branch should be cleaned there, not hoisted
        body.if_else(cond,
            |b| {
                let s = b.string("branch_only");
                b.const_var("local", s);
            },
            |_b| {},
        );
        // After the if-else, "local" should be gone — no leak
        let r = body.number(1.0);
        body.return_val(r);
    });

    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(finalize_and_get(&mut m, "test")) };

    MEM.reset();
    assert_eq!(f(1.0), 1.0);
    let (a1, f1, _, _, _) = MEM.stats();
    assert_eq!(a1, f1, "branch taken: {} allocs, {} frees", a1, f1);

    MEM.reset();
    assert_eq!(f(-1.0), 1.0);
    let (a2, f2, _, _, _) = MEM.stats();
    assert_eq!(a2, f2, "branch skipped: {} allocs, {} frees", a2, f2);
});

mem_test!(deeply_nested_calls_all_clean, {
    let (mut m, rt, mut c) = jit_module();

    // d() allocates and returns
    Function::new("d")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let local = body.string("d_local");
            body.const_var("l", local);
            let result = body.string("d_result");
            body.return_val(result);
        })
        .unwrap();

    // c() calls d(), stores result, allocates own local
    build_f64(&mut m, &rt, &mut c, "c", |body| {
        let from_d = body.call("d", &[]);
        body.const_var("from_d", from_d);
        let own = body.string("c_local");
        body.const_var("own", own);
        let r = body.number(1.0);
        body.return_val(r);
    });

    // b() calls c()
    build_f64(&mut m, &rt, &mut c, "b", |body| {
        let own = body.string("b_local");
        body.const_var("own", own);
        let r = body.call("c", &[]);
        body.return_val(r);
    });

    // a() calls b()
    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let own = body.string("a_local");
        body.const_var("own", own);
        let r = body.call("b", &[]);
        body.return_val(r);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // a_local + b_local + c_local + from_d + d_local + d_result = 6
    // d_result is returned from d, owned by c's "from_d", freed by c
    assert_eq!(allocs, frees, "deep nesting: {} allocs, {} frees", allocs, frees);
});

mem_test!(for_each_with_function_call_per_element, {
    let (mut m, rt, mut c) = jit_module();

    // process() takes a number, allocates a string, returns a number
    build_f64_param(&mut m, &rt, &mut c, "process", "x", |body| {
        let s = body.string("processed");
        body.const_var("label", s);
        let x = body.var("x");
        body.return_val(x);
    });

    build_f64(&mut m, &rt, &mut c, "test", |body| {
        let v1 = body.number(1.0);
        let v2 = body.number(2.0);
        let v3 = body.number(3.0);
        let arr = body.array(&[v1, v2, v3]);
        body.const_var("arr", arr);
        let arr_val = body.var("arr");
        body.for_each("item", arr_val, |b| {
            let item = b.var("item");
            let result = b.call("process", &[item]);
            b.const_var("r", result);
        });
        let r = body.number(1.0);
        body.return_val(r);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // array + 3x "processed" string (one per iteration, each cleaned by process())
    assert_eq!(allocs, frees, "for_each+call: {} allocs, {} frees", allocs, frees);
});

// ─── Hoisted variable assigned inside loop ──────────

mem_test!(hoisted_let_assigned_in_loop, {
    let (mut m, rt, mut c) = jit_module();

    // factory returns a new string each call
    Function::new("factory")
        .returns(RocaType::String)
        .build(&mut *m, &rt, &mut c, |body| {
            let s = body.string("item");
            body.return_val(s);
        })
        .unwrap();

    build_f64(&mut m, &rt, &mut c, "test", |body| {
        // Hoisted let — declared outside the loop
        let init = body.string("initial");
        let var = body.let_var("item", init);

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
                // Each iteration: call factory, assign result to hoisted var
                // Old value should be freed each time
                let new_val = b.call("factory", &[]);
                b.assign(&var_clone, new_val);

                let i = b.var("i");
                let one = b.number(1.0);
                let next = b.add(i, one);
                b.assign_name("i", next);
            },
        );

        // After loop: "item" holds the last value from factory
        let r = body.number(1.0);
        body.return_val(r);
    });

    MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(finalize_and_get(&mut m, "test")) };
    assert_eq!(f(), 1.0);
    let (allocs, frees, _, _, _) = MEM.stats();
    // "initial" + 3x "item" from factory = 4 allocs
    // 3 freed by reassignment + 1 freed at scope exit = 4 frees
    assert_eq!(allocs, 4, "initial + 3 loop iterations");
    assert_eq!(allocs, frees, "hoisted let: {} allocs, {} frees", allocs, frees);
});
