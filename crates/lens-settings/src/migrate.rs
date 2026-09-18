//! Migration from the hand-written `settings.conf` to `settings.toml`.
//!
//! The old format is a hand-written `key = value` grammar parsed by
//! [`lens_core::settings::Settings::parse`]. The new format is TOML,
//! parsed by [`toml_edit`]. This module bridges the two: it reads the
//! old file, converts it to the new schema, and returns the TOML text.
//!
//! The old file is never deleted by this module. The caller decides
//! whether to back it up.

use crate::schema::Schema;

/// Read a `settings.conf` text and produce a [`Schema`] from it.
///
/// Parsing uses the existing `lens_core` parser, which never fails:
/// unknown keys become problems rather than errors, and defaults fill
/// in for anything absent. The resulting schema therefore always has
/// every field, even if the old file was empty.
///
/// The returned [`MigrationReport`] carries both the schema and any
/// problems the old parser found, so the caller can surface them.
pub fn migrate_from_conf(text: &str) -> MigrationReport {
    let (settings, problems) = lens_core::settings::Settings::parse(text);
    let schema = Schema::from_settings(&settings);
    MigrationReport { schema, problems }
}

/// The result of migrating a `settings.conf` file.
#[derive(Debug)]
pub struct MigrationReport {
    /// The schema derived from the old file. Always complete, even if
    /// the old file was empty (defaults fill in).
    pub schema: Schema,
    /// Problems the old parser found. These are not migration errors:
    /// they are what the old file had wrong, carried forward so the
    /// reader can fix them in the new format.
    pub problems: Vec<lens_core::settings::Problem>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lens_core::policy::AppTier;

    #[test]
    fn an_empty_conf_file_produces_a_complete_schema() {
        let report = migrate_from_conf("");
        assert!(report.problems.is_empty());
        assert_eq!(report.schema, Schema::default());
    }

    #[test]
    fn a_full_conf_file_maps_every_field() {
        let conf = "language = Japanese
model = gemma3:4b
dwell_ms = 250
theme = night-lake
translucency = clear
text = large
endpoint = 127.0.0.1:11434
ocr = automatic
profile = developer
default = allow

[applications]
chrome = allow
code = deny
";
        let report = migrate_from_conf(conf);
        assert!(report.problems.is_empty(), "{:?}", report.problems);

        let schema = &report.schema;
        assert_eq!(schema.version, crate::schema::CURRENT_VERSION);
        assert_eq!(schema.general.language, "Japanese");
        assert_eq!(schema.general.dwell_ms, 250);
        assert_eq!(schema.general.theme, "night-lake");
        assert_eq!(schema.general.translucency, "clear");
        assert_eq!(schema.general.text_size, "large");
        assert_eq!(schema.general.default_tier, AppTier::Allow);
        assert_eq!(schema.general.ocr, "automatic");
        assert_eq!(schema.general.profile, "developer");

        assert_eq!(schema.quick.model.as_deref(), Some("gemma3:4b"));
        assert_eq!(schema.quick.endpoint, "127.0.0.1:11434");

        // Deep starts as a copy of quick, because the old format has
        // only one engine section.
        assert_eq!(schema.deep, schema.quick);

        assert_eq!(
            schema.applications,
            vec![
                crate::schema::ApplicationConfig::new("chrome".to_owned(), AppTier::Allow),
                crate::schema::ApplicationConfig::new("code".to_owned(), AppTier::Deny),
            ]
        );
    }

    #[test]
    fn problems_in_the_old_file_are_carried_forward() {
        let conf = "theme = purple\nlanguage = Japanese\n";
        let report = migrate_from_conf(conf);
        assert_eq!(report.problems.len(), 1);
        assert_eq!(report.schema.general.language, "Japanese");
        assert_eq!(report.schema.general.theme, "system");
    }

    #[test]
    fn auto_model_becomes_none() {
        let report = migrate_from_conf("model = auto\n");
        assert_eq!(report.schema.quick.model, None);
    }
}
