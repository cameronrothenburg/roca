const Url = (() => {
	const _URL = globalThis.URL;

	function wrap(u) {
		return {
			href() { return u.href; },
			origin() { return u.origin; },
			protocol() { return u.protocol; },
			hostname() { return u.hostname; },
			host() { return u.host; },
			port() { return u.port; },
			pathname() { return u.pathname; },
			search() { return u.search; },
			hash() { return u.hash; },
			getParam(name) { return u.searchParams.get(name); },
			hasParam(name) { return u.searchParams.has(name); },
			toString() { return u.href; },
		};
	}

	return {
		parse(raw) {
			try { return { value: wrap(new _URL(raw)), err: null }; }
			catch (e) { return { value: null, err: { name: "parse_failed", message: e.message } }; }
		},
		isValid(raw) {
			try { new _URL(raw); return true; }
			catch { return false; }
		},
	};
})();
