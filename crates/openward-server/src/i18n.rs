use fluent_templates::static_loader;
use fluent_templates::Loader;
use unic_langid::LanguageIdentifier;
use std::collections::HashMap;
use std::sync::Arc;

static_loader! {
    static LOCALES = {
        locales: "./locales",
        fallback_language: "en",
    };
}

/// Supported languages. Compile-time known.
pub const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("fr", "Français"),
    ("mg", "Malagasy"),
];

pub const DEFAULT_LANGUAGE: &str = "en";

/// Legal term overrides loaded at startup.
/// Keys are formatted as "lang:fluent-key" (e.g., "fr:legal-basis-police-custody").
pub type LegalOverrides = Arc<HashMap<String, String>>;

/// The translator object passed to every template.
pub struct T {
    lang: LanguageIdentifier,
    legal_overrides: Option<LegalOverrides>,
}

impl T {
    pub fn new(lang_code: &str, legal_overrides: Option<LegalOverrides>) -> Self {
        let lang: LanguageIdentifier = lang_code
            .parse()
            .unwrap_or_else(|_| DEFAULT_LANGUAGE.parse().unwrap());
        T { lang, legal_overrides }
    }

    /// Look up a UI or legal string.
    /// For legal-* keys, checks runtime overrides first.
    /// Returns the key name itself if not found (never blank).
    pub fn get(&self, key: &str) -> String {
        // Check legal overrides first
        if key.starts_with("legal-") {
            if let Some(overrides) = &self.legal_overrides {
                let lang_key = format!("{}:{}", self.lang.language, key);
                if let Some(val) = overrides.get(&lang_key) {
                    return val.clone();
                }
            }
        }
        // Fall back to compiled .ftl files
        LOCALES.lookup(&self.lang, key)
    }

    /// Look up a string with named arguments.
    pub fn get_with(&self, key: &str, args: &HashMap<String, String>) -> String {
        let fluent_args: HashMap<String, fluent_templates::fluent_bundle::FluentValue> =
            args.iter()
                .map(|(k, v)| (k.clone(), v.as_str().into()))
                .collect();
        LOCALES.lookup_with_args(&self.lang, key, &fluent_args)
    }

    /// Convenience: look up with a single argument.
    pub fn get_1(&self, key: &str, arg_name: &str, arg_val: &str) -> String {
        let mut args = HashMap::new();
        args.insert(arg_name.to_string(), arg_val.to_string());
        self.get_with(key, &args)
    }

    /// Convenience: look up with a single u32 argument (for use in templates).
    pub fn get_u32(&self, key: &str, arg_name: &str, val: &u32) -> String {
        self.get_1(key, arg_name, &val.to_string())
    }

    /// Convenience: look up with two u32 arguments (for use in templates).
    pub fn get_2u32(&self, key: &str, n1: &str, v1: &u32, n2: &str, v2: &u32) -> String {
        let mut args = HashMap::new();
        args.insert(n1.to_string(), v1.to_string());
        args.insert(n2.to_string(), v2.to_string());
        self.get_with(key, &args)
    }

    /// Current language code (for <html lang="...">)
    pub fn lang_code(&self) -> &str {
        self.lang.language.as_str()
    }
}

/// Parse a legal overrides TOML file.
/// Format:
/// ```toml
/// [fr]
/// legal-basis-police-custody = "Garde à vue"
/// [mg]
/// legal-basis-remand = "Baiko fitanana"
/// ```
/// Returns a flat map with keys like "fr:legal-basis-police-custody".
pub fn parse_legal_overrides(toml_str: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let table: toml::Table = match toml_str.parse() {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Failed to parse legal overrides TOML: {}", e);
            return result;
        }
    };
    for (lang_code, section) in &table {
        if let toml::Value::Table(entries) = section {
            for (key, val) in entries {
                if let toml::Value::String(s) = val {
                    result.insert(format!("{}:{}", lang_code, key), s.clone());
                }
            }
        }
    }
    result
}

/// Load a compiled-in legal preset by name.
pub fn load_preset(name: &str) -> HashMap<String, String> {
    let toml_str = match name {
        "fr-MG" => include_str!("../legal-presets/fr-MG.toml"),
        _ => return HashMap::new(),
    };
    parse_legal_overrides(toml_str)
}

/// Load legal overrides: merge preset (if any) with runtime file (if any).
/// Runtime file values take priority over preset values.
pub fn load_legal_overrides(
    preset_name: Option<&str>,
    runtime_path: Option<&str>,
) -> Option<LegalOverrides> {
    let mut overrides = HashMap::new();

    // Load preset first
    if let Some(name) = preset_name {
        overrides.extend(load_preset(name));
    }

    // Merge runtime overrides on top
    if let Some(path) = runtime_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                overrides.extend(parse_legal_overrides(&content));
            }
            Err(e) => {
                tracing::warn!("Failed to read legal overrides file {}: {}", path, e);
            }
        }
    }

    if overrides.is_empty() {
        None
    } else {
        Some(Arc::new(overrides))
    }
}
