const JSON = (() => {
	const _JSON = globalThis.JSON;
	return {
		parse(text) {
			try { return { value: _JSON.parse(text), err: null }; }
			catch(e) { return { value: null, err: { name: "parse_failed", message: e.message } }; }
		},
		stringify(value) { return _JSON.stringify(value); }
	};
})();
