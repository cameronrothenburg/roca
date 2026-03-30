/**
 * roca-test — Roca's built-in property-based test runner.
 *
 * Provides random input generation and a battleTest runner for
 * stress-testing Roca functions with adversarial inputs.
 *
 * Exports: fc, battleTest, arb
 *
 * This file is the readable source. The minified version (roca-test.js)
 * is embedded into the compiler binary.
 */

// ─── Pseudorandom number generator (xorshift32) ────────────────

function createRng(seed) {
  let state = seed | 0 || 1;
  return function next() {
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    return state;
  };
}

// ─── Arbitraries — random value generators ─────────────────────

function stringArb() {
  return { generate: (rng) => {
    const len = Math.abs(rng()) % 64;
    const chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 !@#$%^&*()-_=+[]{}|;:',.<>?/`~";
    let s = "";
    for (let i = 0; i < len; i++) {
      s += chars[Math.abs(rng()) % chars.length];
    }
    return s;
  }};
}

function numberArb() {
  return { generate: (rng) => {
    const special = [0, -0, 1, -1, 0.5, -0.5, Number.MAX_SAFE_INTEGER, Number.MIN_SAFE_INTEGER, 1e-10, 1e10];
    const r = Math.abs(rng()) % 100;
    if (r < 20) return special[Math.abs(rng()) % special.length];
    return (rng() / 2147483647) * 1e6;
  }};
}

function integerArb() {
  return { generate: (rng) => rng() };
}

function boolArb() {
  return { generate: (rng) => (rng() & 1) === 0 };
}

function arrayArb() {
  return { generate: (rng) => {
    const len = Math.abs(rng()) % 10;
    const arr = [];
    for (let i = 0; i < len; i++) arr.push(rng());
    return arr;
  }};
}

function constantArb(value) {
  return { generate: () => value };
}

function recordArb(shape) {
  const keys = Object.keys(shape);
  return {
    generate: (rng) => {
      const obj = {};
      for (const key of keys) {
        obj[key] = shape[key].generate(rng);
      }
      return obj;
    },
    map: (fn) => {
      const inner = recordArb(shape);
      return { generate: (rng) => fn(inner.generate(rng)) };
    },
  };
}

// ─── fc — fast-check compatible API surface ────────────────────

const fc = {
  constant: constantArb,
  record: recordArb,
};

// ─── arb — shorthand arbitrary constructors ────────────────────

const arb = {
  String: stringArb,
  Number: numberArb,
  Integer: integerArb,
  Bool: boolArb,
  Array: arrayArb,
};

// ─── battleTest — property-based stress testing ────────────────

/**
 * Run a function with random inputs and verify it never throws
 * and only returns allowed errors.
 *
 * @param {Function} fn - The function to test
 * @param {Array} arbitraries - Array of arbitrary generators, one per param
 * @param {Array} allowedErrors - Array of allowed error name strings
 * @param {number} numRuns - Number of random inputs to try
 * @returns {{ passed: number, failed: number }}
 */
function battleTest(fn, arbitraries, allowedErrors, numRuns) {
  let passed = 0;
  let failed = 0;
  const rng = createRng(Date.now() ^ 0xdeadbeef);

  for (let i = 0; i < numRuns; i++) {
    const args = arbitraries.map((a) => a.generate(rng));

    try {
      const result = fn(...args);

      // Check {value, err} protocol
      if (result !== null && typeof result === "object" && "err" in result) {
        if (result.err !== null && result.err !== undefined) {
          const errName = result.err.name;
          if (allowedErrors.length > 0 && !allowedErrors.includes(errName)) {
            console.log(
              "BATTLE FAIL: " + fn.name + " returned undeclared error: " + errName,
              "with args:", JSON.stringify(args)
            );
            failed++;
            continue;
          }
        }
      }

      passed++;
    } catch (e) {
      console.log(
        "BATTLE FAIL: " + fn.name + " threw: " + e.message,
        "with args:", JSON.stringify(args)
      );
      failed++;
    }
  }

  return { passed, failed };
}

// ─── Export ─────────────────────────────────────────────────────

module.exports = { fc, battleTest, arb };
