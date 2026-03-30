/**
 * Bootstrap for the embedded V8 runtime.
 *
 * Provides console.log/error/warn and process.exit.
 *
 * When __ROCA_CAPTURE_MODE__ is true (set before loading),
 * console.log routes to op_capture_log for test result parsing.
 * Otherwise it prints directly to stdout.
 */

const __fmt = (...args) =>
  args.map(a => typeof a === "string" ? a : JSON.stringify(a)).join(" ");

globalThis.console = {
  log: (...args) => {
    const msg = __fmt(...args);
    if (typeof __ROCA_CAPTURE_MODE__ !== "undefined" && __ROCA_CAPTURE_MODE__) {
      Deno.core.ops.op_capture_log(msg);
    } else {
      Deno.core.print(msg + "\n", false);
    }
  },
  error: (...args) => {
    Deno.core.print(__fmt(...args) + "\n", true);
  },
  warn: (...args) => {
    Deno.core.print(__fmt(...args) + "\n", true);
  },
};

globalThis.process = {
  exit: (code) => {
    if (code !== 0) throw new Error("__PROCESS_EXIT__");
  },
};
