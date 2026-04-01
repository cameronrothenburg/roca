// Layer 3: Cross-file Roca projects compile and run as JS
import { test, expect, beforeAll, afterAll } from "bun:test";
import { execSync } from "node:child_process";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "../..");
const ROCA = `cargo run --quiet --manifest-path ${ROOT}/Cargo.toml --`;
const API_PROJECT = join(__dirname, "projects/api");
const OUT = join(API_PROJECT, "out");

beforeAll(() => {
    // Clean previous output
    if (existsSync(OUT)) rmSync(OUT, { recursive: true });

    // Build the project
    execSync(`${ROCA} build ${API_PROJECT}`, { cwd: ROOT, stdio: "pipe" });

    // Link local runtime in the output
    const pkg = JSON.parse(readFileSync(join(OUT, "package.json"), "utf8"));
    pkg.dependencies = pkg.dependencies || {};
    pkg.dependencies["@rocalang/runtime"] = `file:${join(ROOT, "packages/runtime")}`;
    pkg.type = "module";
    writeFileSync(join(OUT, "package.json"), JSON.stringify(pkg, null, 2));

    execSync("npm install --silent", { cwd: OUT, stdio: "pipe" });
});

afterAll(() => {
    if (existsSync(OUT)) rmSync(OUT, { recursive: true });
});

function runJS(script) {
    const tmp = join(OUT, "_test_runner.js");
    writeFileSync(tmp, script);
    const result = execSync(`node ${tmp}`, { cwd: OUT, stdio: "pipe" });
    return result.toString().trim();
}

// ─── Build output exists ────────────────────────

test("compiles all files", () => {
    expect(existsSync(join(OUT, "src/types.js"))).toBe(true);
    expect(existsSync(join(OUT, "src/handlers.js"))).toBe(true);
    expect(existsSync(join(OUT, "src/main.js"))).toBe(true);
});

test("generates d.ts files", () => {
    expect(existsSync(join(OUT, "src/types.d.ts"))).toBe(true);
    expect(existsSync(join(OUT, "src/handlers.d.ts"))).toBe(true);
});

test("generates package.json", () => {
    const pkg = JSON.parse(readFileSync(join(OUT, "package.json"), "utf8"));
    expect(pkg.name).toBe("test-api");
});

// ─── Cross-file imports work ────────────────────

test("types.js exports User and ApiResponse", () => {
    const js = readFileSync(join(OUT, "src/types.js"), "utf8");
    expect(js).toContain("class User");
    expect(js).toContain("class ApiResponse");
});

test("handlers.js imports from types.js", () => {
    const js = readFileSync(join(OUT, "src/handlers.js"), "utf8");
    expect(js).toContain('from "./types.js"');
});

test("main.js imports from both", () => {
    const js = readFileSync(join(OUT, "src/main.js"), "utf8");
    expect(js).toContain('from "./types.js"');
    expect(js).toContain('from "./handlers.js"');
});

// ─── JS output actually runs ────────────────────

test("User.create works at runtime", () => {
    const output = runJS(`
        import { User } from "./src/types.js";
        const u = new User({ name: "cam", age: 30 });
        console.log(u.name + ":" + u.age);
    `);
    expect(output).toBe("cam:30");
});

test("ApiResponse.success works at runtime", () => {
    const output = runJS(`
        import { ApiResponse } from "./src/types.js";
        const r = ApiResponse.success("hello");
        console.log(r.status + ":" + r.body);
    `);
    expect(output).toBe("200:hello");
});

test("get_user cross-file call works", () => {
    const output = runJS(`
        import { get_user } from "./src/handlers.js";
        const r = get_user("cam");
        console.log(r.status + ":" + r.body);
    `);
    expect(output).toBe("200:cam");
});

test("handle routes correctly", () => {
    const output = runJS(`
        import { handle } from "./src/main.js";
        const r1 = handle("GET", "/users");
        const r2 = handle("DELETE", "/users");
        console.log(r1.status + ":" + r1.body);
        console.log(r2.status + ":" + r2.body);
    `);
    expect(output).toBe("200:default\n400:not found");
});

test("validate_name rejects short names", () => {
    const output = runJS(`
        import { validate_name } from "./src/handlers.js";
        const r1 = validate_name("cam");
        const r2 = validate_name("x");
        console.log(r1.status + ":" + r1.body);
        console.log(r2.status + ":" + r2.body);
    `);
    expect(output).toBe("200:cam\n400:name too short");
});
