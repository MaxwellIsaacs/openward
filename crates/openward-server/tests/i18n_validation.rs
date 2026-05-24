use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// Recursively collect all files with the given extension under `dir`.
fn collect_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_files(&path, extension));
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            files.push(path);
        }
    }
    files
}

/// Extract all FTL message IDs from a `.ftl` file's content.
fn extract_ftl_keys(content: &str) -> HashSet<String> {
    let pattern = Regex::new(r"^([a-z][-a-z0-9]+)\s*=").unwrap();
    content
        .lines()
        .filter_map(|line| pattern.captures(line))
        .map(|cap| cap[1].to_string())
        .collect()
}

#[test]
fn validate_i18n_keys() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));

    // ── 1. Collect all keys referenced in templates and Rust source ──

    let mut referenced_keys = HashSet::new();

    // Matches t.get("key"), t.get_1("key", …), t.get_with("key", …),
    // t.get_u32("key", …), t.get_2u32("key", …)
    let t_get_re = Regex::new(r#"t\.get(?:_1|_with|_u32|_2u32)?\("([^"]+)""#).unwrap();

    // Scan HTML templates
    for file in collect_files(&base.join("templates"), "html") {
        let content = fs::read_to_string(&file).unwrap();
        for cap in t_get_re.captures_iter(&content) {
            referenced_keys.insert(cap[1].to_string());
        }
    }

    // Scan Rust handler files in src/web/
    for file in collect_files(&base.join("src").join("web"), "rs") {
        let content = fs::read_to_string(&file).unwrap();
        for cap in t_get_re.captures_iter(&content) {
            referenced_keys.insert(cap[1].to_string());
        }
    }

    // Also scan top-level src/*.rs (i18n.rs, templates.rs, api.rs, main.rs)
    for file in collect_files(&base.join("src"), "rs") {
        // Only direct children of src/, not src/web/ (already scanned)
        if file.parent() == Some(&base.join("src")) {
            let content = fs::read_to_string(&file).unwrap();
            for cap in t_get_re.captures_iter(&content) {
                referenced_keys.insert(cap[1].to_string());
            }
        }
    }

    // ── 2. Collect all defined keys from English .ftl files ──

    let mut defined_keys = HashSet::new();

    for ftl_file in &["locales/en/ui.ftl", "locales/en/legal.ftl"] {
        let content = fs::read_to_string(base.join(ftl_file))
            .unwrap_or_else(|e| panic!("Cannot read {ftl_file}: {e}"));
        defined_keys.extend(extract_ftl_keys(&content));
    }

    // ── 1b. Catch keys used via helper functions (e.g. util::basis_key()) ──
    //
    // These return &'static str key names that are later passed to t.get()
    // as variables, so the t_get regex doesn't see them. We scan .rs files
    // for any string literal that matches a *defined* FTL key.

    let key_literal_re = Regex::new(r#""([a-z][a-z0-9]*(?:-[a-z0-9]+)+)""#).unwrap();

    for file in collect_files(&base.join("src"), "rs") {
        let content = fs::read_to_string(&file).unwrap();
        for cap in key_literal_re.captures_iter(&content) {
            let candidate = cap[1].to_string();
            if defined_keys.contains(&candidate) {
                referenced_keys.insert(candidate);
            }
        }
    }

    // ── 3. Cross-check: every referenced key must be defined ──

    let mut missing: Vec<_> = referenced_keys.difference(&defined_keys).cloned().collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "i18n keys referenced in code but not defined in English .ftl files:\n  {}\n",
        missing.join("\n  ")
    );

    // ── 4. Dead keys (warning only — some keys are used programmatically) ──

    let mut unused: Vec<_> = defined_keys.difference(&referenced_keys).cloned().collect();
    unused.sort();
    if !unused.is_empty() {
        eprintln!(
            "WARNING: {} i18n key(s) defined but not referenced in code:\n  {}",
            unused.len(),
            unused.join("\n  ")
        );
    }

    // ── 5. All languages are complete ──

    for lang in &["fr", "mg"] {
        for ftl_name in &["ui.ftl", "legal.ftl"] {
            let en_path = base.join(format!("locales/en/{ftl_name}"));
            let lang_path = base.join(format!("locales/{lang}/{ftl_name}"));

            let en_content = fs::read_to_string(&en_path)
                .unwrap_or_else(|e| panic!("Cannot read {}: {e}", en_path.display()));
            let lang_content = fs::read_to_string(&lang_path)
                .unwrap_or_else(|e| panic!("Cannot read {}: {e}", lang_path.display()));

            let en_keys = extract_ftl_keys(&en_content);
            let lang_keys = extract_ftl_keys(&lang_content);

            let mut missing_in_lang: Vec<_> =
                en_keys.difference(&lang_keys).cloned().collect();
            missing_in_lang.sort();

            assert!(
                missing_in_lang.is_empty(),
                "Keys present in en/{ftl_name} but missing in {lang}/{ftl_name}:\n  {}\n",
                missing_in_lang.join("\n  ")
            );
        }
    }
}
