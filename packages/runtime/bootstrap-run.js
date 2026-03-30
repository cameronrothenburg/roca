/**
 * Bootstrap for `roca run` — direct stdout/stderr output.
 * Provides console.log/error/warn and process.exit.
 */

globalThis.console = {
  log: (...args) => {
    const msg = args.map(a => typeof a === "string" ? a : JSON.stringify(a)).join(" ");
    Deno.core.print(msg + "\n", false);
  },
  error: (...args) => {
    const msg = args.map(a => typeof a === "string" ? a : JSON.stringify(a)).join(" ");
    Deno.core.print(msg + "\n", true);
  },
  warn: (...args) => {
    const msg = args.map(a => typeof a === "string" ? a : JSON.stringify(a)).join(" ");
    Deno.core.print(msg + "\n", true);
  },
};

globalThis.process = {
  exit: (code) => {
    if (code !== 0) throw new Error("__PROCESS_EXIT__");
  },
};
