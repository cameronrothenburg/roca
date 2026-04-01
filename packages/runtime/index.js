// @rocalang/runtime — stdlib implementations for compiled Roca programs.
// Single default export: roca.Math.floor(), roca.Fs.readFile(), etc.

// ─── Error protocol helpers ─────────────────────────

function ok(value) {
    return { value, err: null };
}

function error(name, message) {
    return { value: null, err: { name, message } };
}

/**
 * Wrap a plain JS function into Roca's { value, err } protocol.
 * Catches exceptions and converts them to error tuples.
 * Supports async functions automatically.
 */
export function wrap(fn) {
    return function (...args) {
        try {
            const result = fn(...args);
            if (result && typeof result.then === "function") {
                return result.then(v => ok(v)).catch(e => error(e.name || "Error", e.message || String(e)));
            }
            return ok(result);
        } catch (e) {
            return error(e.name || "Error", e.message || String(e));
        }
    };
}

function notAvailable(contract, method) {
    return function () {
        return error("platform", `${contract}.${method} is not available in this environment`);
    };
}

// ─── Platform detection ─────────────────────────────

const isNode = typeof globalThis.process !== "undefined"
    && typeof globalThis.process.versions !== "undefined"
    && typeof globalThis.process.versions.node !== "undefined";

// Shared text codec instances (used by Encoding + Crypto)
const _te = typeof TextEncoder !== "undefined" ? new TextEncoder() : null;
const _td = typeof TextDecoder !== "undefined" ? new TextDecoder() : null;

// ─── Core (pure JS — works everywhere) ─────────────

const Math = {
    floor: (n) => globalThis.Math.floor(n),
    ceil: (n) => globalThis.Math.ceil(n),
    round: (n) => globalThis.Math.round(n),
    abs: (n) => globalThis.Math.abs(n),
    sqrt: (n) => globalThis.Math.sqrt(n),
    pow: (base, exp) => globalThis.Math.pow(base, exp),
    min: (a, b) => globalThis.Math.min(a, b),
    max: (a, b) => globalThis.Math.max(a, b),
    random: () => globalThis.Math.random(),
};

const Char = {
    fromCode: (code) => String.fromCharCode(code),
    isDigit: (ch) => ch >= "0" && ch <= "9",
    isLetter: (ch) => (ch >= "a" && ch <= "z") || (ch >= "A" && ch <= "Z"),
    isWhitespace: (ch) => ch === " " || ch === "\t" || ch === "\n" || ch === "\r",
    isAlphanumeric: (ch) => Char.isLetter(ch) || Char.isDigit(ch),
};

const NumberParse = {
    parse: (s) => {
        const n = Number(s);
        if (isNaN(n) && s.trim() !== "NaN") {
            return error("invalid", "not a valid number");
        }
        return ok(n);
    },
};

const _sep = isNode && globalThis.process.platform === "win32" ? "\\" : "/";
const Path = {
    join: (base, seg) => base + _sep + seg,
    dirname: (p) => { const i = p.lastIndexOf(_sep); return i < 0 ? "." : p.substring(0, i) || _sep; },
    basename: (p) => { const i = p.lastIndexOf(_sep); return i < 0 ? p : p.substring(i + 1); },
    extension: (p) => { const i = p.lastIndexOf("."); return i < 1 ? "" : p.substring(i); },
    isAbsolute: (p) => p.startsWith("/") || /^[A-Z]:\\/i.test(p),
    normalize: (p) => p.replace(/\/+/g, "/"),
};

const Map = {
    new: () => new globalThis.Map(),
    get: (map, key) => {
        if (!map.has(key)) return error("not_found", "key not found");
        return ok(map.get(key));
    },
    set: (map, key, value) => { map.set(key, value); return map; },
    has: (map, key) => map.has(key),
    delete: (map, key) => map.delete(key),
    keys: (map) => [...map.keys()],
    values: (map) => [...map.values()],
    size: (map) => map.size,
};

// ─── Data ───────────────────────────────────────────

const JSON = (() => {
    const _JSON = globalThis.JSON;
    const SYM = Symbol("roca_raw");
    function wrapJson(value) {
        return {
            [SYM]: value,
            get(key) { return value != null && typeof value === "object" && key in value ? wrapJson(value[key]) : null; },
            getString(key) { const v = value?.[key]; return typeof v === "string" ? v : null; },
            getNumber(key) { const v = value?.[key]; return typeof v === "number" ? v : null; },
            getBool(key) { const v = value?.[key]; return typeof v === "boolean" ? v : null; },
            getArray(key) { const v = value?.[key]; return Array.isArray(v) ? v.map(wrapJson) : null; },
            toString() { return _JSON.stringify(value); },
        };
    }
    return {
        parse: (text) => {
            try { return ok(wrapJson(_JSON.parse(text))); }
            catch (e) { return error("parse_failed", e.message); }
        },
        stringify: (jsonVal) => _JSON.stringify(jsonVal?.[SYM] ?? jsonVal),
    };
})();

const Encoding = (() => {
    return {
        encode: (input) => _te ? _te.encode(input) : new Uint8Array([...input].map(c => c.charCodeAt(0))),
        decode: wrap((bytes) => _td ? _td.decode(bytes) : String.fromCharCode(...bytes)),
        btoa: wrap((input) => globalThis.btoa(input)),
        atob: wrap((input) => globalThis.atob(input)),
    };
})();

// ─── Net ────────────────────────────────────────────

const Http = (() => {
    const _fetch = typeof globalThis.fetch === "function" ? globalThis.fetch.bind(globalThis) : null;
    const doFetch = wrap(async (url, opts) => {
        if (!_fetch) throw Object.assign(new Error("Http is not available in this environment"), { name: "platform" });
        return await _fetch(url, opts);
    });
    return {
        get: (url) => doFetch(url, { method: "GET" }),
        post: (url, body) => doFetch(url, { method: "POST", body }),
        put: (url, body) => doFetch(url, { method: "PUT", body }),
        patch: (url, body) => doFetch(url, { method: "PATCH", body }),
        delete: (url) => doFetch(url, { method: "DELETE" }),
        status: (res) => res?.status ?? 0,
        ok: (res) => res?.ok ?? false,
        text: wrap(async (res) => await res.text()),
        json: wrap(async (res) => await res.json()),
        header: (res, name) => res?.headers?.get(name) ?? null,
    };
})();

const Url = (() => {
    return {
        parse: wrap((raw) => new URL(raw)),
        host: (u) => u?.host ?? "",
        hostname: (u) => u?.hostname ?? "",
        port: (u) => u?.port ?? "",
        protocol: (u) => u?.protocol ?? "",
        pathname: (u) => u?.pathname ?? "",
        search: (u) => u?.search ?? "",
        hash: (u) => u?.hash ?? "",
        origin: (u) => u?.origin ?? "",
        toString: (u) => u?.href ?? "",
        getParam: (u, name) => u?.searchParams?.get(name) ?? null,
        hasParam: (u, name) => u?.searchParams?.has(name) ?? false,
        isValid: (raw) => { try { new URL(raw); return true; } catch { return false; } },
    };
})();

// ─── Security ───────────────────────────────────────

const Crypto = (() => {
    const _crypto = globalThis.crypto;
    async function digest(algo, data) {
        if (!_crypto?.subtle) throw Object.assign(new Error("crypto.subtle is not available"), { name: "platform" });
        const buf = await _crypto.subtle.digest(algo, _te.encode(data));
        return [...new Uint8Array(buf)].map(b => b.toString(16).padStart(2, "0")).join("");
    }
    return {
        randomUUID: () => {
            if (!_crypto?.randomUUID) return error("platform", "crypto.randomUUID is not available");
            return ok(_crypto.randomUUID());
        },
        sha256: wrap(async (data) => await digest("SHA-256", data)),
        sha512: wrap(async (data) => await digest("SHA-512", data)),
    };
})();

// ─── Time ───────────────────────────────────────────

const Time = {
    now: () => Date.now(),
    parse: (input) => {
        const ms = Date.parse(input);
        if (isNaN(ms)) return error("parse_failed", "invalid date string");
        return ok(ms);
    },
};

// ─── IO (Node/Bun only) ────────────────────────────

let Fs;
let Process;

if (isNode) {
    try {
        const _fs = await import("node:fs");
        Fs = {
            readFile: wrap((path) => _fs.readFileSync(path, "utf8")),
            writeFile: wrap((path, content) => { _fs.writeFileSync(path, content, "utf8"); return null; }),
            exists: (path) => _fs.existsSync(path),
            readDir: wrap((path) => _fs.readdirSync(path).map(String)),
        };
    } catch {
        Fs = {
            readFile: notAvailable("Fs", "readFile"),
            writeFile: notAvailable("Fs", "writeFile"),
            exists: notAvailable("Fs", "exists"),
            readDir: notAvailable("Fs", "readDir"),
        };
    }

    Process = {
        args: () => globalThis.process.argv.slice(2),
        env: (key) => {
            const val = globalThis.process.env[key];
            if (val === undefined) return error("notSet", "environment variable not set");
            return ok(val);
        },
        cwd: () => globalThis.process.cwd(),
        exit: (code) => globalThis.process.exit(code),
    };
} else {
    Fs = {
        readFile: notAvailable("Fs", "readFile"),
        writeFile: notAvailable("Fs", "writeFile"),
        exists: notAvailable("Fs", "exists"),
        readDir: notAvailable("Fs", "readDir"),
    };
    Process = {
        args: notAvailable("Process", "args"),
        env: notAvailable("Process", "env"),
        cwd: notAvailable("Process", "cwd"),
        exit: notAvailable("Process", "exit"),
    };
}

// ─── Default export ─────────────────────────────────

export default {
    Math,
    Char,
    NumberParse,
    Path,
    Map,
    JSON,
    Encoding,
    Http,
    Url,
    Crypto,
    Time,
    Fs,
    Process,
};

export { ok, error };
