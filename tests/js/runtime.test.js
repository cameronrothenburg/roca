// Layer 1: Does @rocalang/runtime work?
import { test, expect } from "bun:test";
import roca, { wrap, ok, error } from "@rocalang/runtime";

// ─── wrap utility ─────────────────────────────────

test("wrap: sync success", () => {
    const add = wrap((a, b) => a + b);
    expect(add(2, 3)).toEqual({ value: 5, err: null });
});

test("wrap: sync throw", () => {
    const fail = wrap(() => { throw new Error("boom"); });
    expect(fail().err.name).toBe("Error");
    expect(fail().err.message).toBe("boom");
});

test("wrap: async success", async () => {
    const fn = wrap(async () => "ok");
    expect(await fn()).toEqual({ value: "ok", err: null });
});

test("wrap: async throw", async () => {
    const fn = wrap(async () => { throw new Error("async boom"); });
    const result = await fn();
    expect(result.err.name).toBe("Error");
});

// ─── Math ─────────────────────────────────────────

test("Math.floor", () => expect(roca.Math.floor(3.7)).toBe(3));
test("Math.ceil", () => expect(roca.Math.ceil(3.2)).toBe(4));
test("Math.abs", () => expect(roca.Math.abs(-5)).toBe(5));
test("Math.pow", () => expect(roca.Math.pow(2, 10)).toBe(1024));
test("Math.min/max", () => {
    expect(roca.Math.min(3, 5)).toBe(3);
    expect(roca.Math.max(3, 5)).toBe(5);
});

// ─── Char ─────────────────────────────────────────

test("Char.isDigit", () => {
    expect(roca.Char.isDigit("5")).toBe(true);
    expect(roca.Char.isDigit("a")).toBe(false);
});
test("Char.isLetter", () => expect(roca.Char.isLetter("Z")).toBe(true));
test("Char.fromCode", () => expect(roca.Char.fromCode(65)).toBe("A"));

// ─── Path ─────────────────────────────────────────

test("Path.join", () => expect(roca.Path.join("/usr", "bin")).toBe("/usr/bin"));
test("Path.extension", () => expect(roca.Path.extension("file.txt")).toBe(".txt"));
test("Path.basename", () => expect(roca.Path.basename("/usr/bin/roca")).toBe("roca"));
test("Path.dirname", () => expect(roca.Path.dirname("/usr/bin/roca")).toBe("/usr/bin"));

// ─── NumberParse ──────────────────────────────────

test("NumberParse.parse valid", () => {
    expect(roca.NumberParse.parse("42")).toEqual({ value: 42, err: null });
});
test("NumberParse.parse invalid", () => {
    expect(roca.NumberParse.parse("abc").err.name).toBe("invalid");
});

// ─── JSON ─────────────────────────────────────────

test("JSON.parse valid", () => {
    const result = roca.JSON.parse('{"a":1}');
    expect(result.err).toBeNull();
});
test("JSON.parse invalid", () => {
    expect(roca.JSON.parse("bad").err.name).toBe("parse_failed");
});

// ─── Encoding ─────────────────────────────────────

test("Encoding.btoa", () => {
    expect(roca.Encoding.btoa("hello")).toEqual({ value: "aGVsbG8=", err: null });
});
test("Encoding.atob", () => {
    expect(roca.Encoding.atob("aGVsbG8=")).toEqual({ value: "hello", err: null });
});

// ─── Map ──────────────────────────────────────────

test("Map lifecycle", () => {
    const m = roca.Map.new();
    roca.Map.set(m, "key", "val");
    expect(roca.Map.get(m, "key")).toEqual({ value: "val", err: null });
    expect(roca.Map.get(m, "nope").err.name).toBe("not_found");
    expect(roca.Map.has(m, "key")).toBe(true);
    expect(roca.Map.size(m)).toBe(1);
});

// ─── Time ─────────────────────────────────────────

test("Time.now", () => expect(roca.Time.now()).toBeGreaterThan(0));
test("Time.parse valid", () => {
    expect(roca.Time.parse("2026-01-01").err).toBeNull();
});
test("Time.parse invalid", () => {
    expect(roca.Time.parse("not a date").err.name).toBe("parse_failed");
});

// ─── Url ──────────────────────────────────────────

test("Url.parse valid", () => {
    const r = roca.Url.parse("https://example.com/path");
    expect(r.err).toBeNull();
    expect(r.value.hostname()).toBe("example.com");
    expect(r.value.pathname()).toBe("/path");
    expect(r.value.protocol()).toBe("https:");
});
test("Url.parse invalid", () => {
    expect(roca.Url.parse("not a url").err).not.toBeNull();
});
test("Url.isValid", () => {
    expect(roca.Url.isValid("https://example.com")).toBe(true);
    expect(roca.Url.isValid("nope")).toBe(false);
});

// ─── Crypto ───────────────────────────────────────

test("Crypto.randomUUID", () => {
    const id = roca.Crypto.randomUUID();
    expect(typeof id).toBe("string");
    expect(id.length).toBe(36);
});

// ─── Fs (Node/Bun) ────────────────────────────────

test("Fs.readFile missing", () => {
    const r = roca.Fs.readFile("/nonexistent");
    expect(r.err).not.toBeNull();
});
test("Fs.exists", () => {
    const thisFile = new URL(import.meta.url).pathname;
    expect(roca.Fs.exists(thisFile)).toBe(true);
    expect(roca.Fs.exists("/nope")).toBe(false);
});

// ─── Process ──────────────────────────────────────

test("Process.cwd", () => {
    expect(roca.Process.cwd().length).toBeGreaterThan(0);
});
test("Process.env missing", () => {
    expect(roca.Process.env("ROCA_NONEXISTENT_VAR_12345").err.name).toBe("notSet");
});
