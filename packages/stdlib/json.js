const JSON = (() => {
	const _JSON = globalThis.JSON;

	function wrap(value) {
		return {
			_raw: value,
			get(key) { return value != null && typeof value === "object" && key in value ? wrap(value[key]) : null; },
			getString(key) { const v = value?.[key]; return typeof v === "string" ? v : null; },
			getNumber(key) { const v = value?.[key]; return typeof v === "number" ? v : null; },
			getBool(key) { const v = value?.[key]; return typeof v === "boolean" ? v : null; },
			getArray(key) { const v = value?.[key]; return Array.isArray(v) ? v.map(wrap) : null; },
			toString() { return _JSON.stringify(value); },
		};
	}

	return {
		parse(text) {
			try { return { value: wrap(_JSON.parse(text)), err: null }; }
			catch(e) { return { value: null, err: { name: "parse_failed", message: e.message } }; }
		},
		stringify(jsonVal) { return _JSON.stringify(jsonVal._raw !== undefined ? jsonVal._raw : jsonVal); },
	};
})();
