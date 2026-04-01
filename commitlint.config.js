export default {
    extends: ["@commitlint/config-conventional"],
    rules: {
        "scope-enum": [2, "always", [
            "compiler", "runtime", "native", "checker", "emitter",
            "cli", "spec", "ci", "js", "deps",
        ]],
        "scope-empty": [2, "never"],
    },
};
