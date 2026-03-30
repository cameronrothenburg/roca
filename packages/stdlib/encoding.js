const encoding = (() => {
	const _encoder = new TextEncoder();
	const _decoder = new TextDecoder("utf-8", { fatal: true });
	const _btoa = globalThis.btoa;
	const _atob = globalThis.atob;

	function encode(input) {
		return _encoder.encode(input);
	}

	function decode(bytes) {
		try { return { value: _decoder.decode(bytes), err: null }; }
		catch (e) { return { value: null, err: { name: "invalid", message: e.message } }; }
	}

	function btoa(input) {
		try { return { value: _btoa(input), err: null }; }
		catch (e) { return { value: null, err: { name: "invalid", message: e.message } }; }
	}

	function atob(input) {
		try { return { value: _atob(input), err: null }; }
		catch (e) { return { value: null, err: { name: "invalid", message: e.message } }; }
	}

	return { encode, decode, btoa, atob };
})();
