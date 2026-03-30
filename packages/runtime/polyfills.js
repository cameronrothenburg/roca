/**
 * Web API polyfills for bare V8.
 *
 * V8 doesn't include Web Platform APIs — these are normally provided by
 * the host environment (Deno, Node, browsers). We polyfill the subset
 * that Roca's compiled output may use.
 *
 * Each polyfill is guarded so it's a no-op if the host already provides it.
 */

// ─── TextEncoder / TextDecoder (UTF-8 only) ───────────────────

if (typeof TextEncoder === "undefined") {
  globalThis.TextEncoder = class TextEncoder {
    encode(str) {
      const buf = [];
      for (let i = 0; i < str.length; i++) {
        let c = str.charCodeAt(i);
        if (c < 0x80) {
          buf.push(c);
        } else if (c < 0x800) {
          buf.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
        } else {
          buf.push(
            0xe0 | (c >> 12),
            0x80 | ((c >> 6) & 0x3f),
            0x80 | (c & 0x3f)
          );
        }
      }
      return new Uint8Array(buf);
    }
  };
}

if (typeof TextDecoder === "undefined") {
  globalThis.TextDecoder = class TextDecoder {
    decode(buf) {
      const bytes = new Uint8Array(buf);
      let str = "";
      let i = 0;
      while (i < bytes.length) {
        const c = bytes[i];
        if (c < 0x80) {
          str += String.fromCharCode(c);
          i++;
        } else if (c < 0xe0) {
          str += String.fromCharCode(((c & 0x1f) << 6) | (bytes[i + 1] & 0x3f));
          i += 2;
        } else {
          str += String.fromCharCode(
            ((c & 0x0f) << 12) | ((bytes[i + 1] & 0x3f) << 6) | (bytes[i + 2] & 0x3f)
          );
          i += 3;
        }
      }
      return str;
    }
  };
}

// ─── Base64 (atob / btoa) ──────────────────────────────────────

if (typeof atob === "undefined") {
  const b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";

  globalThis.atob = (s) => {
    let r = "";
    let i = 0;
    s = s.replace(/[^A-Za-z0-9+/=]/g, "");
    while (i < s.length) {
      const a = b64.indexOf(s[i++]);
      const b = b64.indexOf(s[i++]);
      const c = b64.indexOf(s[i++]);
      const d = b64.indexOf(s[i++]);
      r += String.fromCharCode((a << 2) | (b >> 4));
      if (c !== 64) r += String.fromCharCode(((b & 15) << 4) | (c >> 2));
      if (d !== 64) r += String.fromCharCode(((c & 3) << 6) | d);
    }
    return r;
  };

  globalThis.btoa = (s) => {
    let r = "";
    let i = 0;
    while (i < s.length) {
      const a = s.charCodeAt(i++);
      const b = i < s.length ? s.charCodeAt(i++) : NaN;
      const c = i < s.length ? s.charCodeAt(i++) : NaN;
      r += b64[a >> 2] + b64[((a & 3) << 4) | (b >> 4)];
      r += isNaN(b)
        ? "=="
        : b64[((b & 15) << 2) | (c >> 6)] + (isNaN(c) ? "=" : b64[c & 63]);
    }
    return r;
  };
}

// ─── Timers ────────────────────────────────────────────────────

if (typeof setTimeout === "undefined") {
  globalThis.setTimeout = (fn, _ms) => {
    Promise.resolve().then(fn);
    return 0;
  };
  globalThis.clearTimeout = () => {};
}
