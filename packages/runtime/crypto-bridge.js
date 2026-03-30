/**
 * Crypto bridge for bare V8.
 *
 * Provides globalThis.crypto with randomUUID and subtle.digest
 * backed by Rust ops (sha2 + uuid crates).
 */

if (typeof crypto === "undefined" || !crypto.randomUUID) {
  const _sha256 = (data) => Deno.core.ops.op_sha256(data);
  const _sha512 = (data) => Deno.core.ops.op_sha512(data);
  const _randomUUID = () => Deno.core.ops.op_random_uuid();

  globalThis.crypto = {
    randomUUID: _randomUUID,
    subtle: {
      async digest(algorithm, data) {
        const algo = typeof algorithm === "string" ? algorithm : algorithm.name;
        const str = typeof data === "string" ? data : new TextDecoder().decode(data);
        let hex;
        switch (algo) {
          case "SHA-256": hex = _sha256(str); break;
          case "SHA-512": hex = _sha512(str); break;
          default: throw new DOMException("Unsupported algorithm: " + algo, "NotSupportedError");
        }
        // Convert hex string to ArrayBuffer (matches Web Crypto API)
        const bytes = new Uint8Array(hex.length / 2);
        for (let i = 0; i < hex.length; i += 2) {
          bytes[i / 2] = parseInt(hex.substr(i, 2), 16);
        }
        return bytes.buffer;
      },
    },
    getRandomValues(arr) {
      for (let i = 0; i < arr.length; i++) {
        arr[i] = Math.floor(Math.random() * 256);
      }
      return arr;
    },
  };
}
