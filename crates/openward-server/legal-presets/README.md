# Legal Term Presets

This directory contains country-specific legal terminology overrides.

Each TOML file maps Fluent message IDs to display strings, organized by language code.
These override the generic translations in `locales/*/legal.ftl`.

## Format

```toml
[fr]
legal-basis-police-custody = "Garde à vue"

[mg]
legal-basis-police-custody = "Fisamborana ara-polisy"
```

## Creating a new preset

1. Copy an existing preset file (e.g., `fr-MG.toml`) as a starting point
2. Name it `<language>-<country>.toml` (e.g., `fr-SN.toml` for Senegal)
3. Add sections for each language code your facility uses
4. Only include terms that differ from the generic translations
5. Add the preset name to the `load_preset()` match in `src/i18n.rs`
6. Rebuild and deploy

## Runtime overrides

Facilities can also override legal terms at runtime without rebuilding:

1. Create a TOML file in the same format
2. Set `OPENWARD_LEGAL_TERMS=/path/to/legal-terms.toml`
3. Restart the server

Runtime overrides take priority over compiled presets.
