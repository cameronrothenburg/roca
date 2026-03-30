// Roca syntax highlighting for highlight.js (used by mdbook)
document.addEventListener("DOMContentLoaded", function() {
  if (typeof hljs === "undefined") return;

  hljs.registerLanguage("roca", function(hljs) {
    return {
      name: "Roca",
      keywords: {
        keyword: "pub fn contract struct satisfies extern enum import from std if else for in while break continue return match wait waitAll waitFirst const let crash test mock err halt fallback retry skip panic log error warn",
        type: "String Number Bool Array Map Bytes Buffer Ok Optional Loggable",
        literal: "true false null self"
      },
      contains: [
        // Doc comments /** */
        hljs.COMMENT("/\\*\\*", "\\*/", { relevance: 10, className: "comment doctag" }),
        // Block comments /* */
        hljs.COMMENT("/\\*", "\\*/"),
        // Doc comments ///
        { className: "comment doctag", begin: "///", end: "$", relevance: 10 },
        // Line comments
        hljs.COMMENT("//", "$"),
        // Strings
        hljs.QUOTE_STRING_MODE,
        // Numbers
        hljs.C_NUMBER_MODE,
        // Error declarations
        { className: "string", begin: 'err\\s+\\w+\\s*=\\s*"', end: '"' },
        // Type annotations after colon
        { className: "type", begin: "->\\s*", end: "[,\\s{]", excludeEnd: true },
        // Generic types
        { className: "type", begin: "\\b[A-Z]\\w*<", end: ">", contains: [{ className: "type", begin: "\\b[A-Z]\\w*" }] },
        // Type names (capitalized)
        { className: "type", begin: "\\b[A-Z]\\w*\\b" },
        // Crash strategies
        { className: "built_in", begin: "\\b(halt|fallback|retry|skip|panic|log)\\b" },
        // Function definitions
        { className: "function", beginKeywords: "fn", end: "\\(", excludeEnd: true, contains: [hljs.UNDERSCORE_TITLE_MODE] },
      ]
    };
  });

  // Re-highlight all code blocks marked as roca
  document.querySelectorAll("pre code.language-roca").forEach(function(block) {
    hljs.highlightElement(block);
  });
});
