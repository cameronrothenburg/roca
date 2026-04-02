---
name: tests-reveal-bugs
enabled: true
event: file
conditions:
  - field: file_path
    operator: regex_match
    pattern: test
  - field: old_text
    operator: regex_match
    pattern: assert_(eq|ne|matches)
action: warn
---

⚠️ **You are modifying a test assertion.**

Before changing a test, ask yourself:
- Is the test wrong, or is the code wrong?
- If the test is revealing a bug, fix the **code under test** instead
- If the test is genuinely incorrect, explain to the user **why** before editing

Red tests are a gift — they show you exactly where the bug is.
