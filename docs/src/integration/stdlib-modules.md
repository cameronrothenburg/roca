# Stdlib Modules

Standard library modules are imported with `std::` syntax. The JS runtime wrappers are inlined automatically -- no separate runtime file is emitted.

## Import syntax

```roca
import { JSON } from std::json
```

## Usage

```roca
/// Parses a JSON config string, returning empty object on failure
pub fn parse_config(raw: String) -> String {
    const data = JSON.parse(raw)
    return data
    crash {
        JSON.parse -> fallback("{}")
    }
    test { self("invalid") == "{}" }
}
```

Stdlib module methods that can fail require crash block entries. Safe methods (like those on `String`, `Array`, etc.) do not.

## Inline compilation

When you import from `std::json`, the compiler inlines a thin JS wrapper at the call site. There is no separate runtime file or dependency to install. The compiled output is self-contained.
