//! Memory management tests

use super::test_helpers::*;
use crate::native::runtime;

mem_test!(mem_let_reassign_frees_old, {
    let mut m = jit(r#"
        pub fn reassign() -> Number {
            let s = "first"
            s = "second"
            s = "third"
            return 42
        }
    "#);
    runtime::MEM.reset();
    assert_eq!(unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "reassign", 0)) }(), 42.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, 3, "should allocate 3 strings");
    assert_eq!(allocs, frees, "all reassigned freed: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_break_cleans_up, {
    let mut m = jit(r#"
        pub fn break_test() -> Number {
            let i = 0
            while i < 100 {
                const msg = "iteration"
                if i == 5 { break }
                i = i + 1
            }
            return i
        }
    "#);
    runtime::MEM.reset();
    assert_eq!(unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "break_test", 0)) }(), 5.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "break cleans up: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_array_freed_at_scope_exit, {
    let mut m = jit(r#"
        pub fn make_arr() -> Number {
            const arr = [1, 2, 3]
            return arr.length
        }
    "#);
    runtime::MEM.reset();
    assert_eq!(unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make_arr", 0)) }(), 3.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert!(allocs >= 1, "should allocate array");
    assert_eq!(allocs, frees, "array freed: {} allocs, {} frees", allocs, frees);
});

// ─── Cross-function & scope tracking ──────────────

mem_test!(mem_nested_if_scopes, {
    // Strings created in branches must all be freed
    let mut m = jit(r#"
        pub fn branchy(n: Number) -> Number {
            const a = "always"
            if n > 0 {
                const b = "positive"
                return 1
            } else {
                const c = "negative"
                return 0
            }
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "branchy", 1)) };
    assert_eq!(f(5.0), 1.0);
    let (a1, f1, _, _, _) = runtime::MEM.stats();
    assert_eq!(a1, f1, "positive branch: {} allocs, {} frees", a1, f1);

    runtime::MEM.reset();
    assert_eq!(f(-5.0), 0.0);
    let (a2, f2, _, _, _) = runtime::MEM.stats();
    assert_eq!(a2, f2, "negative branch: {} allocs, {} frees", a2, f2);
});

mem_test!(mem_string_concat_intermediates, {
    // String concat creates intermediates that must be freed
    let mut m = jit(r#"
        pub fn concat_test() -> Number {
            const a = "hello"
            const b = " "
            const c = "world"
            const result = a + b + c
            return result.length
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "concat_test", 0)) };
    assert_eq!(f(), 11.0); // "hello world"
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "concat intermediates freed: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_multiple_returns_all_clean, {
    // Function with early returns — all paths must clean up
    let mut m = jit(r#"
        pub fn early(n: Number) -> Number {
            const always = "setup"
            if n == 1 {
                const branch1 = "one"
                return 1
            }
            if n == 2 {
                const branch2 = "two"
                return 2
            }
            const fallthrough = "default"
            return 0
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "early", 1)) };

    // Path 1: n=1
    runtime::MEM.reset();
    assert_eq!(f(1.0), 1.0);
    let (a1, f1, _, _, _) = runtime::MEM.stats();
    assert_eq!(a1, f1, "n=1 path: {} allocs, {} frees", a1, f1);

    // Path 2: n=2
    runtime::MEM.reset();
    assert_eq!(f(2.0), 2.0);
    let (a2, f2, _, _, _) = runtime::MEM.stats();
    assert_eq!(a2, f2, "n=2 path: {} allocs, {} frees", a2, f2);

    // Path 3: fallthrough
    runtime::MEM.reset();
    assert_eq!(f(99.0), 0.0);
    let (a3, f3, _, _, _) = runtime::MEM.stats();
    assert_eq!(a3, f3, "default path: {} allocs, {} frees", a3, f3);
});

mem_test!(mem_loop_with_string_reassign, {
    // String reassignment inside a loop — old values freed each iteration
    let mut m = jit(r#"
        pub fn build() -> Number {
            let msg = "start"
            let i = 0
            while i < 3 {
                msg = "iter"
                i = i + 1
            }
            return i
        }
    "#);
    runtime::MEM.reset();
    assert_eq!(unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "build", 0)) }(), 3.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    // "start" + 3x "iter" = 4 allocs, all freed (3 on reassign + 1 at scope exit)
    assert_eq!(allocs, 4, "4 strings allocated");
    assert_eq!(allocs, frees, "loop reassign: {} allocs, {} frees", allocs, frees);
});

mem_test!(mem_const_strings_freed, {
    // Const string locals — all should be freed at scope exit
    let mut m = jit(r#"
        pub fn const_test() -> Number {
            const greeting = "hello"
            const unused = "waste"
            return 42
        }
    "#);
    runtime::MEM.reset();
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "const_test", 0)) };
    assert_eq!(f(), 42.0);
    let (allocs, frees, _, _, _) = runtime::MEM.stats();
    assert_eq!(allocs, frees, "const strings freed: {} allocs, {} frees", allocs, frees);
});

// ─── Feature coverage: for loop ──────────────────

#[test]
fn for_loop_over_array() {
    let mut m = jit(r#"
        pub fn sum_array() -> Number {
            const arr = [10, 20, 30]
            let total = 0
            for item in arr {
                total = total + item
            }
            return total
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "sum_array", 0)) };
    assert_eq!(f(), 60.0);
}

// ─── Feature coverage: struct field mutation ─────

#[test]
fn struct_field_mutation() {
    let mut m = jit(r#"
        pub fn mutate_field() -> Number {
            const p = Point { x: 10, y: 20 }
            p.x = 99
            return p.x + p.y
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "mutate_field", 0)) };
    assert_eq!(f(), 119.0); // 99 + 20
}

