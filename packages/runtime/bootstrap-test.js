/**
 * Bootstrap for `roca build` proof tests — captures console.log output
 * via op_capture_log for the Rust harness to parse test results.
 * console.error/warn go to stderr directly.
 */

globalThis.console = {
  log: (...args) => {
    const msg = args.map(a => typeof a === "string" ? a : JSON.stringify(a)).join(" ");
    Deno.core.ops.op_capture_log(msg);
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
