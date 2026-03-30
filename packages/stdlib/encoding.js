const Encoding = (() => {
	const _encoder = new TextEncoder();
	const _decoder = new TextDecoder("utf-8", { fatal: true });
	const _btoa = globalThis.btoa;
	const _atob = globalThis.atob;

	return {
		encode(input) {
			return _encoder.encode(input);
		},
		decode(bytes) {
			try { return { value: _decoder.decode(bytes), err: null }; }
			catch (e) { return { value: null, err: { name: "invalid", message: e.message } }; }
		},
		btoa(input) {
			try { return { value: _btoa(input), err: null }; }
			catch (e) { return { value: null, err: { name: "invalid", message: e.message } }; }
		},
		atob(input) {
			try { return { value: _atob(input), err: null }; }
			catch (e) { return { value: null, err: { name: "invalid", message: e.message } }; }
		},
	};
})();
