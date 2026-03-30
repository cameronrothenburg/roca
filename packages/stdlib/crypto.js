const Crypto = (() => {
	const _crypto = globalThis.crypto;

	function toHex(buf) {
		return [...new Uint8Array(buf)].map(b => b.toString(16).padStart(2, "0")).join("");
	}

	return {
		randomUUID() {
			return _crypto.randomUUID();
		},
		async sha256(data) {
			const buf = await _crypto.subtle.digest("SHA-256", new TextEncoder().encode(data));
			return toHex(buf);
		},
		async sha512(data) {
			const buf = await _crypto.subtle.digest("SHA-512", new TextEncoder().encode(data));
			return toHex(buf);
		},
	};
})();
