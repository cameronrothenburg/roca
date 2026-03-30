// Roca syntax highlighting for highlight.js (used by mdbook)
(function() {
  function registerRoca() {
    if (typeof hljs === "undefined") {
      setTimeout(registerRoca, 50);
      return;
    }

    hljs.registerLanguage("roca", function(hljs) {
      return {
        name: "Roca",
        keywords: {
          keyword: "pub fn contract struct satisfies extern enum import from std if else for in while break continue return match wait waitAll waitFirst const let crash test mock err halt fallback retry panic log error warn default",
          type: "String Number Bool Array Map Bytes Buffer Ok Optional Loggable",
          literal: "true false null self"
        },
        contains: [
          hljs.COMMENT("/\\*\\*", "\\*/", { relevance: 10 }),
          hljs.COMMENT("/\\*", "\\*/"),
          { className: "comment", begin: "///", end: "$", relevance: 10 },
          hljs.COMMENT("//", "$"),
          hljs.QUOTE_STRING_MODE,
          hljs.C_NUMBER_MODE,
          { className: "type", begin: "\\b[A-Z]\\w*\\b" },
          { className: "title function_", beginKeywords: "fn", end: "\\(", excludeEnd: true,
            contains: [{ className: "title function_", begin: "\\w+" }] },
        ]
      };
    });

    document.querySelectorAll("pre code.language-roca").forEach(function(block) {
      block.classList.remove("hljs");
      hljs.highlightElement(block);
    });
  }

  if (document.readyState === "complete" || document.readyState === "interactive") {
    registerRoca();
  } else {
    document.addEventListener("DOMContentLoaded", registerRoca);
  }
})();
