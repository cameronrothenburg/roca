const Time = (() => {
	const _Date = globalThis.Date;

	return {
		now() {
			return _Date.now();
		},
		parse(input) {
			const d = new _Date(input);
			if (isNaN(d.getTime())) {
				return { value: null, err: { name: "parse_failed", message: "invalid date string" } };
			}
			return { value: d.getTime(), err: null };
		},
	};
})();
