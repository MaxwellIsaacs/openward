# Phase 5: CI Key Validation Test

## Prerequisites

All previous phases must be complete. All templates use `t.get()` calls, all .ftl files exist, all handlers pass `t` to templates.

Read `specs/i18n.md` section 9 for the validation requirements.

## Task: Write a CI key validation integration test

Create `crates/openward-server/tests/i18n_validation.rs` (or add to an existing test file).

The test should verify:

### 1. Every `t.get("...")` call references a valid key

Scan all `.html` template files in `crates/openward-server/templates/` and all `.rs` files in `crates/openward-server/src/web/` for patterns:
- `t.get("([^"]+)")`
- `t.get_1("([^"]+)"`
- `t.get_with("([^"]+)"`

Collect all referenced keys.

### 2. Every key in English .ftl files exists

Parse `locales/en/ui.ftl` and `locales/en/legal.ftl` to extract all defined message IDs (lines matching `^[a-z][-a-z0-9]+ = `).

### 3. Cross-check: all referenced keys exist in English .ftl

Every key from step 1 must exist in step 2. Report any missing keys.

### 4. No dead keys

Every key from step 2 should be referenced in step 1. Report any unreferenced keys (warnings, not hard failures — some keys may be used programmatically via constructed key names).

### 5. All languages are complete

Every key in `locales/en/ui.ftl` must have a corresponding entry in `locales/fr/ui.ftl` and `locales/mg/ui.ftl`. Same for `legal.ftl`.

### Implementation

```rust
use std::collections::HashSet;
use std::fs;
use regex::Regex;

#[test]
fn validate_i18n_keys() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");

    // 1. Collect all keys referenced in templates and handlers
    let mut referenced_keys = HashSet::new();
    let key_pattern = Regex::new(r#"t\.get(?:_1|_with)?\("([^"]+)""#).unwrap();

    // Scan templates
    for entry in walkdir(format!("{}/templates", manifest_dir)) {
        if entry.ends_with(".html") {
            let content = fs::read_to_string(entry).unwrap();
            for cap in key_pattern.captures_iter(&content) {
                referenced_keys.insert(cap[1].to_string());
            }
        }
    }

    // Scan handler .rs files
    for entry in walkdir(format!("{}/src/web", manifest_dir)) {
        if entry.ends_with(".rs") {
            let content = fs::read_to_string(entry).unwrap();
            for cap in key_pattern.captures_iter(&content) {
                referenced_keys.insert(cap[1].to_string());
            }
        }
    }

    // Also scan src/i18n.rs and src/templates.rs
    // ...

    // 2. Collect all defined keys from English .ftl files
    let mut defined_keys = HashSet::new();
    let ftl_pattern = Regex::new(r"^([a-z][-a-z0-9]+)\s*=").unwrap();

    for ftl_file in &["locales/en/ui.ftl", "locales/en/legal.ftl"] {
        let content = fs::read_to_string(format!("{}/{}", manifest_dir, ftl_file)).unwrap();
        for line in content.lines() {
            if let Some(cap) = ftl_pattern.captures(line) {
                defined_keys.insert(cap[1].to_string());
            }
        }
    }

    // 3. Check referenced keys exist
    let missing: Vec<_> = referenced_keys.difference(&defined_keys).collect();
    assert!(missing.is_empty(), "Keys referenced but not defined: {:?}", missing);

    // 4. Check for dead keys (warning only)
    let unused: Vec<_> = defined_keys.difference(&referenced_keys).collect();
    if !unused.is_empty() {
        eprintln!("WARNING: Keys defined but not referenced: {:?}", unused);
    }

    // 5. Check all languages have all keys
    for lang in &["fr", "mg"] {
        for ftl_name in &["ui.ftl", "legal.ftl"] {
            let content = fs::read_to_string(
                format!("{}/locales/{}/{}", manifest_dir, lang, ftl_name)
            ).unwrap();
            let lang_keys: HashSet<String> = content.lines()
                .filter_map(|line| ftl_pattern.captures(line))
                .map(|cap| cap[1].to_string())
                .collect();

            let en_ftl = if *ftl_name == "ui.ftl" { "locales/en/ui.ftl" } else { "locales/en/legal.ftl" };
            let en_content = fs::read_to_string(format!("{}/{}", manifest_dir, en_ftl)).unwrap();
            let en_keys: HashSet<String> = en_content.lines()
                .filter_map(|line| ftl_pattern.captures(line))
                .map(|cap| cap[1].to_string())
                .collect();

            let missing_in_lang: Vec<_> = en_keys.difference(&lang_keys).collect();
            assert!(
                missing_in_lang.is_empty(),
                "Keys missing in {}/{}: {:?}", lang, ftl_name, missing_in_lang
            );
        }
    }
}
```

You'll need to add `regex` and `walkdir` as dev-dependencies:
```toml
[dev-dependencies]
regex = "1"
walkdir = "2"
```

Or use `std::fs::read_dir` recursively instead of `walkdir`.

## Verify

Run `cargo test -p openward-server -- i18n_validation` to verify the test passes.
