/**
 * URL and URLSearchParams bridge for bare V8.
 *
 * URL parsing is backed by a Rust op (op_url_parse) which uses the
 * `url` crate for WHATWG-compliant parsing. This bridge exposes
 * the parsed result as a standard URL class on globalThis.
 */

if (typeof URLSearchParams === "undefined") {
  globalThis.URLSearchParams = class URLSearchParams {
    constructor(init) {
      this._params = [];
      if (typeof init === "string") {
        const s = init.startsWith("?") ? init.slice(1) : init;
        if (s) {
          s.split("&").forEach((p) => {
            const [k, ...v] = p.split("=");
            this._params.push([
              decodeURIComponent(k),
              decodeURIComponent(v.join("=")),
            ]);
          });
        }
      }
    }
    get(name) {
      const e = this._params.find((p) => p[0] === name);
      return e ? e[1] : null;
    }
    has(name) {
      return this._params.some((p) => p[0] === name);
    }
    get size() {
      return this._params.length;
    }
    toString() {
      return this._params
        .map(
          ([k, v]) => encodeURIComponent(k) + "=" + encodeURIComponent(v)
        )
        .join("&");
    }
  };
}

if (typeof URL === "undefined") {
  globalThis.URL = class URL {
    constructor(raw) {
      const json = Deno.core.ops.op_url_parse(raw);
      if (!json) throw new TypeError("Invalid URL: " + raw);
      const p = JSON.parse(json);
      this.href = p.href;
      this.origin = p.origin;
      this.protocol = p.protocol;
      this.hostname = p.hostname;
      this.host = p.host;
      this.port = p.port;
      this.pathname = p.pathname;
      this.search = p.search;
      this.hash = p.hash;
      this.searchParams = new URLSearchParams(this.search);
    }
    toString() {
      return this.href;
    }
  };
}
