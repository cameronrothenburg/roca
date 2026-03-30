const Time = (() => {
	const _Date = globalThis.Date;

	return {
		now() {
			return _Date.now();
		},
		parse(input) {
			const ms = new _Date(input).getTime();
			if (isNaN(ms)) {
				return { value: null, err: { name: "parse_failed", message: "invalid date string" } };
			}
			return { value: ms, err: null };
		},
	};
})();
