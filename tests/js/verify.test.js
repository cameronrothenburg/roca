// Verify tests — converted from tests/verify/*.rs
// These tests compile Roca source to JS via `roca build`, then execute the
// emitted JS with a user-supplied test script appended.
import { test, expect, describe, beforeAll } from "bun:test";
import { execSync } from "node:child_process";
import { writeFileSync, readFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "../..");
const OUT = join(__dirname, "compiled-verify");
const ROCA = process.env.ROCA_BIN || `cargo run --quiet --manifest-path ${ROOT}/Cargo.toml --`;
const RUNTIME = join(ROOT, "packages/runtime/index.js");

beforeAll(() => {
    if (existsSync(OUT)) rmSync(OUT, { recursive: true });
    mkdirSync(OUT, { recursive: true });
});

let testId = 0;

function run(rocaSource, testScript) {
    const name = `verify_${testId++}`;
    const srcFile = join(OUT, `${name}.roca`);
    const jsFile = join(OUT, `out/${name}.js`);
    writeFileSync(srcFile, rocaSource);

    // Emit JS only (skip native JIT — native correctness is tested via cargo test --lib)
    try {
        execSync(`${ROCA} build --emit-only ${srcFile}`, { cwd: ROOT, stdio: "pipe", timeout: 30000 });
    } catch (e) {
        const stderr = e.stderr?.toString() || "";
        const stdout = e.stdout?.toString() || "";
        throw new Error(`Emit failed for ${name}: ${stderr}${stdout}`);
    }

    if (!existsSync(jsFile)) {
        throw new Error(`No JS output for ${name}`);
    }

    let js = readFileSync(jsFile, "utf8");
    js = js.replace(/export /g, "");
    js = js.replace(
        'import roca from "@rocalang/runtime";',
        `import roca from "${RUNTIME}";`,
    );

    const full = js + "\n" + testScript;
    const runner = join(OUT, `${name}_run.js`);
    writeFileSync(runner, full);

    const result = execSync(`node --input-type=module < ${runner}`, {
        cwd: OUT,
        stdio: "pipe",
        timeout: 30000,
    });
    return result.toString().trim();
}

// ═══════════════════════════════════════════════════════════
// functions.rs
// ═══════════════════════════════════════════════════════════

describe("functions", () => {
    test("returns_number", () => {
        expect(run(
            `pub fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }`,
            `console.log(add(1, 2));`,
        )).toBe("3");
    });

    test("returns_string", () => {
        expect(run(
            `pub fn greet(name: String) -> String {
            return "Hello " + name
            test { self("cam") == "Hello cam" }
        }`,
            `console.log(greet("world"));`,
        )).toBe("Hello world");
    });

    test("returns_bool", () => {
        expect(run(
            `pub fn is_positive(n: Number) -> Bool {
            if n > 0 { return true }
            return false
            test { self(1) == true self(0) == false }
        }`,
            `console.log(is_positive(5)); console.log(is_positive(-3));`,
        )).toBe("true\nfalse");
    });

    test("no_params", () => {
        expect(run(
            `pub fn hello() -> String {
            return "hello"
            test { self() == "hello" }
        }`,
            `console.log(hello());`,
        )).toBe("hello");
    });

    test("multiple_functions", () => {
        expect(run(
            `pub fn double(x: Number) -> Number {
            return x * 2
            test { self(5) == 10 }
        }
        pub fn add_one(x: Number) -> Number {
            return x + 1
            test { self(5) == 6 }
        }`,
            `console.log(add_one(double(5)));`,
        )).toBe("11");
    });

    test("private_not_exported", () => {
        expect(run(
            `fn helper(x: Number) -> Number {
            return x + 1
            test { self(0) == 1 }
        }
        pub fn use_helper(x: Number) -> Number {
            return helper(x) + helper(x)
            test { self(5) == 12 }
        }`,
            `console.log(use_helper(5));`,
        )).toBe("12");
    });

    test("arithmetic_operators", () => {
        expect(run(
            `pub fn math(a: Number, b: Number) -> Number {
            return (a + b) * (a - b) / b
            test { self(10, 5) == 15 }
        }`,
            `console.log(math(10, 5));`,
        )).toBe("15");
    });

    test("string_concat_multiple", () => {
        expect(run(
            `pub fn full_name(first: String, last: String) -> String {
            return first + " " + last
            test { self("John", "Doe") == "John Doe" }
        }`,
            `console.log(full_name("John", "Doe"));`,
        )).toBe("John Doe");
    });
});

// ═══════════════════════════════════════════════════════════
// variables.rs
// ═══════════════════════════════════════════════════════════

describe("variables", () => {
    test("const_binding", () => {
        expect(run(
            `pub fn msg() -> String {
            const greeting = "hello"
            return greeting
            test { self() == "hello" }
        }`,
            `console.log(msg());`,
        )).toBe("hello");
    });

    test("let_binding", () => {
        expect(run(
            `pub fn count() -> Number {
            let x = 0
            x = x + 1
            x = x + 1
            x = x + 1
            return x
            test { self() == 3 }
        }`,
            `console.log(count());`,
        )).toBe("3");
    });

    test("multiple_bindings", () => {
        expect(run(
            `pub fn compute() -> Number {
            const a = 10
            const b = 20
            let result = a + b
            result = result * 2
            return result
            test { self() == 60 }
        }`,
            `console.log(compute());`,
        )).toBe("60");
    });

    test("let_with_string", () => {
        expect(run(
            `pub fn build() -> String {
            let msg = "hello"
            msg = msg + " world"
            return msg
            test { self() == "hello world" }
        }`,
            `console.log(build());`,
        )).toBe("hello world");
    });
});

// ═══════════════════════════════════════════════════════════
// control_flow.rs
// ═══════════════════════════════════════════════════════════

describe("control_flow", () => {
    test("if_true_branch", () => {
        expect(run(
            `pub fn check(x: Number) -> String {
            if x > 0 { return "positive" }
            return "not positive"
            test { self(5) == "positive" }
        }`,
            `console.log(check(5));`,
        )).toBe("positive");
    });

    test("if_false_branch", () => {
        expect(run(
            `pub fn check(x: Number) -> String {
            if x > 0 { return "positive" }
            return "not positive"
            test { self(-1) == "not positive" }
        }`,
            `console.log(check(-1));`,
        )).toBe("not positive");
    });

    test("if_else", () => {
        expect(run(
            `pub fn sign(x: Number) -> String {
            if x > 0 {
                return "positive"
            } else {
                return "non-positive"
            }
            test {
                self(5) == "positive"
                self(-1) == "non-positive"
            }
        }`,
            `console.log(sign(5)); console.log(sign(-1)); console.log(sign(0));`,
        )).toBe("positive\nnon-positive\nnon-positive");
    });

    test("nested_if", () => {
        expect(run(
            `pub fn classify(x: Number) -> String {
            if x > 0 {
                if x > 100 { return "big" }
                return "small"
            }
            return "negative"
            test {
                self(200) == "big"
                self(5) == "small"
                self(-1) == "negative"
            }
        }`,
            `console.log(classify(200)); console.log(classify(5)); console.log(classify(-1));`,
        )).toBe("big\nsmall\nnegative");
    });

    test("clamp", () => {
        expect(run(
            `pub fn clamp(val: Number, min: Number, max: Number) -> Number {
            if val < min { return min }
            if val > max { return max }
            return val
            test {
                self(5, 0, 10) == 5
                self(-5, 0, 10) == 0
                self(50, 0, 10) == 10
            }
        }`,
            `console.log(clamp(-5, 0, 10)); console.log(clamp(50, 0, 10)); console.log(clamp(5, 0, 10));`,
        )).toBe("0\n10\n5");
    });

    test("for_loop", () => {
        expect(run(
            `pub fn sum_to(n: Number) -> Number {
            let total = 0
            let i = 1
            if i <= n {
                total = total + i
                i = i + 1
                if i <= n {
                    total = total + i
                    i = i + 1
                    if i <= n {
                        total = total + i
                    }
                }
            }
            return total
            test { self(3) == 6 }
        }`,
            `console.log(sum_to(3));`,
        )).toBe("6");
    });

    test("boolean_and", () => {
        expect(run(
            `pub fn both(a: Bool, b: Bool) -> Bool {
            if a {
                if b { return true }
            }
            return false
            test { self(true, true) == true self(true, false) == false }
        }`,
            `console.log(both(true, true)); console.log(both(true, false)); console.log(both(false, true));`,
        )).toBe("true\nfalse\nfalse");
    });
});

// ═══════════════════════════════════════════════════════════
// arrays.rs
// ═══════════════════════════════════════════════════════════

describe("arrays", () => {
    test("array_literal", () => {
        expect(run(
            `pub fn nums() -> Number {
            const arr = [1, 2, 3]
            return arr[0]
            test { self() == 1 }
        }`,
            `console.log(nums());`,
        )).toBe("1");
    });

    test("array_index_access", () => {
        expect(run(
            `pub fn second(items: String) -> Number {
            const arr = [10, 20, 30]
            return arr[1]
            test { self("x") == 20 }
        }`,
            `console.log(second("x"));`,
        )).toBe("20");
    });

    test("array_length", () => {
        expect(run(
            `pub fn count() -> Number {
            const arr = [1, 2, 3, 4, 5]
            return arr.length
            test { self() == 5 }
        }`,
            `console.log(count());`,
        )).toBe("5");
    });

    test("empty_array", () => {
        expect(run(
            `pub fn empty() -> Number {
            const arr = []
            return arr.length
            test { self() == 0 }
        }`,
            `console.log(empty());`,
        )).toBe("0");
    });

    test("array_of_strings", () => {
        expect(run(
            `pub fn first_name() -> String {
            const names = ["alice", "bob", "cam"]
            return names[0]
            test { self() == "alice" }
        }`,
            `console.log(first_name());`,
        )).toBe("alice");
    });

    test("for_in_array", () => {
        expect(run(
            `pub fn sum() -> Number {
            const nums = [1, 2, 3]
            let total = 0
            for n in nums {
                total = total + n
            }
            return total
            test { self() == 6 }
        }`,
            `console.log(sum());`,
        )).toBe("6");
    });

    test("array_method_push", () => {
        expect(run(
            `pub fn build() -> Number {
            let arr = [1, 2]
            arr.push(3)
            return arr.length
            crash { arr.push -> skip }
            test { self() == 3 }
        }`,
            `console.log(build());`,
        )).toBe("3");
    });

    test("nested_array_access", () => {
        expect(run(
            `pub fn get() -> Number {
            const matrix = [[1, 2], [3, 4]]
            return matrix[1][0]
            test { self() == 3 }
        }`,
            `console.log(get());`,
        )).toBe("3");
    });
});

// ═══════════════════════════════════════════════════════════
// closures.rs
// ═══════════════════════════════════════════════════════════

describe("closures", () => {
    test("map_with_closure", () => {
        expect(run(
            `pub fn double_all() -> String {
            const nums = [1, 2, 3]
            const doubled = nums.map(fn(x) -> x * 2)
            return doubled.join(",")
            crash {
                nums.map -> skip
                doubled.join -> skip
            }
            test { self() == "2,4,6" }
        }`,
            `console.log(double_all());`,
        )).toBe("2,4,6");
    });

    test("filter_with_closure", () => {
        expect(run(
            `pub fn only_big() -> String {
            const nums = [1, 5, 10, 15, 3]
            const big = nums.filter(fn(x) -> x > 5)
            return big.join(",")
            crash {
                nums.filter -> skip
                big.join -> skip
            }
            test { self() == "10,15" }
        }`,
            `console.log(only_big());`,
        )).toBe("10,15");
    });

    test("closure_string_transform", () => {
        expect(run(
            `pub fn shout() -> String {
            const words = ["hello", "world"]
            const upper = words.map(fn(w) -> w.toUpperCase())
            return upper.join(" ")
            crash {
                words.map -> skip
                upper.join -> skip
            }
            test { self() == "HELLO WORLD" }
        }`,
            `console.log(shout());`,
        )).toBe("HELLO WORLD");
    });

    test("closure_no_params", () => {
        expect(run(
            `pub fn make_array() -> String {
            const arr = [1, 2, 3]
            const result = arr.map(fn(x) -> "item")
            return result.join(",")
            crash {
                arr.map -> skip
                result.join -> skip
            }
            test { self() == "item,item,item" }
        }`,
            `console.log(make_array());`,
        )).toBe("item,item,item");
    });

    test("closure_multi_param", () => {
        expect(run(
            `pub fn with_index() -> String {
            const arr = ["a", "b", "c"]
            const result = arr.map(fn(item, i) -> String(i) + ":" + item)
            return result.join(",")
            crash {
                arr.map -> skip
                result.join -> skip
                String -> skip
            }
            test { self() == "0:a,1:b,2:c" }
        }`,
            `console.log(with_index());`,
        )).toBe("0:a,1:b,2:c");
    });

    test("closure_in_variable", () => {
        expect(run(
            `pub fn apply() -> Number {
            const double = fn(x) -> x * 2
            return double(5)
            test { self() == 10 }
        }`,
            `console.log(apply());`,
        )).toBe("10");
    });

    test("chained_map_filter", () => {
        expect(run(
            `pub fn process() -> String {
            const nums = [1, 2, 3, 4, 5, 6]
            const result = nums.filter(fn(x) -> x > 2).map(fn(x) -> x * 10)
            return result.join(",")
            crash {
                nums.filter -> skip
                result.join -> skip
            }
            test { self() == "30,40,50,60" }
        }`,
            `console.log(process());`,
        )).toBe("30,40,50,60");
    });
});

// ═══════════════════════════════════════════════════════════
// comparisons.rs
// ═══════════════════════════════════════════════════════════

describe("comparisons", () => {
    test("string_equality", () => {
        expect(run(
            `pub fn check(a: String, b: String) -> Bool {
            return a == b
            test { self("hello", "hello") == true self("a", "b") == false }
        }`,
            `console.log(check("same", "same")); console.log(check("a", "b"));`,
        )).toBe("true\nfalse");
    });

    test("string_inequality", () => {
        expect(run(
            `pub fn check(a: String, b: String) -> Bool {
            return a != b
            test { self("a", "b") == true self("a", "a") == false }
        }`,
            `console.log(check("x", "y")); console.log(check("x", "x"));`,
        )).toBe("true\nfalse");
    });

    test("string_ordering", () => {
        expect(run(
            `pub fn comes_first(a: String, b: String) -> Bool {
            return a < b
            test { self("a", "b") == true self("z", "a") == false }
        }`,
            `console.log(comes_first("apple", "banana")); console.log(comes_first("z", "a"));`,
        )).toBe("true\nfalse");
    });

    test("number_equality", () => {
        expect(run(
            `pub fn eq(a: Number, b: Number) -> Bool {
            return a == b
            test { self(1, 1) == true self(1, 2) == false }
        }`,
            `console.log(eq(42, 42)); console.log(eq(1, 2));`,
        )).toBe("true\nfalse");
    });

    test("number_ordering", () => {
        expect(run(
            `pub fn greater(a: Number, b: Number) -> Bool {
            return a > b
            test { self(10, 5) == true self(1, 10) == false }
        }`,
            `console.log(greater(10, 5)); console.log(greater(1, 100));`,
        )).toBe("true\nfalse");
    });

    test("number_lte_gte", () => {
        expect(run(
            `pub fn in_range(val: Number, min: Number, max: Number) -> Bool {
            if val >= min {
                if val <= max { return true }
            }
            return false
            test { self(5, 0, 10) == true self(15, 0, 10) == false }
        }`,
            `console.log(in_range(5, 0, 10)); console.log(in_range(-1, 0, 10)); console.log(in_range(10, 0, 10));`,
        )).toBe("true\nfalse\ntrue");
    });

    test("bool_equality", () => {
        expect(run(
            `pub fn same(a: Bool, b: Bool) -> Bool {
            return a == b
            test { self(true, true) == true self(true, false) == false }
        }`,
            `console.log(same(true, true)); console.log(same(true, false));`,
        )).toBe("true\nfalse");
    });

    // SKIPPED: cross_type_comparison_caught — checker test (uses roca::check::check)
    // SKIPPED: struct_comparison_caught — checker test
    // SKIPPED: bool_ordering_caught — checker test
    // SKIPPED: field_comparison_works — checker test
    // SKIPPED: string_number_mismatch_in_if — checker test
    // SKIPPED: inferred_type_comparison — checker test
    // SKIPPED: same_type_comparison_passes — checker test

    test("string_gte_works", () => {
        expect(run(
            `pub fn gte(a: String, b: String) -> Bool {
            return a >= b
            test { self("b", "a") == true self("a", "b") == false }
        }`,
            `console.log(gte("b", "a")); console.log(gte("a", "z"));`,
        )).toBe("true\nfalse");
    });
});

// ═══════════════════════════════════════════════════════════
// constraints.rs — ALL checker/parser tests, SKIPPED
// ═══════════════════════════════════════════════════════════

// SKIPPED: string_constraints_parse — parser test
// SKIPPED: number_constraints_parse — parser test
// SKIPPED: no_constraints_is_empty — parser test
// SKIPPED: contract_fields_with_constraints — parser test
// SKIPPED: min_greater_than_max_caught — checker test
// SKIPPED: contains_on_number_caught — checker test
// SKIPPED: valid_constraints_pass — checker test
// SKIPPED: constraints_with_nullable — parser test
// SKIPPED: constraints_with_pattern — parser test

// ═══════════════════════════════════════════════════════════
// contracts.rs
// ═══════════════════════════════════════════════════════════

describe("contracts", () => {
    test("contract_errors_object", () => {
        expect(run(
            `contract HttpClient {
            get(url: String) -> String, err {
                err timeout = "request timed out"
                err not_found = "404 not found"
            }
        }`,
            `
            console.log(HttpClientErrors.timeout);
            console.log(HttpClientErrors.not_found);
        `,
        )).toBe("request timed out\n404 not found");
    });

    test("contract_multiple_methods_errors", () => {
        expect(run(
            `contract Database {
            save(data: String) -> String, err {
                err connection = "connection lost"
                err duplicate = "duplicate key"
            }
            find(id: String) -> String, err {
                err not_found = "not found"
            }
        }`,
            `
            console.log(DatabaseErrors.connection);
            console.log(DatabaseErrors.duplicate);
            console.log(DatabaseErrors.not_found);
        `,
        )).toBe("connection lost\nduplicate key\nnot found");
    });

    test("enum_contract_values", () => {
        expect(run(
            `contract StatusCode { 200 201 400 404 500 }`,
            `
            console.log(StatusCode["200"]);
            console.log(StatusCode["404"]);
            console.log(StatusCode["500"]);
        `,
        )).toBe("200\n404\n500");
    });

    test("contract_no_errors_no_object", () => {
        expect(run(
            `contract Stringable { to_string() -> String }`,
            `
            console.log(typeof StringableErrors);
        `,
        )).toBe("undefined");
    });
});

// ═══════════════════════════════════════════════════════════
// crash.rs
// ═══════════════════════════════════════════════════════════

describe("crash", () => {
    test("halt_lets_error_propagate", () => {
        expect(run(
            `/// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Calls risky
        pub fn caller() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> fallback(fn(e) -> "error: " + e.message) }
            test { self() == "error: boom" }
        }`,
            `
            console.log(caller());
        `,
        )).toBe("error: boom");
    });

    test("skip_ignores_error", () => {
        expect(run(
            `/// Maybe fail operation
        pub struct MaybeFail {
            call(x: Number) -> Number, err {
                err zero = "zero"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.zero }
                return x
                test { self(1) == 1 self(0) is err.zero }
            }
        }

        /// Calls maybe fail safely
        pub fn safe_call() -> String {
            const result = MaybeFail.call(0)
            return "continued"
            crash { MaybeFail.call -> skip }
            test { self() == "continued" }
        }`,
            `
            console.log(safe_call());
        `,
        )).toBe("continued");
    });

    test("fallback_provides_default", () => {
        expect(run(
            `/// Gets a value
        pub struct GetValue {
            call(x: Number) -> Number, err {
                err not_found = "not_found"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.not_found }
                return x
                test { self(1) == 1 self(0) is err.not_found }
            }
        }

        /// Gets with default
        pub fn with_default() -> Number {
            const result = GetValue.call(0)
            return result
            crash { GetValue.call -> fallback(99) }
            test { self() == 99 }
        }`,
            `
            console.log(with_default());
        `,
        )).toBe("99");
    });

    test("retry_on_success", () => {
        expect(run(
            `/// Flaky operation
        pub struct Flaky {
            call(s: String) -> String, err {
                err fail = "fail"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.fail }
                return "ok"
                test { self("ok") == "ok" self("") is err.fail }
            }
        }

        /// Calls flaky
        pub fn caller() -> String {
            const result = Flaky.call("ok")
            return result
            crash { Flaky.call -> retry(3, 0) |> fallback("failed") }
            test { self() == "ok" }
        }`,
            `console.log(caller());`,
        )).toBe("ok");
    });

    test("retry_exhausts_attempts_then_throws", () => {
        expect(run(
            `/// Always fails
        pub struct AlwaysFail {
            call(s: String) -> String, err {
                err broken = "broken"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.broken }
                return s
                test { self("ok") == "ok" self("") is err.broken }
            }
        }

        /// Calls always fail
        pub fn caller() -> String, err {
            if false { return err.broken }
            const result = AlwaysFail.call("")
            return result
            crash { AlwaysFail.call -> retry(3, 0) |> halt }
            test { self() is Ok self() is err.broken }
        }`,
            `
            const result = caller();
            console.log(result.err ? result.err.name : "no error");
        `,
        )).toBe("broken");
    });

    test("retry_succeeds_on_later_attempt", () => {
        expect(run(
            `/// Gets count
        pub fn get_count() -> Number {
            return 0
            test { self() == 0 }
        }`,
            `
            // Simulate flaky with global state
            let attempts = 0;
            function flaky() {
                attempts++;
                if (attempts < 3) return { value: null, err: new Error("not yet") };
                return { value: "success", err: null };
            }

            let result;
            let _err;
            for (let _attempt = 0; _attempt < 5; _attempt++) {
                const _retry_tmp = flaky();
                _err = _retry_tmp.err;
                if (!_err) { result = _retry_tmp.value; break; }
                if (_attempt === 4) throw _err;
            }
            console.log(result);
            console.log(attempts);
        `,
        )).toBe("success\n3");
    });

    test("halt_success_passes_through", () => {
        expect(run(
            `/// Safe operation
        pub struct Safe {
            call(x: Number) -> Number, err {
                err fail = "fail"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.fail }
                return 42
                test { self(1) == 42 self(0) is err.fail }
            }
        }

        /// Calls safe
        pub fn caller() -> Number {
            const result = Safe.call(1)
            return result
            crash { Safe.call -> fallback(0) }
            test { self() == 42 }
        }`,
            `
            console.log(caller());
        `,
        )).toBe("42");
    });

    test("multiple_crash_handlers", () => {
        expect(run(
            `/// Step one
        pub struct StepOne {
            call(x: Number) -> Number, err {
                err fail = "fail"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.fail }
                return 10
                test { self(1) == 10 self(0) is err.fail }
            }
        }

        /// Step two
        pub struct StepTwo {
            call(x: Number) -> Number, err {
                err fail = "fail"
            }
        }{
            pub fn call(x: Number) -> Number, err {
                if x == 0 { return err.fail }
                return 20
                test { self(1) == 20 self(0) is err.fail }
            }
        }

        /// Pipeline
        pub fn pipeline() -> Number {
            const a = StepOne.call(1)
            const b = StepTwo.call(1)
            return a + b
            crash {
                StepOne.call -> fallback(0)
                StepTwo.call -> fallback(0)
            }
            test { self() == 30 }
        }`,
            `console.log(pipeline());`,
        )).toBe("30");
    });

    test("detailed_crash_per_error", () => {
        expect(run(
            `/// Fetches data
        pub struct Fetch {
            call(url: String) -> String, err {
                err invalid = "invalid"
                err timeout = "timeout"
            }
        }{
            pub fn call(url: String) -> String, err {
                if url == "" { return err.invalid }
                if url == "timeout" { return err.timeout }
                return "data"
                test {
                    self("ok") == "data"
                    self("") is err.invalid
                    self("timeout") is err.timeout
                }
            }
        }

        /// Loads data
        pub fn load() -> String {
            const result = Fetch.call("timeout")
            return result
            crash {
                Fetch.call {
                    err.timeout -> fallback("cached")
                    err.invalid -> fallback("none")
                    default -> halt
                }
            }
            test { self() == "cached" }
        }`,
            `console.log(load());`,
        )).toBe("cached");
    });

    test("detailed_crash_default_halt", () => {
        expect(run(
            `/// Fetches data
        pub struct Fetch {
            call(url: String) -> String, err {
                err unknown = "unknown"
            }
        }{
            pub fn call(url: String) -> String, err {
                if url == "bad" { return err.unknown }
                return "ok"
                test { self("ok") == "ok" self("bad") is err.unknown }
            }
        }

        /// Loads data
        pub fn load() -> String, err {
            const result = Fetch.call("bad")
            return result
            crash {
                Fetch.call {
                    err.timeout -> fallback("cached")
                    default -> halt
                }
            }
            test { self() is err.unknown }
        }`,
            `
            const { value, err } = load();
            if (err) {
                console.log("caught");
                console.log(err.message);
            } else {
                console.log("should not reach");
            }
        `,
        )).toBe("caught\nunknown");
    });

    test("halt_propagates_tuple_error", () => {
        expect(run(
            `/// Validates input
        pub struct Validate {
            call(s: String) -> String, err {
                err empty = "value cannot be empty"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.empty("value cannot be empty") }
                return s
                test { self("ok") == "ok" self("") is err.empty }
            }
        }

        /// Processes input
        pub fn process(s: String) -> String, err {
            const result = Validate.call(s)
            return result
            crash { Validate.call -> halt }
            test { self("ok") == "ok" self("") is err.empty }
        }`,
            `
            const { value: val1, err: err1 } = process("hello");
            console.log(val1);
            console.log(err1);

            const { value: val2, err: err2 } = process("");
            console.log(val2);
            console.log(err2.name);
            console.log(err2.message);
        `,
        )).toBe("hello\nnull\nnull\nempty\nvalue cannot be empty");
    });

    test("fallback_on_tuple_uses_default", () => {
        expect(run(
            `/// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Safe wrapper
        pub fn safe() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> fallback("default") }
            test { self() == "default" }
        }`,
            `
            console.log(safe());
        `,
        )).toBe("default");
    });

    test("skip_on_tuple_continues", () => {
        expect(run(
            `/// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Ignores errors
        pub fn ignorer() -> String {
            const result = Risky.call("")
            return "continued"
            crash { Risky.call -> skip }
            test { self() == "continued" }
        }`,
            `
            console.log(ignorer());
        `,
        )).toBe("continued");
    });

    test("chain_log_halt", () => {
        expect(run(
            `/// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Calls risky
        pub fn caller() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> log |> fallback("got error") }
            test { self() == "got error" }
        }`,
            `console.log(caller());`,
        )).toBe("got error");
    });

    test("chain_log_skip", () => {
        expect(run(
            `/// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Ignores errors
        pub fn ignorer() -> String {
            const result = Risky.call("")
            return "continued"
            crash { Risky.call -> log |> skip }
            test { self() == "continued" }
        }`,
            `console.log(ignorer());`,
        )).toBe("continued");
    });

    test("chain_log_fallback", () => {
        expect(run(
            `/// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "boom"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Safe wrapper
        pub fn safe() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> log |> fallback("safe_default") }
            test { self() == "safe_default" }
        }`,
            `console.log(safe());`,
        )).toBe("safe_default");
    });

    // SKIPPED: panic_emits_process_exit — emitter test (uses roca::emit::emit)

    test("fallback_closure_receives_error", () => {
        expect(run(
            `/// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err boom = "something broke"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.boom("something broke") }
                return s
                test { self("ok") == "ok" self("") is err.boom }
            }
        }

        /// Handles errors
        pub fn handler() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> fallback(fn(e) -> "error: " + e.message) }
            test { self() == "error: something broke" }
        }`,
            `console.log(handler());`,
        )).toBe("error: something broke");
    });

    test("fallback_closure_error_name", () => {
        expect(run(
            `/// Risky operation
        pub struct Risky {
            call(s: String) -> String, err {
                err timeout = "took too long"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.timeout("took too long") }
                return s
                test { self("ok") == "ok" self("") is err.timeout }
            }
        }

        /// Handles errors
        pub fn handler() -> String {
            const result = Risky.call("")
            return result
            crash { Risky.call -> fallback(fn(e) -> e.name + ": " + e.message) }
            test { self() == "timeout: took too long" }
        }`,
            `console.log(handler());`,
        )).toBe("timeout: took too long");
    });

    test("halt_propagates_error_tuple", () => {
        expect(run(
            `/// Validates input
        pub struct Validate {
            call(s: String) -> String, err {
                err empty = "cannot be empty"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.empty("cannot be empty") }
                return s
                test { self("ok") == "ok" self("") is err.empty }
            }
        }

        /// Processes input
        pub fn process(s: String) -> String, err {
            const result = Validate.call(s)
            return result
            crash { Validate.call -> halt }
            test { self("ok") == "ok" self("") is err.empty }
        }`,
            `
            const { value: v1, err: e1 } = process("hello");
            console.log(v1);
            const { value: v2, err: e2 } = process("");
            console.log(e2.name);
            console.log(e2.message);
        `,
        )).toBe("hello\nempty\ncannot be empty");
    });
});

// ═══════════════════════════════════════════════════════════
// enums.rs
// ═══════════════════════════════════════════════════════════

describe("enums", () => {
    // SKIPPED: string_enum_parses — parser test
    // SKIPPED: number_enum_parses — parser test

    test("enum_emits_js_object", () => {
        expect(run(
            `pub enum Status {
            active = "active"
            inactive = "inactive"
            suspended = "suspended"
        }

        pub fn check(s: String) -> String {
            if s == Status.active { return "is active" }
            return "not active"
            test { self("active") == "is active" }
        }`,
            `
            console.log(Status.active);
            console.log(Status.suspended);
            console.log(check("active"));
            console.log(check("other"));
        `,
        )).toBe("active\nsuspended\nis active\nnot active");
    });

    test("number_enum_emits_js", () => {
        expect(run(
            `pub enum HttpCode {
            ok = 200
            not_found = 404
            server_error = 500
        }

        pub fn is_ok(code: Number) -> Bool {
            return code == HttpCode.ok
            test { self(200) == true self(404) == false }
        }`,
            `
            console.log(HttpCode.ok);
            console.log(HttpCode.not_found);
            console.log(is_ok(200));
            console.log(is_ok(404));
        `,
        )).toBe("200\n404\ntrue\nfalse");
    });

    test("enum_in_match", () => {
        expect(run(
            `pub enum Color {
            red = "red"
            green = "green"
            blue = "blue"
        }

        pub fn describe(c: String) -> String {
            return match c {
                Color.red => "warm"
                Color.blue => "cool"
                _ => "other"
            }
            test { self("red") == "warm" self("blue") == "cool" self("green") == "other" }
        }`,
            `
            console.log(describe("red"));
            console.log(describe("blue"));
            console.log(describe("green"));
        `,
        )).toBe("warm\ncool\nother");
    });

    // SKIPPED: pub_enum_exported — emitter test
    // SKIPPED: private_enum_not_exported — emitter test

    test("enum_in_struct_field", () => {
        expect(run(
            `pub enum Role {
            admin = "admin"
            user = "user"
        }

        pub struct Account {
            name: String
            role: String
        }{}

        pub fn is_admin(a: Account) -> Bool {
            return a.role == Role.admin
            test { self(Account { name: "cam", role: "admin" }) == true }
        }`,
            `
            const a = new Account({ name: "cam", role: "admin" });
            console.log(is_admin(a));
            const b = new Account({ name: "cam", role: "user" });
            console.log(is_admin(b));
        `,
        )).toBe("true\nfalse");
    });
});

// ═══════════════════════════════════════════════════════════
// errors.rs
// ═══════════════════════════════════════════════════════════

describe("errors", () => {
    test("err_variable_message_access", () => {
        expect(run(
            `/// Checks input
        pub struct Check {
            call(s: String) -> String, err {
                err empty = "empty"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.empty }
                return s
                test { self("ok") == "ok" self("") is err.empty }
            }
        }
        /// Calls check
        pub fn caller(s: String) -> String {
            const result = Check.call(s)
            return result
            crash { Check.call -> fallback(fn(e) -> "error: " + e.message) }
            test { self("ok") == "ok" }
        }`,
            `console.log(caller("ok")); console.log(caller(""));`,
        )).toBe("ok\nerror: empty");
    });

    test("success_returns_value_and_null", () => {
        expect(run(
            `/// Divides numbers
        pub struct Divide {
            call(a: Number, b: Number) -> Number, err {
                err division_by_zero = "division_by_zero"
            }
        }{
            pub fn call(a: Number, b: Number) -> Number, err {
                if b == 0 { return err.division_by_zero }
                return a / b
                test { self(10, 2) == 5 self(10, 0) is err.division_by_zero }
            }
        }`,
            `
            const { value: val, err } = Divide.call(10, 2);
            console.log(val);
            console.log(err);
        `,
        )).toBe("5\nnull");
    });

    test("error_returns_zero_value_and_error", () => {
        expect(run(
            `/// Divides numbers
        pub struct Divide {
            call(a: Number, b: Number) -> Number, err {
                err division_by_zero = "division_by_zero"
            }
        }{
            pub fn call(a: Number, b: Number) -> Number, err {
                if b == 0 { return err.division_by_zero }
                return a / b
                test { self(10, 2) == 5 self(10, 0) is err.division_by_zero }
            }
        }`,
            `
            const { value: val, err } = Divide.call(10, 0);
            console.log(val);
            console.log(typeof val);
            console.log(err.name);
            console.log(err.message);
        `,
        )).toBe("0\nnumber\ndivision_by_zero\ndivision_by_zero");
    });

    test("multiple_error_paths", () => {
        expect(run(
            `/// Parses age
        pub struct ParseAge {
            call(s: String) -> Number, err {
                err empty = "empty"
                err invalid = "invalid"
            }
        }{
            pub fn call(s: String) -> Number, err {
                if s == "" { return err.empty }
                if s == "bad" { return err.invalid }
                return 25
                test {
                    self("ok") == 25
                    self("") is err.empty
                    self("bad") is err.invalid
                }
            }
        }`,
            `
            const { value: v1, err: e1 } = ParseAge.call("ok");
            console.log(v1);
            const { value: v2, err: e2 } = ParseAge.call("");
            console.log(e2.message);
            const { value: v3, err: e3 } = ParseAge.call("bad");
            console.log(e3.message);
        `,
        )).toBe("25\nempty\ninvalid");
    });

    test("err_tuple_destructure_in_caller", () => {
        expect(run(
            `/// Validates input
        pub struct Validate {
            call(s: String) -> String, err {
                err empty = "empty"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.empty }
                return s
                test { self("a") == "a" self("") is err.empty }
            }
        }
        /// Processes input
        pub fn process(s: String) -> String {
            const result = Validate.call(s)
            return "ok"
            crash { Validate.call -> fallback("") }
            test { self("a") == "ok" }
        }`,
            `
            const { value: val, err } = Validate.call("hello");
            console.log(val);
            console.log(err);
        `,
        )).toBe("hello\nnull");
    });

    test("non_err_function_returns_plain_value", () => {
        expect(run(
            `/// Adds two numbers
        pub fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }`,
            `
            const result = add(1, 2);
            console.log(result);
            console.log(typeof result);
        `,
        )).toBe("3\nnumber");
    });
});

// ═══════════════════════════════════════════════════════════
// field_assign.rs
// ═══════════════════════════════════════════════════════════

describe("field_assign", () => {
    test("self_field_assign", () => {
        expect(run(
            `pub struct Counter {
            count: Number
            increment() -> Counter
        }{
            fn increment() -> Counter {
                self.count = self.count + 1
                return self
                test {}
            }
        }

        pub fn test_counter() -> Number {
            let c = Counter { count: 0 }
            const c2 = c.increment()
            return c2.count
            crash { c.increment -> skip }
            test { self() == 1 }
        }`,
            `
            console.log(test_counter());
        `,
        )).toBe("1");
    });
});

// ═══════════════════════════════════════════════════════════
// generics.rs
// ═══════════════════════════════════════════════════════════

describe("generics", () => {
    // SKIPPED: generic_array_type_parses — parser test
    // SKIPPED: generic_map_type_parses — parser test
    // SKIPPED: generic_param_type_parses — parser test
    // SKIPPED: generic_return_type_parses — parser test

    test("generic_array_js_execution", () => {
        expect(run(
            `pub fn first(items: Array<String>) -> String {
            return items[0]
            test { self(["hello"]) == "hello" }
        }`,
            `console.log(first(["a", "b", "c"]));`,
        )).toBe("a");
    });

    // SKIPPED: nullable_generic — parser test
});

// ═══════════════════════════════════════════════════════════
// match_expr.rs
// ═══════════════════════════════════════════════════════════

describe("match_expr", () => {
    test("match_number", () => {
        expect(run(
            `pub fn describe(code: Number) -> String {
            return match code {
                200 => "ok"
                404 => "not found"
                500 => "error"
                _ => "unknown"
            }
            test {
                self(200) == "ok"
                self(404) == "not found"
                self(999) == "unknown"
            }
        }`,
            `
            console.log(describe(200));
            console.log(describe(404));
            console.log(describe(500));
            console.log(describe(999));
        `,
        )).toBe("ok\nnot found\nerror\nunknown");
    });

    test("match_string", () => {
        expect(run(
            `pub fn greet(lang: String) -> String {
            return match lang {
                "en" => "hello"
                "es" => "hola"
                "de" => "hallo"
                _ => "hi"
            }
            test {
                self("en") == "hello"
                self("fr") == "hi"
            }
        }`,
            `
            console.log(greet("en"));
            console.log(greet("es"));
            console.log(greet("fr"));
        `,
        )).toBe("hello\nhola\nhi");
    });

    test("match_in_variable", () => {
        expect(run(
            `pub fn label(x: Number) -> String {
            const msg = match x {
                1 => "one"
                2 => "two"
                _ => "other"
            }
            return msg
            test {
                self(1) == "one"
                self(2) == "two"
                self(99) == "other"
            }
        }`,
            `console.log(label(1)); console.log(label(2)); console.log(label(99));`,
        )).toBe("one\ntwo\nother");
    });

    test("match_no_default", () => {
        expect(run(
            `pub fn check(x: Number) -> String {
            return match x {
                0 => "zero"
                1 => "one"
            }
            test { self(0) == "zero" }
        }`,
            `console.log(check(0)); console.log(check(1));`,
        )).toBe("zero\none");
    });

    test("match_bool", () => {
        expect(run(
            `pub fn yesno(b: Bool) -> String {
            return match b {
                true => "yes"
                false => "no"
            }
            test {
                self(true) == "yes"
                self(false) == "no"
            }
        }`,
            `console.log(yesno(true)); console.log(yesno(false));`,
        )).toBe("yes\nno");
    });
});

// ═══════════════════════════════════════════════════════════
// match_err.rs
// ═══════════════════════════════════════════════════════════

describe("match_err", () => {
    test("match_returns_error", () => {
        expect(run(
            `pub fn categorize(code: Number) -> String, err {
            return match code {
                200 => "ok"
                404 => err.not_found
                500 => err.server_error
                _ => err.unknown
            }
            test {
                self(200) == "ok"
                self(404) is err.not_found
                self(500) is err.server_error
                self(999) is err.unknown
            }
        }`,
            `
            const { value: r1, err: e1 } = categorize(200);
            console.log(r1);
            const { value: r2, err: e2 } = categorize(404);
            console.log(e2.message);
            const { value: r3, err: e3 } = categorize(500);
            console.log(e3.message);
        `,
        )).toBe("ok\nnot_found\nserver_error");
    });
});

// ═══════════════════════════════════════════════════════════
// nullable.rs
// ═══════════════════════════════════════════════════════════

describe("nullable", () => {
    // SKIPPED: nullable_field_parses — parser test
    // SKIPPED: method_on_nullable_rejected — checker test
    // SKIPPED: method_on_non_nullable_passes — checker test

    test("nullable_field_can_be_null", () => {
        expect(run(
            `pub struct Profile {
            name: String
            bio: Optional<String>
        }{}

        pub fn has_bio(p: Profile) -> Bool {
            if p.bio == null { return false }
            return true
            test { self(Profile { name: "cam", bio: null }) == false }
        }`,
            `
            const p1 = new Profile({ name: "cam", bio: null });
            console.log(has_bio(p1));
            const p2 = new Profile({ name: "cam", bio: "hello" });
            console.log(has_bio(p2));
        `,
        )).toBe("false\ntrue");
    });

    test("nullable_field_with_value", () => {
        expect(run(
            `pub struct Config {
            name: String
            description: Optional<String>
        }{}

        pub fn display(c: Config) -> String {
            if c.description == null { return c.name }
            return c.name + ": " + c.description
            test { self(Config { name: "app", description: null }) == "app" }
        }`,
            `
            const c1 = new Config({ name: "app", description: null });
            console.log(display(c1));
            const c2 = new Config({ name: "app", description: "my app" });
            console.log(display(c2));
        `,
        )).toBe("app\napp: my app");
    });

    test("function_returns_nullable", () => {
        expect(run(
            `/// Finds an item by id
        pub struct Find {
            call(id: String) -> String, err {
                err not_found = "not_found"
            }
        }{
            pub fn call(id: String) -> String, err {
                if id == "" { return err.not_found }
                return "found: " + id
                test {
                    self("1") == "found: 1"
                    self("") is err.not_found
                }
            }
        }`,
            `
            const { value: v1 } = Find.call("1");
            console.log(v1);
            const { value: v2, err } = Find.call("");
            console.log(err ? "not_found" : v2);
        `,
        )).toBe("found: 1\nnot_found");
    });

    // SKIPPED: nullable_error_mentions_null_check — checker test
});

// ═══════════════════════════════════════════════════════════
// structs.rs
// ═══════════════════════════════════════════════════════════

describe("structs", () => {
    test("constructor_sets_fields", () => {
        expect(run(
            `pub struct Point {
            x: Number
            y: Number
        }{}`,
            `
            const p = new Point({ x: 3, y: 4 });
            console.log(p.x);
            console.log(p.y);
        `,
        )).toBe("3\n4");
    });

    test("constructor_single_field", () => {
        expect(run(
            `pub struct Name {
            value: String
        }{}`,
            `
            const n = new Name({ value: "cam" });
            console.log(n.value);
        `,
        )).toBe("cam");
    });

    test("static_method", () => {
        expect(run(
            `pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err missing = "required"
                err invalid = "invalid format"
            }
        }{
            fn validate(raw: String) -> Email, err {
                if raw == "" { return err.missing }
                if raw == "x" { return err.invalid }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.missing
                    self("x") is err.invalid
                }
            }
        }`,
            `
            const { value: email, err } = Email.validate("cam@test.com");
            console.log(email.value);
            console.log(err);
        `,
        )).toBe("cam@test.com\nnull");
    });

    test("static_method_returns_error", () => {
        expect(run(
            `pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err missing = "required"
            }
        }{
            fn validate(raw: String) -> Email, err {
                if raw == "" { return err.missing }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.missing
                }
            }
        }`,
            `
            const { value: email, err } = Email.validate("");
            console.log(email === null);
            console.log(err.name);
            console.log(err.message);
        `,
        )).toBe("true\nmissing\nrequired");
    });

    test("multiple_fields_and_validate", () => {
        expect(run(
            `pub struct User {
            name: String
            age: Number
            validate(name: String, age: Number) -> User, err {
                err missing_name = "name required"
                err invalid_age = "age must be positive"
            }
        }{
            fn validate(name: String, age: Number) -> User, err {
                if name == "" { return err.missing_name }
                if age < 0 { return err.invalid_age }
                return User { name: name, age: age }
                test {
                    self("cam", 25) is Ok
                    self("", 25) is err.missing_name
                    self("cam", -1) is err.invalid_age
                }
            }
        }`,
            `
            const { value: u, err: _e0 } = User.validate("cam", 25);
            console.log(u.name);
            console.log(u.age);
            const { value: _v1, err: e1 } = User.validate("", 25);
            console.log(e1.name);
            console.log(e1.message);
            const { value: _v2, err: e2 } = User.validate("cam", -1);
            console.log(e2.name);
            console.log(e2.message);
        `,
        )).toBe("cam\n25\nmissing_name\nname required\ninvalid_age\nage must be positive");
    });

    test("empty_struct_no_fields", () => {
        expect(run(
            `pub struct Config {
            get_default() -> Number
        }{
            fn get_default() -> Number {
                return 42
                test { self() == 42 }
            }
        }`,
            `console.log(Config.get_default());`,
        )).toBe("42");
    });
});

// ═══════════════════════════════════════════════════════════
// emit_edge_cases.rs
// ═══════════════════════════════════════════════════════════

describe("emit_edge_cases", () => {
    test("nested_match_err_in_if", () => {
        expect(run(
            `pub fn check(flag: Bool, code: Number) -> String, err {
            if flag {
                return match code {
                    200 => "ok"
                    _ => err.bad
                }
            }
            return "skipped"
            test {
                self(true, 200) == "ok"
                self(true, 500) is err.bad
                self(false, 200) == "skipped"
            }
        }`,
            `
            const { value: r1, err: e1 } = check(true, 200);
            console.log(r1);
            const { value: r2, err: e2 } = check(true, 500);
            console.log(e2.message);
            const { value: r3, err: e3 } = check(false, 200);
            console.log(r3);
        `,
        )).toBe("ok\nbad\nskipped");
    });

    test("self_field_in_satisfies", () => {
        expect(run(
            `contract Settable { set_name(n: String) -> String }

        pub struct User {
            name: String
        }{}

        User satisfies Settable {
            fn set_name(n: String) -> String {
                self.name = n
                return self.name
                test { self("cam") == "cam" }
            }
        }`,
            `
            const u = new User({ name: "anon" });
            console.log(u.set_name("cam"));
            console.log(u.name);
        `,
        )).toBe("cam\ncam");
    });

    // SKIPPED: multiple_sequential_waits — emitter test

    test("closure_captures_outer", () => {
        expect(run(
            `pub fn prefix_all() -> String {
            const prefix = "hi-"
            const items = ["a", "b", "c"]
            const result = items.map(fn(x) -> prefix + x)
            return result.join(",")
            crash {
                items.map -> skip
                result.join -> skip
            }
            test { self() == "hi-a,hi-b,hi-c" }
        }`,
            `console.log(prefix_all());`,
        )).toBe("hi-a,hi-b,hi-c");
    });

    test("string_interp_with_method", () => {
        expect(run(
            `pub fn clean(name: String) -> String {
            return "name: {name.trim()}"
            crash { name.trim -> skip }
            test { self("  cam  ") == "name: cam" }
        }`,
            `console.log(clean("  hello  "));`,
        )).toBe("name: hello");
    });

    test("for_loop_with_push", () => {
        expect(run(
            `pub fn double_list() -> String {
            const items = [1, 2, 3]
            let result = []
            for item in items {
                result.push(item * 2)
            }
            return result.join(",")
            crash {
                result.push -> skip
                result.join -> skip
            }
            test { self() == "2,4,6" }
        }`,
            `console.log(double_list());`,
        )).toBe("2,4,6");
    });

    test("match_all_error_arms", () => {
        expect(run(
            `pub fn fail_all(x: Number) -> String, err {
            return match x {
                1 => err.a
                2 => err.b
                _ => err.c
            }
            test {
                self(1) is err.a
                self(2) is err.b
                self(99) is err.c
            }
        }`,
            `
            const { value: r1, err: e1 } = fail_all(1);
            console.log(e1.message);
            const { value: r2, err: e2 } = fail_all(2);
            console.log(e2.message);
            const { value: r3, err: e3 } = fail_all(99);
            console.log(e3.message);
        `,
        )).toBe("a\nb\nc");
    });

    test("deeply_nested_if_else", () => {
        expect(run(
            `pub fn classify(x: Number) -> String {
            if x > 100 {
                if x > 1000 {
                    return "huge"
                } else {
                    return "big"
                }
            } else {
                if x > 0 {
                    if x > 50 {
                        return "medium"
                    } else {
                        return "small"
                    }
                } else {
                    return "zero-or-neg"
                }
            }
            test {
                self(2000) == "huge"
                self(200) == "big"
                self(75) == "medium"
                self(10) == "small"
                self(-5) == "zero-or-neg"
            }
        }`,
            `
            console.log(classify(2000));
            console.log(classify(200));
            console.log(classify(75));
            console.log(classify(10));
            console.log(classify(-5));
        `,
        )).toBe("huge\nbig\nmedium\nsmall\nzero-or-neg");
    });
});

// ═══════════════════════════════════════════════════════════
// string_interp.rs
// ═══════════════════════════════════════════════════════════

describe("string_interp", () => {
    test("basic_interpolation", () => {
        expect(run(
            `pub fn greet(name: String) -> String {
            return "hello {name}"
            test { self("cam") == "hello cam" }
        }`,
            `console.log(greet("world"));`,
        )).toBe("hello world");
    });

    test("multiple_interpolations", () => {
        expect(run(
            `pub fn intro(name: String, age: Number) -> String {
            return "{name} is {age}"
            crash { age.toString -> skip }
            test { self("cam", 25) == "cam is 25" }
        }`,
            `console.log(intro("cam", 25));`,
        )).toBe("cam is 25");
    });

    test("interpolation_at_start", () => {
        expect(run(
            `pub fn show(val: String) -> String {
            return "{val}!"
            test { self("hi") == "hi!" }
        }`,
            `console.log(show("hi"));`,
        )).toBe("hi!");
    });

    test("interpolation_at_end", () => {
        expect(run(
            `pub fn show(val: String) -> String {
            return "value: {val}"
            test { self("42") == "value: 42" }
        }`,
            `console.log(show("42"));`,
        )).toBe("value: 42");
    });

    test("no_interpolation_plain_string", () => {
        expect(run(
            `pub fn plain() -> String {
            return "no interpolation here"
            test { self() == "no interpolation here" }
        }`,
            `console.log(plain());`,
        )).toBe("no interpolation here");
    });

    test("interpolation_with_method_call", () => {
        expect(run(
            `pub fn show(n: Number) -> String {
            return "value: {n.toString()}"
            crash { n.toString -> skip }
            test { self(42) == "value: 42" }
        }`,
            `console.log(show(42));`,
        )).toBe("value: 42");
    });
});

// ═══════════════════════════════════════════════════════════
// while_loop.rs
// ═══════════════════════════════════════════════════════════

describe("while_loop", () => {
    test("basic_while", () => {
        expect(run(
            `pub fn count_to(n: Number) -> Number {
            let i = 0
            let total = 0
            while i < n {
                total = total + i
                i = i + 1
            }
            return total
            test { self(5) == 10 }
        }`,
            `console.log(count_to(5)); console.log(count_to(0));`,
        )).toBe("10\n0");
    });

    test("while_with_break", () => {
        expect(run(
            `pub fn find_first_gt(threshold: Number) -> Number {
            let i = 0
            while i < 100 {
                if i > threshold { break }
                i = i + 1
            }
            return i
            test { self(5) == 6 }
        }`,
            `console.log(find_first_gt(5)); console.log(find_first_gt(0));`,
        )).toBe("6\n1");
    });

    test("while_with_continue", () => {
        expect(run(
            `pub fn sum_odd(n: Number) -> Number {
            let i = 0
            let total = 0
            while i < n {
                i = i + 1
                if i == 2 { continue }
                if i == 4 { continue }
                total = total + i
            }
            return total
            test { self(5) == 9 }
        }`,
            `console.log(sum_odd(5));`,
        )).toBe("9");
    });

    test("while_string_builder", () => {
        expect(run(
            `pub fn repeat_str(s: String, n: Number) -> String {
            let result = ""
            let i = 0
            while i < n {
                result = result + s
                i = i + 1
            }
            return result
            test { self("ab", 3) == "ababab" }
        }`,
            `console.log(repeat_str("ha", 3));`,
        )).toBe("hahaha");
    });

    test("nested_while", () => {
        expect(run(
            `pub fn grid(rows: Number, cols: Number) -> Number {
            let count = 0
            let r = 0
            while r < rows {
                let c = 0
                while c < cols {
                    count = count + 1
                    c = c + 1
                }
                r = r + 1
            }
            return count
            test { self(3, 4) == 12 }
        }`,
            `console.log(grid(3, 4));`,
        )).toBe("12");
    });

    test("while_false_never_runs", () => {
        expect(run(
            `pub fn never() -> Number {
            let x = 0
            while false {
                x = 999
            }
            return x
            test { self() == 0 }
        }`,
            `console.log(never());`,
        )).toBe("0");
    });
});

// ═══════════════════════════════════════════════════════════
// safe_cast.rs
// ═══════════════════════════════════════════════════════════

describe("safe_cast", () => {
    test("number_cast_valid_string", () => {
        expect(run(
            `pub fn parse(s: String) -> Number {
            let n, err = Number(s)
            return n
            crash { Number -> fallback(0) }
            test { self("42") == 42 }
        }`,
            `console.log(parse("42")); console.log(parse("3.14"));`,
        )).toBe("42\n3.14");
    });

    test("number_cast_invalid_string", () => {
        expect(run(
            `pub fn parse(s: String) -> Number {
            let n, err = Number(s)
            return n
            crash { Number -> fallback(-1) }
            test { self("hello") == -1 self("42") == 42 }
        }`,
            `console.log(parse("hello")); console.log(parse("abc"));`,
        )).toBe("-1\n-1");
    });

    test("number_cast_null", () => {
        expect(run(
            `pub fn parse_or_zero(val: String) -> Number {
            let n, err = Number(val)
            return n
            crash { Number -> fallback(0) }
            test { self("5") == 5 }
        }`,
            `console.log(parse_or_zero(null));`,
        )).toBe("0");
    });

    test("string_cast_number", () => {
        expect(run(
            `pub fn to_str(n: Number) -> String {
            let s, err = String(n)
            return s
            crash { String -> fallback("error") }
            test { self(42) == "42" }
        }`,
            `console.log(to_str(42));`,
        )).toBe("42");
    });

    test("string_cast_null", () => {
        expect(run(
            `pub fn to_str(val: String) -> String {
            let s, err = String(val)
            return s
            crash { String -> fallback("was null") }
            test { self("hello") == "hello" }
        }`,
            `console.log(to_str(null));`,
        )).toBe("was null");
    });

    test("bool_cast_truthy", () => {
        expect(run(
            `pub fn to_bool(s: String) -> Bool {
            let b, err = Bool(s)
            return b
            crash { Bool -> fallback(false) }
            test { self("hello") == true }
        }`,
            `console.log(to_bool("hello")); console.log(to_bool(""));`,
        )).toBe("true\nfalse");
    });

    test("bool_cast_null", () => {
        expect(run(
            `pub fn to_bool(val: String) -> Bool {
            let b, err = Bool(val)
            return b
            crash { Bool -> fallback(false) }
            test { self("x") == true }
        }`,
            `console.log(to_bool(null));`,
        )).toBe("false");
    });

    test("null_literal", () => {
        expect(run(
            `pub fn check(s: String) -> Bool {
            if s == null { return true }
            return false
            test { self("hello") == false }
        }`,
            `console.log(check(null)); console.log(check("hello"));`,
        )).toBe("true\nfalse");
    });

    test("null_assignment", () => {
        expect(run(
            `pub fn make_null() -> String {
            const x = null
            if x == null { return "is null" }
            return "not null"
            test { self() == "is null" }
        }`,
            `console.log(make_null());`,
        )).toBe("is null");
    });

    test("number_cast_error_message", () => {
        expect(run(
            `pub fn parse(s: String) -> Number {
            let n, e = Number(s)
            return n
            crash { Number -> fallback(0) }
            test { self("42") == 42 }
        }`,
            `
            // Verify the error message from the safe cast directly
            let err = null;
            try {
                const _input = "hello";
                const _raw = Number(_input);
                if (_input === null || _input === undefined || Number.isNaN(_raw)) {
                    err = new Error("invalid_number");
                }
            } catch(e) { err = e; }
            console.log(err.message);
        `,
        )).toBe("invalid_number");
    });

    test("string_cast_null_error_message", () => {
        expect(run(
            `pub fn cast(val: String) -> String {
            let s, e = String(val)
            return s
            crash { String -> fallback("none") }
            test { self("hello") == "hello" }
        }`,
            `
            // Verify the error message from the safe cast directly
            let err = null;
            try {
                const _input = null;
                const _raw = String(_input);
                if (_input === null || _input === undefined) {
                    err = new Error("invalid_string");
                }
            } catch(e) { err = e; }
            console.log(err.message);
        `,
        )).toBe("invalid_string");
    });
});

// ═══════════════════════════════════════════════════════════
// satisfies.rs
// ═══════════════════════════════════════════════════════════

describe("satisfies", () => {
    test("satisfies_adds_instance_method", () => {
        expect(run(
            `contract Stringable { to_string() -> String }

        pub struct Email {
            value: String
        }{}

        Email satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }`,
            `
            const e = new Email({ value: "cam@test.com" });
            console.log(e.to_string());
        `,
        )).toBe("cam@test.com");
    });

    test("satisfies_two_contracts", () => {
        expect(run(
            `contract Stringable { to_string() -> String }
        contract Measurable { len() -> Number }

        pub struct Name {
            value: String
        }{}

        Name satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }

        Name satisfies Measurable {
            fn len() -> Number {
                return self.value.length
                test { self() == 4 }
            }
        }`,
            `
            const n = new Name({ value: "Cameron" });
            console.log(n.to_string());
            console.log(n.len());
        `,
        )).toBe("Cameron\n7");
    });

    test("satisfies_with_struct_own_methods", () => {
        expect(run(
            `contract Stringable { to_string() -> String }

        pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err invalid = "bad"
            }
        }{
            fn validate(raw: String) -> Email, err {
                if raw == "" { return err.invalid }
                return Email { value: raw }
                test { self("a@b.com") is Ok self("") is err.invalid }
            }
        }

        Email satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }`,
            `
            const { value: e, err: _ } = Email.validate("cam@test.com");
            console.log(e.to_string());
            console.log(e.value);
        `,
        )).toBe("cam@test.com\ncam@test.com");
    });

    test("satisfies_method_uses_self_field", () => {
        expect(run(
            `contract Describable { describe() -> String }

        pub struct Product {
            name: String
            price: Number
        }{}

        Product satisfies Describable {
            fn describe() -> String {
                return self.name + " costs " + self.price
                test { self() == "Widget costs 10" }
            }
        }`,
            `
            const p = new Product({ name: "Widget", price: 10 });
            console.log(p.describe());
        `,
        )).toBe("Widget costs 10");
    });
});

// ═══════════════════════════════════════════════════════════
// satisfies_type.rs — mostly checker tests
// ═══════════════════════════════════════════════════════════

describe("satisfies_type", () => {
    // SKIPPED: satisfies_string_registry_check — checker test
    // SKIPPED: does_not_satisfy_unimplemented — checker test
    // SKIPPED: multiple_satisfies_tracked — checker test
    // SKIPPED: secret_satisfies_loggable_but_not_string — checker test

    test("email_satisfies_string_can_be_used_as_string", () => {
        expect(run(
            `contract Trimmable { trim() -> String }

        pub struct Email {
            value: String
            create(raw: String) -> Email
        }{
            pub fn create(raw: String) -> Email {
                return Email { value: raw }
                test {}
            }
        }

        Email satisfies Trimmable {
            fn trim() -> String { return self.value.trim() crash { self.value.trim -> skip } test {} }
        }

        pub fn show_trimmed(s: String) -> String {
            return s.trim()
            crash { s.trim -> skip }
            test { self(" hello ") == "hello" }
        }`,
            `
            const email = Email.create(" cam@test.com ");
            // Call trim directly on email -- it satisfies Trimmable
            console.log(email.trim());
        `,
        )).toBe("cam@test.com");
    });
});

// ═══════════════════════════════════════════════════════════
// serialize.rs
// ═══════════════════════════════════════════════════════════

describe("serialize", () => {
    test("serializable_satisfies", () => {
        expect(run(
            `/// A user
        pub struct User {
            name: String
        }{}
        User satisfies Serializable {
            fn toJson() -> String {
                return self.name
                test { self() == "test" }
            }
        }`,
            `console.log("ok");`,
        )).toBe("ok");
    });

    test("deserializable_satisfies_generic", () => {
        expect(run(
            `/// A user
        pub struct User {
            name: String
            create(data: String) -> User, err {
                err invalid = "bad"
            }
        }{
            pub fn create(data: String) -> User, err {
                if data == "" { return err.invalid }
                return User { name: data }
                test {
                    self("cam") is Ok
                    self("") is err.invalid
                }
            }
        }
        User satisfies Deserializable<User> {
            fn parse(data: String) -> User, err {
                return User { name: data }
                test { self("cam") is Ok }
            }
        }`,
            `
            const { value } = User.create("cam");
            console.log(value.name);
        `,
        )).toBe("cam");
    });
});

// ═══════════════════════════════════════════════════════════
// type_coercion.rs
// ═══════════════════════════════════════════════════════════

describe("type_coercion", () => {
    test("number_to_string", () => {
        expect(run(
            `pub fn stringify(n: Number) -> String {
            return n.toString()
            crash { n.toString -> skip }
            test { self(42) == "42" }
        }`,
            `console.log(stringify(42));`,
        )).toBe("42");
    });

    test("number_in_string_concat", () => {
        expect(run(
            `pub fn label(name: String, age: Number) -> String {
            return name + " is " + age.toString()
            crash { age.toString -> skip }
            test { self("cam", 25) == "cam is 25" }
        }`,
            `console.log(label("cam", 25));`,
        )).toBe("cam is 25");
    });

    test("bool_to_string", () => {
        expect(run(
            `pub fn show(b: Bool) -> String {
            return b.toString()
            crash { b.toString -> skip }
            test { self(true) == "true" }
        }`,
            `console.log(show(true)); console.log(show(false));`,
        )).toBe("true\nfalse");
    });

    test("number_literal_method", () => {
        expect(run(
            `pub fn fixed() -> String {
            const x = 42
            return x.toString()
            crash { x.toString -> skip }
            test { self() == "42" }
        }`,
            `console.log(fixed());`,
        )).toBe("42");
    });

    test("array_join", () => {
        expect(run(
            `pub fn csv(items: String) -> String {
            const arr = ["a", "b", "c"]
            return arr.join(", ")
            crash { arr.join -> skip }
            test { self("x") == "a, b, c" }
        }`,
            `console.log(csv("x"));`,
        )).toBe("a, b, c");
    });

    test("string_includes", () => {
        expect(run(
            `pub fn has_at(s: String) -> Bool {
            return s.includes("@")
            crash { s.includes -> skip }
            test { self("a@b") == true self("nope") == false }
        }`,
            `console.log(has_at("cam@test.com")); console.log(has_at("nope"));`,
        )).toBe("true\nfalse");
    });
});

// ═══════════════════════════════════════════════════════════
// type_convert.rs
// ═══════════════════════════════════════════════════════════

describe("type_convert", () => {
    test("number_to_string", () => {
        expect(run(
            `pub fn convert(n: Number) -> String {
            return String(n)
            crash { String -> skip }
            test { self(42) == "42" }
        }`,
            `console.log(convert(42));`,
        )).toBe("42");
    });

    test("bool_to_string_via_convert", () => {
        expect(run(
            `pub fn convert(b: Bool) -> String {
            return String(b)
            crash { String -> skip }
            test { self(true) == "true" }
        }`,
            `console.log(convert(true)); console.log(convert(false));`,
        )).toBe("true\nfalse");
    });

    test("string_to_string_noop", () => {
        expect(run(
            `pub fn convert(s: String) -> String {
            return String(s)
            crash { String -> skip }
            test { self("hello") == "hello" }
        }`,
            `console.log(convert("hello"));`,
        )).toBe("hello");
    });

    test("string_to_number", () => {
        expect(run(
            `pub fn convert(s: String) -> Number {
            return Number(s)
            crash { Number -> skip }
            test { self("42") == 42 }
        }`,
            `console.log(convert("42")); console.log(convert("3.14"));`,
        )).toBe("42\n3.14");
    });

    test("bool_to_number", () => {
        expect(run(
            `pub fn convert(b: Bool) -> Number {
            return Number(b)
            crash { Number -> skip }
            test { self(true) == 1 }
        }`,
            `console.log(convert(true)); console.log(convert(false));`,
        )).toBe("1\n0");
    });

    test("string_to_bool", () => {
        expect(run(
            `pub fn convert(s: String) -> Bool {
            return Bool(s)
            crash { Bool -> skip }
            test { self("hello") == true }
        }`,
            `console.log(convert("hello")); console.log(convert(""));`,
        )).toBe("true\nfalse");
    });

    test("number_to_bool", () => {
        expect(run(
            `pub fn convert(n: Number) -> Bool {
            return Bool(n)
            crash { Bool -> skip }
            test { self(1) == true }
        }`,
            `console.log(convert(1)); console.log(convert(0));`,
        )).toBe("true\nfalse");
    });

    test("concat_with_plus", () => {
        expect(run(
            `pub fn greet(first: String, last: String) -> String {
            return first + " " + last
            test { self("John", "Doe") == "John Doe" }
        }`,
            `console.log(greet("Jane", "Smith"));`,
        )).toBe("Jane Smith");
    });

    test("concat_number_needs_conversion", () => {
        expect(run(
            `pub fn label(name: String, age: Number) -> String {
            return name + " is " + String(age)
            crash { String -> skip }
            test { self("cam", 25) == "cam is 25" }
        }`,
            `console.log(label("cam", 25));`,
        )).toBe("cam is 25");
    });

    test("interpolation_still_works", () => {
        expect(run(
            `pub fn greet(name: String) -> String {
            return "hello {name}!"
            test { self("world") == "hello world!" }
        }`,
            `console.log(greet("roca"));`,
        )).toBe("hello roca!");
    });

    // SKIPPED: struct_still_uses_new — emitter test
    // SKIPPED: string_conversion_no_new — emitter test
});

// ═══════════════════════════════════════════════════════════
// integration.rs
// ═══════════════════════════════════════════════════════════

describe("integration", () => {
    test("full_email_pipeline", () => {
        expect(run(
            `contract Stringable { to_string() -> String }

        pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err missing = "value is required"
                err invalid = "format is not valid"
            }
        }{
            pub fn validate(raw: String) -> Email, err {
                if raw == "" { return err.missing }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.missing
                    self("x") is err.invalid
                }
            }
        }

        Email satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "a@b.com" }
            }
        }

        pub fn format_email(raw: String) -> String {
            const result = Email.validate(raw)
            return "ok"
            crash { Email.validate -> fallback(fn(e) -> "error") }
            test {
                self("a@b.com") == "ok"
            }
        }`,
            `
            // Validate and use
            const { value: email, err } = Email.validate("cam@test.com");
            console.log(email.value);
            console.log(email.to_string());
            console.log(err);

            // Validate failure
            const { value: bad, err: err2 } = Email.validate("");
            console.log(bad);
            console.log(err2.name);
            console.log(err2.message);

            // Contract errors accessible
            console.log(typeof EmailErrors === "undefined");
        `,
        )).toBe("cam@test.com\ncam@test.com\nnull\nnull\nmissing\nvalue is required\ntrue");
    });

    test("full_user_registration", () => {
        expect(run(
            `contract Stringable { to_string() -> String }

        pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err invalid = "invalid email"
            }
        }{
            pub fn validate(raw: String) -> Email, err {
                if raw == "" { return err.invalid }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.invalid
                }
            }
        }

        Email satisfies Stringable {
            fn to_string() -> String {
                return self.value
                test { self() == "test" }
            }
        }

        pub struct User {
            name: String
            email: Email
            validate(name: String, email_raw: String) -> User, err {
                err missing_name = "name is required"
                err bad_email = "email is invalid"
            }
        }{
            pub fn validate(name: String, email_raw: String) -> User, err {
                if name == "" { return err.missing_name }
                if email_raw == "" { return err.bad_email }
                const email_result = Email.validate(email_raw)
                return User { name: name, email: email_result }
                crash { Email.validate -> skip }
                test {
                    self("cam", "a@b.com") is Ok
                    self("", "a@b.com") is err.missing_name
                    self("cam", "") is err.bad_email
                }
            }
        }

        User satisfies Stringable {
            fn to_string() -> String {
                return self.name
                test { self() == "cam" }
            }
        }`,
            `
            // Note: Email.validate returns { value: Email, err: null } object
            // but User.validate uses it directly -- crash halt would throw on error
            // For this test, we call with valid data
            const { value: emailResult, err: _e0 } = Email.validate("cam@test.com");
            const { value: user, err: _e1 } = User.validate("cam", "cam@test.com");

            console.log(user.name);
            console.log(user.to_string());

            // Test error path
            const { value: bad, err } = User.validate("", "a@b.com");
            console.log(bad);
            console.log(err.name);
        `,
        )).toBe("cam\ncam\nnull\nmissing_name");
    });

    test("struct_uses_another_struct", () => {
        expect(run(
            `pub struct Point {
            x: Number
            y: Number
            create(x: Number, y: Number) -> Point
        }{
            pub fn create(x: Number, y: Number) -> Point {
                return Point { x: x, y: y }
                test { self(1, 2) == Point { x: 1, y: 2 } }
            }
        }

        pub struct Line {
            start: Point
            end: Point
            create(x1: Number, y1: Number, x2: Number, y2: Number) -> Line
        }{
            pub fn create(x1: Number, y1: Number, x2: Number, y2: Number) -> Line {
                const s = Point.create(x1, y1)
                const e = Point.create(x2, y2)
                return Line { start: s, end: e }
                crash {
                    Point.create -> skip
                }
                test { self(0, 0, 1, 1) == Line { start: Point { x: 0, y: 0 }, end: Point { x: 1, y: 1 } } }
            }
        }`,
            `
            const line = Line.create(0, 0, 10, 20);
            console.log(line.start.x);
            console.log(line.start.y);
            console.log(line.end.x);
            console.log(line.end.y);
        `,
        )).toBe("0\n0\n10\n20");
    });

    test("let_result_destructure", () => {
        expect(run(
            `/// Safe division
        pub struct SafeDivide {
            call(a: Number, b: Number) -> Number, err {
                err div_zero = "div_zero"
            }
        }{
            pub fn call(a: Number, b: Number) -> Number, err {
                if b == 0 { return err.div_zero }
                return a / b
                test {
                    self(10, 2) == 5
                    self(10, 0) is err.div_zero
                }
            }
        }

        /// Computes division result
        pub fn compute(a: Number, b: Number) -> String {
            const result = SafeDivide.call(a, b)
            return "result: " + result
            crash { SafeDivide.call -> fallback(fn(e) -> "error: " + e.message) }
            test { self(10, 2) == "result: 5" }
        }`,
            `
            console.log(compute(10, 2));
        `,
        )).toBe("result: 5");
    });

    test("method_chaining_on_string", () => {
        expect(run(
            `pub fn process(input: String) -> String {
            const trimmed = input.trim()
            const upper = trimmed.toUpperCase()
            return upper
            crash {
                input.trim -> skip
                trimmed.toUpperCase -> skip
            }
            test { self(" hello ") == "HELLO" }
        }`,
            `console.log(process("  hello  "));`,
        )).toBe("HELLO");
    });

    test("string_length", () => {
        expect(run(
            `pub fn char_count(s: String) -> Number {
            return s.length
            test { self("hello") == 5 }
        }`,
            `console.log(char_count("hello world"));`,
        )).toBe("11");
    });

    test("struct_satisfies_three_contracts", () => {
        expect(run(
            `contract Stringable { to_string() -> String }
        contract Measurable { size() -> Number }
        contract Describable { describe() -> String }

        pub struct Config {
            name: String
            value: Number
        }{}

        Config satisfies Stringable {
            fn to_string() -> String {
                return self.name + "=" + self.value
                test { self() == "timeout=30" }
            }
        }

        Config satisfies Measurable {
            fn size() -> Number {
                return self.value
                test { self() == 30 }
            }
        }

        Config satisfies Describable {
            fn describe() -> String {
                return "Config(" + self.name + ")"
                test { self() == "Config(timeout)" }
            }
        }`,
            `
            const c = new Config({ name: "timeout", value: 30 });
            console.log(c.to_string());
            console.log(c.size());
            console.log(c.describe());
        `,
        )).toBe("timeout=30\n30\nConfig(timeout)");
    });

    test("error_checked_with_truthiness", () => {
        expect(run(
            `/// Validates input
        pub struct Validate {
            call(s: String) -> String, err {
                err empty = "empty"
                err invalid = "invalid"
            }
        }{
            pub fn call(s: String) -> String, err {
                if s == "" { return err.empty }
                if s == "bad" { return err.invalid }
                return s
                test {
                    self("ok") == "ok"
                    self("") is err.empty
                    self("bad") is err.invalid
                }
            }
        }`,
            `
            const { value: v1, err: e1 } = Validate.call("hello");
            console.log(e1 ? "error" : "ok");

            const { value: v2, err: e2 } = Validate.call("");
            console.log(e2 ? "error" : "ok");
            console.log(e2.message);

            const { value: v3, err: e3 } = Validate.call("bad");
            console.log(e3 ? "error" : "ok");
            console.log(e3.message);
        `,
        )).toBe("ok\nerror\nempty\nerror\ninvalid");
    });

    test("enum_contract_used_in_logic", () => {
        expect(run(
            `contract StatusCode { 200 400 500 }

        pub fn status_message(code: Number) -> String {
            if code == 200 { return "ok" }
            if code == 400 { return "bad request" }
            if code == 500 { return "server error" }
            return "unknown"
            test {
                self(200) == "ok"
                self(400) == "bad request"
                self(999) == "unknown"
            }
        }`,
            `
            console.log(StatusCode["200"]);
            console.log(status_message(200));
            console.log(status_message(400));
            console.log(status_message(500));
            console.log(status_message(999));
        `,
        )).toBe("200\nok\nbad request\nserver error\nunknown");
    });
});

// ═══════════════════════════════════════════════════════════
// loggable.rs
// ═══════════════════════════════════════════════════════════

describe("loggable", () => {
    test("log_string_works", () => {
        expect(run(
            `pub fn greet(name: String) -> String {
            log(name)
            return name
            crash { log -> skip }
            test { self("cam") == "cam" }
        }`,
            `greet("cam");`,
        )).toBe("cam");
    });

    test("log_number_works", () => {
        expect(run(
            `pub fn show(n: Number) -> Number {
            log(n)
            return n
            crash { log -> skip }
            test { self(42) == 42 }
        }`,
            `show(42);`,
        )).toBe("42");
    });

    test("log_bool_works", () => {
        expect(run(
            `pub fn check(b: Bool) -> Bool {
            log(b)
            return b
            crash { log -> skip }
            test { self(true) == true }
        }`,
            `check(true);`,
        )).toBe("true");
    });

    test("error_works_like_log", () => {
        expect(run(
            `pub fn fail(msg: String) -> String {
            error(msg)
            return msg
            crash { error -> skip }
            test { self("oops") == "oops" }
        }`,
            `console.log(fail("oops"));`,
        )).toBe("oops");
    });

    test("warn_works_like_log", () => {
        expect(run(
            `pub fn caution(msg: String) -> String {
            warn(msg)
            return msg
            crash { warn -> skip }
            test { self("careful") == "careful" }
        }`,
            `console.log(caution("careful"));`,
        )).toBe("careful");
    });

    test("log_with_to_log_call", () => {
        expect(run(
            `pub fn show(n: Number) -> String {
            const msg = n.toString()
            log(msg)
            return msg
            crash {
                n.toString -> skip
                log -> skip
            }
            test { self(42) == "42" }
        }`,
            `show(42);`,
        )).toBe("42");
    });

    // SKIPPED: array_cannot_be_logged — checker test
    // SKIPPED: map_cannot_be_logged — checker test
    // SKIPPED: custom_struct_cannot_be_logged_without_to_log — checker test
    // SKIPPED: error_also_requires_loggable — checker test
    // SKIPPED: warn_also_requires_loggable — checker test

    test("secret_with_to_log_redacts", () => {
        expect(run(
            `contract Loggable { to_log() -> String }

        pub struct Secret {
            value: String
            create(v: String) -> Secret
        }{
            pub fn create(v: String) -> Secret {
                return Secret { value: v }
                test {}
            }
        }

        Secret satisfies Loggable {
            fn to_log() -> String {
                return "REDACTED"
                test {}
            }
        }

        pub fn process(s: String) -> String {
            const secret = Secret.create(s)
            log(secret.to_log())
            return "done"
            crash {
                Secret.create -> skip
                secret.to_log -> halt
                log -> skip
            }
            test { self("my-password") == "done" }
        }`,
            `process("super-secret-password");`,
        )).toBe("REDACTED");
    });

    test("email_with_to_log_shows_value", () => {
        expect(run(
            `contract Loggable { to_log() -> String }

        pub struct Email {
            value: String
            create(v: String) -> Email
        }{
            pub fn create(v: String) -> Email {
                return Email { value: v }
                test {}
            }
        }

        Email satisfies Loggable {
            fn to_log() -> String {
                return self.value
                test {}
            }
        }

        pub fn process(email_raw: String) -> String {
            const email = Email.create(email_raw)
            log(email.to_log())
            return "done"
            crash {
                Email.create -> skip
                email.to_log -> halt
                log -> skip
            }
            test { self("cam@test.com") == "done" }
        }`,
            `process("cam@test.com");`,
        )).toBe("cam@test.com");
    });

    test("credit_card_logs_last_four", () => {
        expect(run(
            `contract Loggable { to_log() -> String }

        pub struct CreditCard {
            number: String
            create(n: String) -> CreditCard
        }{
            pub fn create(n: String) -> CreditCard {
                return CreditCard { number: n }
                test {}
            }
        }

        CreditCard satisfies Loggable {
            fn to_log() -> String {
                return "****" + self.number.slice(12)
                crash { self.number.slice -> skip }
                test {}
            }
        }

        pub fn charge(card_num: String) -> String {
            const card = CreditCard.create(card_num)
            log(card.to_log())
            return "charged"
            crash {
                CreditCard.create -> skip
                card.to_log -> halt
                log -> skip
            }
            test { self("4242424242421234") == "charged" }
        }`,
            `charge("4242424242421234");`,
        )).toBe("****1234");
    });
});

// ═══════════════════════════════════════════════════════════
// adapter.rs
// ═══════════════════════════════════════════════════════════

describe("adapter", () => {
    test("adapter_pattern_basic", () => {
        expect(run(
            `extern contract Database {
            query(sql: String) -> String, err {
                err failed = "query failed"
            }
        }

        pub struct Runtime {
            db: Database
        }{}

        pub fn get_user(rt: Runtime) -> String {
            const result = rt.db.query("SELECT name FROM users")
            return result
            crash { rt.db.query -> fallback("unknown") }
            test { self(Runtime { db: null }) == "alice" }
        }`,
            `
            const rt = { db: { query: (sql) => ({ value: "alice", err: null }) } };
            console.log(get_user(rt));
        `,
        )).toBe("alice");
    });

    test("adapter_pattern_error_propagation", () => {
        expect(run(
            `extern contract Database {
            query(sql: String) -> String, err {
                err failed = "query failed"
            }
        }

        pub struct Runtime {
            db: Database
        }{}

        pub fn get_user(rt: Runtime) -> String, err {
            if false { return err.failed }
            const result = rt.db.query("SELECT 1")
            return result
            crash { rt.db.query -> log |> halt }
            test { self(Runtime { db: null }) == "ok" self(Runtime { db: null }) is err.failed }
        }`,
            `
            const rt = { db: { query: (sql) => ({ value: "", err: { name: "failed", message: "query failed" } }) } };
            const { value: val, err } = get_user(rt);
            console.log(err.name);
            console.log(err.message);
        `,
        )).toBe("failed\nquery failed");
    });

    test("adapter_multiple_externs", () => {
        expect(run(
            `extern contract HttpClient {
            get(url: String) -> String, err {
                err network = "network error"
            }
        }

        extern contract Cache {
            get(key: String) -> String | null
        }

        pub struct Services {
            http: HttpClient
            cache: Cache
        }{}

        pub fn fetch_data(svc: Services, url: String) -> String {
            const result = svc.http.get(url)
            return result
            crash {
                svc.http.get -> fallback("offline")
            }
            test { self(Services { http: null, cache: null }, "/api") == "data" }
        }`,
            `
            const svc = {
                http: { get: (url) => ({ value: "hello from " + url, err: null }) },
                cache: { get: (key) => null }
            };
            console.log(fetch_data(svc, "/api"));
        `,
        )).toBe("hello from /api");
    });

    test("adapter_with_fallback", () => {
        expect(run(
            `extern contract Logger {
            info(msg: String) -> Ok
        }

        pub struct App {
            logger: Logger
        }{}

        pub fn greet(app: App, name: String) -> String {
            app.logger.info("greeting " + name)
            return "Hello " + name
            crash { app.logger.info -> skip }
            test { self(App { logger: null }, "cam") == "Hello cam" }
        }`,
            `
            const app = { logger: { info: (msg) => ({ value: null, err: null }) } };
            console.log(greet(app, "world"));
        `,
        )).toBe("Hello world");
    });
});

// ═══════════════════════════════════════════════════════════
// wait.rs — mostly emitter/checker tests
// ═══════════════════════════════════════════════════════════

describe("wait", () => {
    // SKIPPED: wait_single_emits_async — emitter test
    // SKIPPED: wait_all_emits_promise_all — emitter test
    // SKIPPED: wait_first_emits_promise_race — emitter test
    // SKIPPED: no_wait_stays_sync — emitter test

    test("wait_resolves_value", () => {
        expect(run(
            `pub fn test_wait() -> String {
            return "sync"
            test { self() == "sync" }
        }`,
            `
            async function main() {
                const result = await Promise.resolve("resolved");
                console.log(result);
            }
            main();
        `,
        )).toBe("resolved");
    });

    test("wait_catches_failure", () => {
        expect(run(
            `pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }`,
            `
            async function main() {
                let result;
                let failed;
                try {
                    result = await Promise.reject(new Error("network error"));
                } catch(_e) {
                    failed = _e;
                }
                console.log(result === undefined);
                console.log(failed.message);
            }
            main();
        `,
        )).toBe("true\nnetwork error");
    });

    // SKIPPED: wait_emitted_try_catch_structure — emitter test

    test("wait_all_resolves_multiple", () => {
        expect(run(
            `pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }`,
            `
            async function main() {
                const [a, b] = await Promise.all([
                    Promise.resolve("first"),
                    Promise.resolve("second")
                ]);
                console.log(a);
                console.log(b);
            }
            main();
        `,
        )).toBe("first\nsecond");
    });

    test("wait_all_fails_if_any_fails", () => {
        expect(run(
            `pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }`,
            `
            async function main() {
                let results;
                let failed;
                try {
                    results = await Promise.all([
                        Promise.resolve("ok"),
                        Promise.reject(new Error("second failed"))
                    ]);
                } catch(_e) {
                    failed = _e;
                }
                console.log(results === undefined);
                console.log(failed.message);
            }
            main();
        `,
        )).toBe("true\nsecond failed");
    });

    // SKIPPED: wait_all_emitted_structure — emitter test
    // SKIPPED: wait_all_three_calls — emitter test

    test("wait_first_resolves_fastest", () => {
        expect(run(
            `pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }`,
            `
            async function main() {
                const result = await Promise.race([
                    new Promise(r => setTimeout(() => r("slow"), 100)),
                    Promise.resolve("fast")
                ]);
                console.log(result);
            }
            main();
        `,
        )).toBe("fast");
    });

    // SKIPPED: wait_first_emitted_structure — emitter test
    // SKIPPED: function_with_wait_is_async — emitter test
    // SKIPPED: function_without_wait_is_sync — emitter test
    // SKIPPED: wait_in_if_branch_makes_async — emitter test
    // SKIPPED: wait_calls_appear_in_crash — checker test
    // SKIPPED: wait_with_crash_passes — checker test
    // SKIPPED: multiple_waits_in_sequence — emitter test
    // SKIPPED: wait_then_sync_code — emitter test
    // SKIPPED: wait_all_single_call — emitter test
    // SKIPPED: wait_first_single_call — emitter test
    // SKIPPED: wait_with_error_handling_pattern — emitter test
});

// ═══════════════════════════════════════════════════════════
// stdlib.rs
// ═══════════════════════════════════════════════════════════

describe("stdlib", () => {
    test("log_emits_console_log", () => {
        expect(run(
            `pub fn greet(name: String) -> String {
            log("hello " + name)
            return name
            crash { log -> skip }
            test { self("cam") == "cam" }
        }`,
            `greet("world");`,
        )).toBe("hello world");
    });

    test("log_multiple_calls", () => {
        expect(run(
            `pub fn count() -> Number {
            log("one")
            log("two")
            log("three")
            return 3
            crash {
                log -> skip
            }
            test { self() == 3 }
        }`,
            `count();`,
        )).toBe("one\ntwo\nthree");
    });

    test("map_basic_operations", () => {
        expect(run(
            `pub fn use_map() -> Bool {
            const m = Map()
            m.set("name", "cam")
            m.set("city", "rothenburg")
            const val = m.get("name")
            return m.has("name")
            crash {
                Map -> skip
                m.set -> skip
                m.get -> skip
                m.has -> skip
            }
            test { self() == true }
        }`,
            `
            console.log(use_map());
            // Verify actual get value from JS
            const m = new Map();
            m.set("name", "cam");
            console.log(m.get("name"));
        `,
        )).toBe("true\ncam");
    });

    test("map_has_and_size", () => {
        expect(run(
            `pub fn check_map() -> Bool {
            const m = Map()
            m.set("key", "val")
            return m.has("key")
            crash {
                Map -> skip
                m.set -> skip
                m.has -> skip
            }
            test { self() == true }
        }`,
            `console.log(check_map());`,
        )).toBe("true");
    });

    test("string_valid_methods_compile", () => {
        expect(run(
            `pub fn process(s: String) -> String {
            const trimmed = s.trim()
            const upper = trimmed.toUpperCase()
            const has_a = upper.includes("A")
            return upper
            crash {
                s.trim -> skip
                trimmed.toUpperCase -> skip
                upper.includes -> skip
            }
            test { self(" hello ") == "HELLO" }
        }`,
            `console.log(process(" hello "));`,
        )).toBe("HELLO");
    });

    test("number_tostring_works", () => {
        expect(run(
            `pub fn show(n: Number) -> String {
            return n.toString()
            crash { n.toString -> skip }
            test { self(42) == "42" }
        }`,
            `console.log(show(42));`,
        )).toBe("42");
    });

    test("number_tofixed_works", () => {
        expect(run(
            `pub fn format(n: Number) -> String {
            return n.toFixed(2)
            crash { n.toFixed -> skip }
            test { self(3.14159) == "3.14" }
        }`,
            `console.log(format(3.14159));`,
        )).toBe("3.14");
    });
});

// ═══════════════════════════════════════════════════════════
// bytes.rs
// ═══════════════════════════════════════════════════════════

describe("bytes", () => {
    test("bytes_from_string", () => {
        expect(run(
            `import { Encoding } from std::encoding
        pub fn encode(s: String) -> Number {
            const bytes = Encoding.encode(s)
            return bytes.byteLength
            crash {
                Encoding.encode -> skip
                bytes.byteLength -> skip
            }
            test { self("hello") == 5 }
        }`,
            `console.log(encode("hello"));`,
        )).toBe("5");
    });

    test("bytes_to_string", () => {
        expect(run(
            `import { Encoding } from std::encoding
        pub fn decode(s: String) -> String {
            const bytes = Encoding.encode(s)
            const result = Encoding.decode(bytes)
            return result
            crash {
                Encoding.encode -> skip
                Encoding.decode -> fallback("")
            }
            test { self("hello") == "hello" }
        }`,
            `
            console.log(decode("hello"));
        `,
        )).toBe("hello");
    });

    test("uint8array_operations", () => {
        expect(run(
            `pub fn byte_work() -> Number {
            const arr = Uint8Array(4)
            return arr.byteLength
            crash {
                Uint8Array -> skip
                arr.byteLength -> skip
            }
            test { self() == 4 }
        }`,
            `console.log(byte_work());`,
        )).toBe("4");
    });

    test("buffer_create", () => {
        expect(run(
            `pub fn make_buffer() -> Number {
            const buf = ArrayBuffer(16)
            return buf.byteLength
            crash {
                ArrayBuffer -> skip
                buf.byteLength -> skip
            }
            test { self() == 16 }
        }`,
            `console.log(make_buffer());`,
        )).toBe("16");
    });

    test("base64_roundtrip", () => {
        expect(run(
            `pub fn roundtrip(s: String) -> String {
            const encoded = btoa(s)
            const decoded = atob(encoded)
            return decoded
            crash {
                btoa -> skip
                atob -> skip
            }
            test { self("hello") == "hello" }
        }`,
            `console.log(roundtrip("hello world"));`,
        )).toBe("hello world");
    });

    // SKIPPED: bytes_has_methods — checker/registry test
    // SKIPPED: buffer_has_methods — checker/registry test
});

// ═══════════════════════════════════════════════════════════
// crypto.rs
// ═══════════════════════════════════════════════════════════

describe("crypto", () => {
    test("crypto_random_uuid", () => {
        expect(run(
            `import { Crypto } from std::crypto
        /// Gets a UUID
        pub fn new_id() -> String {
            return Crypto.randomUUID()
            test {}
        }`,
            `
            const id = new_id();
            console.log(id.length === 36 ? "ok" : "fail:" + id.length);
        `,
        )).toBe("ok");
    });

    test("crypto_uuid_unique", () => {
        expect(run(
            `import { Crypto } from std::crypto
        /// Gets two UUIDs
        pub fn two_ids() -> String {
            const a = Crypto.randomUUID()
            const b = Crypto.randomUUID()
            if a == b { return "same" }
            return "different"
            test {}
        }`,
            `console.log(two_ids());`,
        )).toBe("different");
    });

    test("crypto_sha256", () => {
        expect(run(
            `import { Crypto } from std::crypto
        /// Hashes data
        pub fn hash(s: String) -> String {
            const h = wait Crypto.sha256(s)
            return h
            crash { Crypto.sha256 -> skip }
            test { self("test") == "" }
        }`,
            `
            const h = await hash("");
            console.log(h.length);
        `,
        )).toBe("64");
    });
});

// ═══════════════════════════════════════════════════════════
// encoding.rs
// ═══════════════════════════════════════════════════════════

describe("encoding", () => {
    test("btoa_encodes_to_base64", () => {
        expect(run(
            `import { Encoding } from std::encoding
        pub fn to_b64(s: String) -> String {
            const result = Encoding.btoa(s)
            return result
            crash { Encoding.btoa -> fallback("") }
            test { self("hello") == "aGVsbG8=" }
        }`,
            `
            console.log(to_b64("hello"));
        `,
        )).toBe("aGVsbG8=");
    });

    test("atob_decodes_from_base64", () => {
        expect(run(
            `import { Encoding } from std::encoding
        pub fn from_b64(s: String) -> String {
            const result = Encoding.atob(s)
            return result
            crash { Encoding.atob -> fallback("") }
            test { self("aGVsbG8=") == "hello" }
        }`,
            `
            console.log(from_b64("aGVsbG8="));
        `,
        )).toBe("hello");
    });

    test("btoa_atob_roundtrip", () => {
        expect(run(
            `import { Encoding } from std::encoding
        pub fn roundtrip(s: String) -> String {
            const encoded = Encoding.btoa(s)
            const decoded = Encoding.atob(encoded)
            return decoded
            crash {
                Encoding.btoa -> fallback("")
                Encoding.atob -> fallback("")
            }
            test { self("test data") == "test data" }
        }`,
            `
            console.log(roundtrip("test data"));
        `,
        )).toBe("test data");
    });
});

// ═══════════════════════════════════════════════════════════
// http.rs — all checker/emitter tests
// ═══════════════════════════════════════════════════════════

// SKIPPED: http_contract_registered — checker test
// SKIPPED: http_post_registered — checker test
// SKIPPED: http_js_uses_runtime — emitter test

// ═══════════════════════════════════════════════════════════
// repl.rs — uses custom harness (spawns roca repl), not run()
// ═══════════════════════════════════════════════════════════

// SKIPPED: arithmetic — REPL test (spawns roca repl binary)
// SKIPPED: string_method — REPL test
// SKIPPED: boolean_logic — REPL test
// SKIPPED: string_concat — REPL test
// SKIPPED: define_and_call_function — REPL test
// SKIPPED: struct_as_json — REPL test
// SKIPPED: array_join — REPL test
// SKIPPED: compiler_error_shown — REPL test

// ═══════════════════════════════════════════════════════════
// time.rs
// ═══════════════════════════════════════════════════════════

describe("time", () => {
    test("time_now_returns_positive", () => {
        expect(run(
            `import { Time } from std::time
        pub fn ts() -> Number {
            return Time.now()
            test {}
        }`,
            `
            const v = ts();
            console.log(v > 0 ? "ok" : "fail");
        `,
        )).toBe("ok");
    });

    test("time_parse_valid_iso", () => {
        expect(run(
            `import { Time } from std::time
        pub fn parse_ts(s: String) -> Number {
            const ts = Time.parse(s)
            return ts
            crash { Time.parse -> fallback(fn(e) -> 0) }
            test { self("2026-01-01T00:00:00Z") == 0 }
        }`,
            `
            const v = parse_ts("2026-01-01T00:00:00Z");
            console.log(v > 0 ? "ok" : "fail");
        `,
        )).toBe("ok");
    });

    test("time_parse_invalid_propagates", () => {
        expect(run(
            `import { Time } from std::time
        pub fn try_parse(s: String) -> Number, err {
            if false { return err.parse_failed }
            const ts = Time.parse(s)
            return ts
            crash { Time.parse -> halt }
            test { self("2026-01-01") is Ok self("bad") is err.parse_failed }
        }`,
            `
            const { value, err } = try_parse("not a date");
            console.log(err ? err.name : "no error");
        `,
        )).toBe("parse_failed");
    });

    test("time_parse_fallback", () => {
        expect(run(
            `import { Time } from std::time
        pub fn safe_parse(s: String) -> Number {
            const ts = Time.parse(s)
            return ts
            crash { Time.parse -> fallback(fn(e) -> 0) }
            test { self("2026-01-01") == 0 }
        }`,
            `
            const v = safe_parse("not a date");
            console.log(v === 0 ? "fallback" : "parsed");
        `,
        )).toBe("fallback");
    });
});

// ═══════════════════════════════════════════════════════════
// url.rs
// ═══════════════════════════════════════════════════════════

describe("url", () => {
    test("url_parse_valid", () => {
        expect(run(
            `import { Url } from std::url
        pub fn get_host(raw: String) -> String {
            const url = Url.parse(raw)
            return url.hostname()
            crash { Url.parse -> fallback(fn(e) -> "") }
            test { self("https://example.com/path") == "example.com" }
        }`,
            `console.log(get_host("https://example.com:8080/path?q=1"));`,
        )).toBe("example.com");
    });

    test("url_parse_invalid", () => {
        expect(run(
            `import { Url } from std::url
        pub fn try_host(raw: String) -> String, err {
            if false { return err.parse_failed }
            const url = Url.parse(raw)
            return url.hostname()
            crash { Url.parse -> halt }
            test { self("https://example.com") == "example.com" self("bad") is err.parse_failed }
        }`,
            `
            const { err } = try_host("not a url");
            console.log(err ? err.name : "no error");
        `,
        )).toBe("parse_failed");
    });

    test("url_is_valid", () => {
        expect(run(
            `import { Url } from std::url
        pub fn check(raw: String) -> Bool {
            return Url.isValid(raw)
            test { self("https://example.com") == true }
        }`,
            `
            console.log(check("https://example.com"));
            console.log(check("not a url"));
        `,
        )).toBe("true\nfalse");
    });

    test("url_parts", () => {
        expect(run(
            `import { Url } from std::url
        pub fn parts(raw: String) -> String {
            const url = Url.parse(raw)
            return url.protocol() + " " + url.pathname() + " " + url.search()
            crash { Url.parse -> fallback(fn(e) -> "") }
            test { self("https://example.com/path?q=1") == "https: /path ?q=1" }
        }`,
            `console.log(parts("https://example.com/path?q=1"));`,
        )).toBe("https: /path ?q=1");
    });

    test("url_get_param", () => {
        expect(run(
            `import { Url } from std::url
        pub fn param(raw: String, name: String) -> String {
            const url = Url.parse(raw)
            const val = url.getParam(name)
            if val == null { return "none" }
            return "" + val
            crash { Url.parse -> fallback(fn(e) -> "") }
            test { self("https://x.com?a=1", "a") == "1" }
        }`,
            `
            console.log(param("https://x.com?foo=bar&baz=42", "foo"));
            console.log(param("https://x.com?foo=bar", "missing"));
        `,
        )).toBe("bar\nnone");
    });
});
