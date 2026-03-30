const Http = (() => {
	const _fetch = globalThis.fetch;

	function wrapResponse(resp) {
		let _bodyUsed = false;
		return {
			status() { return resp.status; },
			ok() { return resp.ok; },
			async text() {
				if (_bodyUsed) return { value: null, err: { name: "consumed", message: "body already consumed" } };
				_bodyUsed = true;
				try { return { value: await resp.text(), err: null }; }
				catch (e) { return { value: null, err: { name: "consumed", message: e.message } }; }
			},
			async json() {
				if (_bodyUsed) return { value: null, err: { name: "consumed", message: "body already consumed" } };
				_bodyUsed = true;
				try {
					const text = await resp.text();
					const parsed = JSON.parse(text);
					return { value: parsed, err: null };
				} catch (e) {
					const name = e instanceof SyntaxError ? "parse" : "consumed";
					return { value: null, err: { name, message: e.message } };
				}
			},
			header(name) { return resp.headers.get(name); },
		};
	}

	return {
		async fetch(url) {
			try {
				const resp = await _fetch(url);
				return { value: wrapResponse(resp), err: null };
			} catch (e) {
				const name = e.name === "AbortError" ? "abort" : "network";
				return { value: null, err: { name, message: e.message } };
			}
		},
		async post(url, body) {
			try {
				const resp = await _fetch(url, { method: "POST", body });
				return { value: wrapResponse(resp), err: null };
			} catch (e) {
				const name = e.name === "AbortError" ? "abort" : "network";
				return { value: null, err: { name, message: e.message } };
			}
		},
	};
})();
