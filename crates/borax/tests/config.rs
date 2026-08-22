#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use borax::config::{
    BibLayer, Config, ConfigError, ExtractionLayer, Layer, NetworkLayer, OVERRIDE_FILE, Origin,
    RenameLayer, collection_root, global_config_path, layer_from_env, layer_from_toml,
    nearest_override, resolve,
};
use borax::event::Event;
use borax_core::bib_output::DuplicatePolicy;
use borax_core::rename::CollisionPolicy;
use borax_pdf::tiered::DEFAULT_PAGE_LIMIT;
use borax_sources::pace::{DEFAULT_CONCURRENCY, DEFAULT_MIN_INTERVAL};
use borax_sources::source::SourceName;

// --- Config::default() ---

#[test]
fn default_templates_holds_only_the_default_key_with_the_documented_pattern() {
    let config = Config::default();

    assert_eq!(config.templates.len(), 1);
    assert_eq!(
        config.templates.get("default").map(String::as_str),
        Some("[auth:lower][year]_[shorttitle3:camel]")
    );
}

/// Citation keys are their own table, separate from the filename
/// templates, and default to the short `[auth:lower][year]` shape a
/// citation convention expects — not the long filename pattern.
#[test]
fn default_citation_keys_holds_only_the_default_key_with_the_documented_pattern() {
    let config = Config::default();

    assert_eq!(config.citation_keys.len(), 1);
    assert_eq!(
        config.citation_keys.get("default").map(String::as_str),
        Some("[auth:lower][year]")
    );
}

// The default is what borax has clients for, not every service it can
// name: `SourceName::ALL` also lists the ones the dispatch table routes
// to but no client exists for yet.
#[test]
fn default_sources_is_every_supported_service_in_priority_order() {
    assert_eq!(Config::default().sources, SourceName::SUPPORTED.to_vec());
}

#[test]
fn default_has_no_mailto_and_no_bib_path() {
    let config = Config::default();
    assert_eq!(config.mailto, None);
    assert_eq!(config.bib_path, None);
}

/// No configuration means no override: discovery decides the
/// collection root on its own.
#[test]
fn default_has_no_collection_root() {
    assert_eq!(Config::default().collection_root, None);
}

#[test]
fn default_concurrency_and_pacing_match_borax_sources_pace_defaults() {
    let config = Config::default();
    assert_eq!(config.concurrency, DEFAULT_CONCURRENCY);
    assert_eq!(
        config.min_interval_ms,
        DEFAULT_MIN_INTERVAL.as_millis() as u64
    );
}

#[test]
fn default_page_limit_matches_borax_pdf_tiered_default() {
    assert_eq!(Config::default().page_limit, DEFAULT_PAGE_LIMIT);
}

#[test]
fn default_collision_is_suffix_and_duplicates_is_skip() {
    let config = Config::default();
    assert_eq!(config.collision, CollisionPolicy::Suffix);
    assert_eq!(config.duplicates, DuplicatePolicy::Skip);
}

#[test]
fn default_has_no_sidecars_and_the_cache_is_on() {
    let config = Config::default();
    assert!(!config.sidecars);
    assert!(config.cache);
}

/// design: "The ledger is an append-only JSONL file" and "Run logs are
/// the event stream, persisted" are both on by default, degrading loudly
/// rather than opting in.
#[test]
fn default_has_the_ledger_and_the_run_log_on() {
    let config = Config::default();
    assert!(config.ledger);
    assert!(config.run_log);
}

// --- layer_from_toml() ---

#[test]
fn layer_from_toml_parses_a_full_document_into_the_expected_layer() {
    let text = r#"
        sources = ["crossref", "arxiv"]
        mailto = "test@example.org"
        collection-root = "/archive"

        [templates]
        default = "[auth:lower][year]"
        thesis = "[auth:lower]_thesis"

        [citation-keys]
        default = "[auth:lower][year]"
        thesis = "[auth:lower]_thesis_key"

        [rename]
        collision = "skip"

        [bib]
        path = "refs.bib"
        duplicates = "update"
        sidecars = true

        [extraction]
        page-limit = 5

        [network]
        concurrency = 8
        min-interval-ms = 250
        cache = false
    "#;

    let layer = layer_from_toml(text, Path::new("/config/full.toml")).unwrap();

    let mut templates = BTreeMap::new();
    templates.insert("default".to_string(), "[auth:lower][year]".to_string());
    templates.insert("thesis".to_string(), "[auth:lower]_thesis".to_string());

    let mut citation_keys = BTreeMap::new();
    citation_keys.insert("default".to_string(), "[auth:lower][year]".to_string());
    citation_keys.insert("thesis".to_string(), "[auth:lower]_thesis_key".to_string());

    assert_eq!(
        layer,
        Layer {
            templates: Some(templates),
            citation_keys: Some(citation_keys),
            sources: Some(vec!["crossref".to_string(), "arxiv".to_string()]),
            mailto: Some("test@example.org".to_string()),
            collection_root: Some(PathBuf::from("/archive")),
            ledger: None,
            run_log: None,
            rename: Some(RenameLayer {
                collision: Some("skip".to_string()),
            }),
            bib: Some(BibLayer {
                path: Some(PathBuf::from("refs.bib")),
                duplicates: Some("update".to_string()),
                sidecars: Some(true),
            }),
            extraction: Some(ExtractionLayer {
                page_limit: Some(5),
            }),
            network: Some(NetworkLayer {
                concurrency: Some(8),
                min_interval_ms: Some(250),
                cache: Some(false),
            }),
        }
    );
}

#[test]
fn layer_from_toml_of_an_empty_document_is_the_default_layer() {
    let layer = layer_from_toml("", Path::new("/empty.toml")).unwrap();
    assert_eq!(layer, Layer::default());
}

#[test]
fn layer_from_toml_setting_one_key_leaves_every_other_field_none() {
    let layer = layer_from_toml(r#"mailto = "solo@example.org""#, Path::new("/solo.toml")).unwrap();

    assert_eq!(layer.mailto, Some("solo@example.org".to_string()));
    assert_eq!(layer.templates, None);
    assert_eq!(layer.citation_keys, None);
    assert_eq!(layer.sources, None);
    assert_eq!(layer.collection_root, None);
    assert_eq!(layer.rename, None);
    assert_eq!(layer.bib, None);
    assert_eq!(layer.extraction, None);
    assert_eq!(layer.network, None);
}

#[test]
fn layer_from_toml_reads_a_solo_collection_root_key() {
    let layer = layer_from_toml(
        r#"collection-root = "/archive/papers""#,
        Path::new("/solo.toml"),
    )
    .unwrap();

    assert_eq!(
        layer.collection_root,
        Some(PathBuf::from("/archive/papers"))
    );
    assert_eq!(layer.mailto, None);
}

#[test]
fn layer_from_toml_reads_the_ledger_and_run_log_keys() {
    let layer =
        layer_from_toml("ledger = false\nrun-log = false\n", Path::new("/solo.toml")).unwrap();

    assert_eq!(layer.ledger, Some(false));
    assert_eq!(layer.run_log, Some(false));
}

#[test]
fn layer_from_toml_setting_only_run_log_leaves_ledger_unset() {
    let layer = layer_from_toml("run-log = true", Path::new("/solo.toml")).unwrap();

    assert_eq!(layer.run_log, Some(true));
    assert_eq!(layer.ledger, None);
}

#[test]
fn layer_from_toml_accepts_kebab_case_keys() {
    let text = r#"
        [extraction]
        page-limit = 7

        [network]
        min-interval-ms = 500
    "#;

    let layer = layer_from_toml(text, Path::new("/kebab.toml")).unwrap();

    assert_eq!(
        layer.extraction,
        Some(ExtractionLayer {
            page_limit: Some(7),
        })
    );
    assert_eq!(
        layer.network,
        Some(NetworkLayer {
            concurrency: None,
            min_interval_ms: Some(500),
            cache: None,
        })
    );
}

#[test]
fn layer_from_toml_of_malformed_text_is_unreadable_with_the_given_path() {
    let path = Path::new("/bad/path.toml");
    let err = layer_from_toml("this is not [ valid toml", path).unwrap_err();

    match err {
        ConfigError::Unreadable { path: got, .. } => assert_eq!(got, path),
        other => panic!("expected Unreadable, got {other:?}"),
    }
}

#[test]
fn layer_from_toml_rejects_an_unknown_top_level_key() {
    let err = layer_from_toml("bogus = 1", Path::new("/unknown.toml")).unwrap_err();
    assert!(matches!(err, ConfigError::Unreadable { .. }));
}

#[test]
fn layer_from_toml_rejects_an_unknown_key_inside_a_table() {
    let text = "[network]\nbogus = 1\n";
    let err = layer_from_toml(text, Path::new("/unknown-nested.toml")).unwrap_err();
    assert!(matches!(err, ConfigError::Unreadable { .. }));
}

/// A typo of one of the two keys this change adds is still an unknown
/// key, the same as any other misspelled setting.
#[test]
fn layer_from_toml_rejects_a_typo_of_the_new_ledger_key() {
    let err = layer_from_toml("ledgerz = true", Path::new("/unknown-ledger.toml")).unwrap_err();
    assert!(matches!(err, ConfigError::Unreadable { .. }));
}

/// cli spec 5.1 / design "one schema, typed values": `ledger` takes a
/// boolean, and a string in its place is a load-time error naming both
/// the key and the type it expected — not a value silently coerced or
/// accepted.
#[test]
fn layer_from_toml_rejects_ledger_set_to_a_non_boolean_value() {
    let err = layer_from_toml(r#"ledger = "yes""#, Path::new("/bad-ledger.toml")).unwrap_err();

    match err {
        ConfigError::Unreadable { message, .. } => {
            assert!(message.contains("ledger"), "got {message:?}");
            assert!(message.to_lowercase().contains("bool"), "got {message:?}");
        }
        other => panic!("expected Unreadable, got {other:?}"),
    }
}

#[test]
fn layer_from_toml_rejects_run_log_set_to_a_non_boolean_value() {
    let err = layer_from_toml(r#"run-log = "yes""#, Path::new("/bad-run-log.toml")).unwrap_err();

    match err {
        ConfigError::Unreadable { message, .. } => {
            assert!(message.contains("run-log"), "got {message:?}");
            assert!(message.to_lowercase().contains("bool"), "got {message:?}");
        }
        other => panic!("expected Unreadable, got {other:?}"),
    }
}

// --- layer_from_env() ---

#[test]
fn layer_from_env_reads_borax_mailto() {
    let layer = layer_from_env([("BORAX_MAILTO", "test@example.org")]).unwrap();
    assert_eq!(layer.mailto, Some("test@example.org".to_string()));
}

#[test]
fn layer_from_env_reads_borax_network_concurrency() {
    let layer = layer_from_env([("BORAX_NETWORK_CONCURRENCY", "8")]).unwrap();
    assert_eq!(
        layer.network,
        Some(NetworkLayer {
            concurrency: Some(8),
            min_interval_ms: None,
            cache: None,
        })
    );
}

#[test]
fn layer_from_env_reads_borax_network_min_interval_ms() {
    let layer = layer_from_env([("BORAX_NETWORK_MIN_INTERVAL_MS", "250")]).unwrap();
    assert_eq!(
        layer.network,
        Some(NetworkLayer {
            concurrency: None,
            min_interval_ms: Some(250),
            cache: None,
        })
    );
}

#[test]
fn layer_from_env_reads_borax_network_cache() {
    let layer = layer_from_env([("BORAX_NETWORK_CACHE", "false")]).unwrap();
    assert_eq!(
        layer.network,
        Some(NetworkLayer {
            concurrency: None,
            min_interval_ms: None,
            cache: Some(false),
        })
    );
}

#[test]
fn layer_from_env_reads_borax_extraction_page_limit() {
    let layer = layer_from_env([("BORAX_EXTRACTION_PAGE_LIMIT", "7")]).unwrap();
    assert_eq!(
        layer.extraction,
        Some(ExtractionLayer {
            page_limit: Some(7),
        })
    );
}

#[test]
fn layer_from_env_reads_borax_rename_collision() {
    let layer = layer_from_env([("BORAX_RENAME_COLLISION", "skip")]).unwrap();
    assert_eq!(
        layer.rename,
        Some(RenameLayer {
            collision: Some("skip".to_string()),
        })
    );
}

#[test]
fn layer_from_env_reads_borax_bib_path() {
    let layer = layer_from_env([("BORAX_BIB_PATH", "refs.bib")]).unwrap();
    assert_eq!(
        layer.bib,
        Some(BibLayer {
            path: Some(PathBuf::from("refs.bib")),
            duplicates: None,
            sidecars: None,
        })
    );
}

#[test]
fn layer_from_env_reads_borax_bib_duplicates() {
    let layer = layer_from_env([("BORAX_BIB_DUPLICATES", "update")]).unwrap();
    assert_eq!(
        layer.bib,
        Some(BibLayer {
            path: None,
            duplicates: Some("update".to_string()),
            sidecars: None,
        })
    );
}

#[test]
fn layer_from_env_reads_borax_bib_sidecars() {
    let layer = layer_from_env([("BORAX_BIB_SIDECARS", "true")]).unwrap();
    assert_eq!(
        layer.bib,
        Some(BibLayer {
            path: None,
            duplicates: None,
            sidecars: Some(true),
        })
    );
}

#[test]
fn layer_from_env_reads_borax_collection_root() {
    let layer = layer_from_env([("BORAX_COLLECTION_ROOT", "/archive")]).unwrap();
    assert_eq!(layer.collection_root, Some(PathBuf::from("/archive")));
}

#[test]
fn layer_from_env_reads_borax_sources_as_a_comma_separated_list() {
    let layer = layer_from_env([("BORAX_SOURCES", "crossref,arxiv")]).unwrap();
    assert_eq!(
        layer.sources,
        Some(vec!["crossref".to_string(), "arxiv".to_string()])
    );
}

#[test]
fn layer_from_env_ignores_unprefixed_variables() {
    let layer = layer_from_env([("PATH", "/usr/bin"), ("BORAX_MAILTO", "a@example.org")]).unwrap();
    assert_eq!(layer.mailto, Some("a@example.org".to_string()));
}

#[test]
fn layer_from_env_a_prefixed_variable_naming_no_key_is_unknown_env() {
    let err = layer_from_env([("BORAX_FROB", "x")]).unwrap_err();
    assert_eq!(
        err,
        ConfigError::UnknownEnv {
            name: "FROB".to_string()
        }
    );
}

/// Templates are open-ended and cannot be addressed by a single
/// environment variable, so this name is unknown even though its prefix
/// matches a real table.
#[test]
fn layer_from_env_borax_templates_default_is_unknown_env() {
    let err = layer_from_env([("BORAX_TEMPLATES_DEFAULT", "[year]")]).unwrap_err();
    assert_eq!(
        err,
        ConfigError::UnknownEnv {
            name: "TEMPLATES_DEFAULT".to_string()
        }
    );
}

#[test]
fn layer_from_env_a_non_numeric_concurrency_is_invalid_naming_the_key() {
    let err = layer_from_env([("BORAX_NETWORK_CONCURRENCY", "many")]).unwrap_err();
    match err {
        ConfigError::Invalid { key, .. } => assert_eq!(key, "network.concurrency"),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn layer_from_env_a_non_boolean_cache_is_invalid_naming_the_key() {
    let err = layer_from_env([("BORAX_NETWORK_CACHE", "maybe")]).unwrap_err();
    match err {
        ConfigError::Invalid { key, .. } => assert_eq!(key, "network.cache"),
        other => panic!("expected Invalid, got {other:?}"),
    }
}

// --- resolve() precedence ---

#[test]
fn resolve_with_no_layers_gives_defaults_with_default_origin_everywhere() {
    let effective = resolve(vec![]).unwrap();

    assert_eq!(*effective.config(), Config::default());
    for key in [
        "bib.duplicates",
        "bib.path",
        "bib.sidecars",
        "citation-keys.default",
        "collection-root",
        "extraction.page-limit",
        "ledger",
        "mailto",
        "network.cache",
        "network.concurrency",
        "network.min-interval-ms",
        "rename.collision",
        "run-log",
        "sources",
        "templates.default",
    ] {
        assert_eq!(effective.origin(key), Some(&Origin::Default), "{key}");
    }
}

#[test]
fn resolve_lets_a_key_a_layer_is_silent_about_fall_through_to_a_lower_layer() {
    let global_path = PathBuf::from("/base/home/.config/borax/config.toml");
    let global = (
        Origin::GlobalFile(global_path.clone()),
        Layer {
            mailto: Some("global@example.org".to_string()),
            ..Layer::default()
        },
    );
    let directory = (
        Origin::DirectoryFile(PathBuf::from("/proj/.borax.toml")),
        Layer {
            extraction: Some(ExtractionLayer {
                page_limit: Some(9),
            }),
            ..Layer::default()
        },
    );

    let effective = resolve(vec![global, directory]).unwrap();

    assert_eq!(
        effective.config().mailto.as_deref(),
        Some("global@example.org")
    );
    assert_eq!(
        effective.origin("mailto"),
        Some(&Origin::GlobalFile(global_path))
    );
}

/// Cover the full spec chain — defaults, global file, directory
/// override, env, flag — asserting both values and origins.
#[test]
fn resolve_full_precedence_chain_defaults_through_flag() {
    let global_path = PathBuf::from("/base/home/.config/borax/config.toml");
    let dir_path = PathBuf::from("/proj/.borax.toml");

    let global = (
        Origin::GlobalFile(global_path.clone()),
        Layer {
            mailto: Some("global@example.org".to_string()),
            rename: Some(RenameLayer {
                collision: Some("skip".to_string()),
            }),
            extraction: Some(ExtractionLayer {
                page_limit: Some(10),
            }),
            network: Some(NetworkLayer {
                concurrency: Some(2),
                min_interval_ms: None,
                cache: None,
            }),
            ..Layer::default()
        },
    );
    let directory = (
        Origin::DirectoryFile(dir_path.clone()),
        Layer {
            extraction: Some(ExtractionLayer {
                page_limit: Some(20),
            }),
            ..Layer::default()
        },
    );
    let env = (
        Origin::Env("NETWORK_CONCURRENCY".to_string()),
        Layer {
            network: Some(NetworkLayer {
                concurrency: Some(8),
                min_interval_ms: None,
                cache: None,
            }),
            ..Layer::default()
        },
    );
    let flag = (
        Origin::Flag("mailto".to_string()),
        Layer {
            mailto: Some("flag@example.org".to_string()),
            ..Layer::default()
        },
    );

    let effective = resolve(vec![global, directory, env, flag]).unwrap();
    let config = effective.config();

    assert_eq!(config.mailto.as_deref(), Some("flag@example.org"));
    assert_eq!(
        effective.origin("mailto"),
        Some(&Origin::Flag("mailto".to_string()))
    );

    assert_eq!(config.page_limit, 20);
    assert_eq!(
        effective.origin("extraction.page-limit"),
        Some(&Origin::DirectoryFile(dir_path))
    );

    assert_eq!(config.concurrency, 8);
    assert_eq!(
        effective.origin("network.concurrency"),
        Some(&Origin::Env("NETWORK_CONCURRENCY".to_string()))
    );

    assert_eq!(config.collision, CollisionPolicy::Skip);
    assert_eq!(
        effective.origin("rename.collision"),
        Some(&Origin::GlobalFile(global_path))
    );

    // Untouched by any layer: falls all the way through to the default.
    assert_eq!(
        config.min_interval_ms,
        DEFAULT_MIN_INTERVAL.as_millis() as u64
    );
    assert_eq!(
        effective.origin("network.min-interval-ms"),
        Some(&Origin::Default)
    );
}

/// cli spec scenario "Collection root from config discovery" hinges on
/// this key being resolvable and reportable exactly like every other
/// setting — a `.borax.toml` setting `collection-root` wins over the
/// global file and reports its own origin.
#[test]
fn resolve_reports_a_collection_root_override_with_its_file_origin() {
    let dir_path = PathBuf::from("/proj/.borax.toml");
    let layers = vec![(
        Origin::DirectoryFile(dir_path.clone()),
        Layer {
            collection_root: Some(PathBuf::from("/archive/unusual-layout")),
            ..Layer::default()
        },
    )];

    let effective = resolve(layers).unwrap();

    assert_eq!(
        effective.config().collection_root,
        Some(PathBuf::from("/archive/unusual-layout"))
    );
    assert_eq!(
        effective.origin("collection-root"),
        Some(&Origin::DirectoryFile(dir_path))
    );
}

/// design "Config hardening": the `ledger` and `run-log` booleans this
/// change adds resolve and report their origin exactly like every other
/// setting.
#[test]
fn resolve_applies_configured_ledger_and_run_log_values_with_their_origin() {
    let dir_path = PathBuf::from("/proj/.borax.toml");
    let layers = vec![(
        Origin::DirectoryFile(dir_path.clone()),
        Layer {
            ledger: Some(false),
            run_log: Some(false),
            ..Layer::default()
        },
    )];

    let effective = resolve(layers).unwrap();

    assert!(!effective.config().ledger);
    assert!(!effective.config().run_log);
    assert_eq!(
        effective.origin("ledger"),
        Some(&Origin::DirectoryFile(dir_path.clone()))
    );
    assert_eq!(
        effective.origin("run-log"),
        Some(&Origin::DirectoryFile(dir_path))
    );
}

// --- template table merging ---

#[test]
fn template_table_merge_keeps_the_lower_layers_default_when_an_override_adds_thesis() {
    let mut thesis_only = BTreeMap::new();
    thesis_only.insert("thesis".to_string(), "[auth]_thesis".to_string());

    let dir_path = PathBuf::from("/proj/.borax.toml");
    let layers = vec![(
        Origin::DirectoryFile(dir_path.clone()),
        Layer {
            templates: Some(thesis_only),
            ..Layer::default()
        },
    )];

    let effective = resolve(layers).unwrap();
    let config = effective.config();

    assert_eq!(
        config.templates.get("default").map(String::as_str),
        Some("[auth:lower][year]_[shorttitle3:camel]")
    );
    assert_eq!(
        config.templates.get("thesis").map(String::as_str),
        Some("[auth]_thesis")
    );

    assert_eq!(
        effective.origin("templates.default"),
        Some(&Origin::Default)
    );
    assert_eq!(
        effective.origin("templates.thesis"),
        Some(&Origin::DirectoryFile(dir_path))
    );
}

/// Spec scenario: "Per-directory template override" — a `.borax.toml`
/// redefining `templates.default` wins, and its origin is reported as
/// that override file.
#[test]
fn directory_override_redefining_templates_default_wins_and_reports_its_own_origin() {
    let mut redefined = BTreeMap::new();
    redefined.insert("default".to_string(), "[year]-[auth]".to_string());

    let dir_path = PathBuf::from("/proj/.borax.toml");
    let layers = vec![(
        Origin::DirectoryFile(dir_path.clone()),
        Layer {
            templates: Some(redefined),
            ..Layer::default()
        },
    )];

    let effective = resolve(layers).unwrap();

    assert_eq!(
        effective
            .config()
            .templates
            .get("default")
            .map(String::as_str),
        Some("[year]-[auth]")
    );
    assert_eq!(
        effective.origin("templates.default"),
        Some(&Origin::DirectoryFile(dir_path))
    );
}

// --- citation-key table merging ---

#[test]
fn citation_keys_table_merge_keeps_the_lower_layers_default_when_an_override_adds_thesis() {
    let mut thesis_only = BTreeMap::new();
    thesis_only.insert("thesis".to_string(), "[auth]_thesis".to_string());

    let dir_path = PathBuf::from("/proj/.borax.toml");
    let layers = vec![(
        Origin::DirectoryFile(dir_path.clone()),
        Layer {
            citation_keys: Some(thesis_only),
            ..Layer::default()
        },
    )];

    let effective = resolve(layers).unwrap();
    let config = effective.config();

    assert_eq!(
        config.citation_keys.get("default").map(String::as_str),
        Some("[auth:lower][year]")
    );
    assert_eq!(
        config.citation_keys.get("thesis").map(String::as_str),
        Some("[auth]_thesis")
    );

    assert_eq!(
        effective.origin("citation-keys.default"),
        Some(&Origin::Default)
    );
    assert_eq!(
        effective.origin("citation-keys.thesis"),
        Some(&Origin::DirectoryFile(dir_path))
    );
}

/// Spec scenario: "Citation-key override reported" — a configuration
/// file redefining `citation-keys.default` wins, and its origin is
/// reported as that file.
#[test]
fn directory_override_redefining_citation_keys_default_wins_and_reports_its_own_origin() {
    let mut redefined = BTreeMap::new();
    redefined.insert("default".to_string(), "[year]-[auth:lower]".to_string());

    let dir_path = PathBuf::from("/proj/.borax.toml");
    let layers = vec![(
        Origin::DirectoryFile(dir_path.clone()),
        Layer {
            citation_keys: Some(redefined),
            ..Layer::default()
        },
    )];

    let effective = resolve(layers).unwrap();

    assert_eq!(
        effective
            .config()
            .citation_keys
            .get("default")
            .map(String::as_str),
        Some("[year]-[auth:lower]")
    );
    assert_eq!(
        effective.origin("citation-keys.default"),
        Some(&Origin::DirectoryFile(dir_path))
    );
}

/// The regression this change exists to prevent: `templates` and
/// `citation-keys` are merged from separate layer fields, so redefining
/// one leaves the other at its own default.
#[test]
fn overriding_templates_default_does_not_change_the_citation_keys_default() {
    let mut redefined = BTreeMap::new();
    redefined.insert("default".to_string(), "[year]-[auth]-long-form".to_string());

    let layers = vec![(
        Origin::Flag("templates".to_string()),
        Layer {
            templates: Some(redefined),
            ..Layer::default()
        },
    )];

    let effective = resolve(layers).unwrap();

    assert_eq!(
        effective
            .config()
            .templates
            .get("default")
            .map(String::as_str),
        Some("[year]-[auth]-long-form")
    );
    assert_eq!(
        effective
            .config()
            .citation_keys
            .get("default")
            .map(String::as_str),
        Some("[auth:lower][year]"),
        "citation-keys.default must not move when templates.default does"
    );
    assert_eq!(
        effective.origin("citation-keys.default"),
        Some(&Origin::Default)
    );
}

// --- value validation at resolve time ---

#[test]
fn resolve_rejects_an_unknown_source_name() {
    let origin = Origin::Flag("sources".to_string());
    let layers = vec![(
        origin.clone(),
        Layer {
            sources: Some(vec!["semanticscholar".to_string()]),
            ..Layer::default()
        },
    )];

    let err = resolve(layers).unwrap_err();
    match err {
        ConfigError::Invalid {
            key,
            origin: got_origin,
            ..
        } => {
            assert_eq!(key, "sources");
            assert_eq!(got_origin, origin);
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn resolve_rejects_a_collision_policy_that_is_neither_suffix_nor_skip() {
    let origin = Origin::Flag("rename.collision".to_string());
    let layers = vec![(
        origin.clone(),
        Layer {
            rename: Some(RenameLayer {
                collision: Some("clobber".to_string()),
            }),
            ..Layer::default()
        },
    )];

    let err = resolve(layers).unwrap_err();
    match err {
        ConfigError::Invalid {
            key,
            origin: got_origin,
            ..
        } => {
            assert_eq!(key, "rename.collision");
            assert_eq!(got_origin, origin);
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn resolve_rejects_a_duplicates_policy_that_is_neither_skip_nor_update() {
    let origin = Origin::Flag("bib.duplicates".to_string());
    let layers = vec![(
        origin.clone(),
        Layer {
            bib: Some(BibLayer {
                path: None,
                duplicates: Some("merge".to_string()),
                sidecars: None,
            }),
            ..Layer::default()
        },
    )];

    let err = resolve(layers).unwrap_err();
    match err {
        ConfigError::Invalid {
            key,
            origin: got_origin,
            ..
        } => {
            assert_eq!(key, "bib.duplicates");
            assert_eq!(got_origin, origin);
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn resolve_maps_valid_collision_and_duplicates_spellings_to_their_variants() {
    let layers = vec![(
        Origin::Flag("flags".to_string()),
        Layer {
            rename: Some(RenameLayer {
                collision: Some("skip".to_string()),
            }),
            bib: Some(BibLayer {
                path: None,
                duplicates: Some("update".to_string()),
                sidecars: None,
            }),
            sources: Some(vec!["crossref".to_string(), "openalex".to_string()]),
            ..Layer::default()
        },
    )];

    let config = resolve(layers).unwrap().config().clone();

    assert_eq!(config.collision, CollisionPolicy::Skip);
    assert_eq!(config.duplicates, DuplicatePolicy::Update);
    assert_eq!(
        config.sources,
        vec![SourceName::Crossref, SourceName::OpenAlex]
    );
}

// --- Effective::origin / Effective::entries ---

#[test]
fn origin_of_an_unknown_key_is_none() {
    let effective = resolve(vec![]).unwrap();
    assert_eq!(effective.origin("does.not.exist"), None);
}

#[test]
fn entries_are_ordered_by_key_and_cover_every_setting() {
    let effective = resolve(vec![]).unwrap();
    let keys: Vec<String> = effective
        .entries()
        .into_iter()
        .map(|(key, _, _)| key)
        .collect();

    assert_eq!(
        keys,
        vec![
            "bib.duplicates",
            "bib.path",
            "bib.sidecars",
            "citation-keys.default",
            "collection-root",
            "extraction.page-limit",
            "ledger",
            "mailto",
            "network.cache",
            "network.concurrency",
            "network.min-interval-ms",
            "rename.collision",
            "run-log",
            "sources",
            "templates.default",
        ]
    );
}

/// `collection-root` renders like `bib.path`: quoted TOML text when set,
/// the empty string — never a bare `None` — when it is not, since that
/// is the one form `borax config` output cannot be mistaken for a
/// value pasted back into a configuration file.
#[test]
fn collection_root_renders_as_a_quoted_path_when_set_and_empty_when_unset() {
    let unset = resolve(vec![]).unwrap();
    let (_, value, _) = unset
        .entries()
        .into_iter()
        .find(|(key, _, _)| key == "collection-root")
        .unwrap();
    assert_eq!(value, "");

    let layers = vec![(
        Origin::Flag("collection-root".to_string()),
        Layer {
            collection_root: Some(PathBuf::from("/archive")),
            ..Layer::default()
        },
    )];
    let set = resolve(layers).unwrap();
    let (_, value, _) = set
        .entries()
        .into_iter()
        .find(|(key, _, _)| key == "collection-root")
        .unwrap();
    assert_eq!(value, "\"/archive\"");
}

// --- Effective::events() ---

#[test]
fn events_yields_one_config_setting_per_entry_in_the_same_key_order_with_matching_values_and_origins()
 {
    let effective = resolve(vec![]).unwrap();

    let entries = effective.entries();
    let events = effective.events();

    assert_eq!(events.len(), entries.len(), "got {events:?}");
    for (entry, event) in entries.iter().zip(events.iter()) {
        let (key, value, origin) = entry;
        assert_eq!(
            *event,
            Event::ConfigSetting {
                key: key.clone(),
                value: value.clone(),
                origin: origin.to_string(),
            },
            "mismatch for key {key}"
        );
    }
}

/// `events` renders `origin` through [`Origin`]'s own `Display`, not a
/// bespoke wording of its own — pinned with a non-default origin so a
/// mismatch between the two would show up as an unequal string.
#[test]
fn events_renders_a_non_default_origin_using_its_display_form() {
    let layers = vec![(
        Origin::Flag("mailto".to_string()),
        Layer {
            mailto: Some("flag@example.org".to_string()),
            ..Layer::default()
        },
    )];
    let effective = resolve(layers).unwrap();

    let events = effective.events();
    let mailto_event = events
        .iter()
        .find(|event| matches!(event, Event::ConfigSetting { key, .. } if key == "mailto"))
        .unwrap_or_else(|| panic!("no ConfigSetting for mailto in {events:?}"));

    assert_eq!(
        *mailto_event,
        Event::ConfigSetting {
            key: "mailto".to_string(),
            value: "\"flag@example.org\"".to_string(),
            origin: Origin::Flag("mailto".to_string()).to_string(),
        }
    );
}

/// cli spec scenario "Citation-key override reported": a configuration
/// file setting `citation-keys.default` is reported by `borax config`
/// with that file as its origin.
#[test]
fn events_reports_a_citation_keys_override_with_its_file_origin() {
    let config_path = PathBuf::from("/proj/.borax.toml");
    let layers = vec![(
        Origin::DirectoryFile(config_path.clone()),
        Layer {
            citation_keys: Some(BTreeMap::from([(
                "default".to_string(),
                "[auth:lower][year]a".to_string(),
            )])),
            ..Layer::default()
        },
    )];
    let effective = resolve(layers).unwrap();

    let events = effective.events();
    let event = events
        .iter()
        .find(|event| matches!(event, Event::ConfigSetting { key, .. } if key == "citation-keys.default"))
        .unwrap_or_else(|| panic!("no ConfigSetting for citation-keys.default in {events:?}"));

    assert_eq!(
        *event,
        Event::ConfigSetting {
            key: "citation-keys.default".to_string(),
            value: "\"[auth:lower][year]a\"".to_string(),
            origin: Origin::DirectoryFile(config_path).to_string(),
        }
    );
}

/// `borax config` reports the `ledger` and `run-log` booleans this
/// change adds with the same key/value/origin shape as `network.cache`
/// and every other setting.
#[test]
fn events_reports_a_ledger_and_run_log_override_with_its_file_origin() {
    let config_path = PathBuf::from("/proj/.borax.toml");
    let layers = vec![(
        Origin::DirectoryFile(config_path.clone()),
        Layer {
            ledger: Some(false),
            run_log: Some(false),
            ..Layer::default()
        },
    )];
    let effective = resolve(layers).unwrap();

    let events = effective.events();
    let ledger_event = events
        .iter()
        .find(|event| matches!(event, Event::ConfigSetting { key, .. } if key == "ledger"))
        .unwrap_or_else(|| panic!("no ConfigSetting for ledger in {events:?}"));
    let run_log_event = events
        .iter()
        .find(|event| matches!(event, Event::ConfigSetting { key, .. } if key == "run-log"))
        .unwrap_or_else(|| panic!("no ConfigSetting for run-log in {events:?}"));

    assert_eq!(
        *ledger_event,
        Event::ConfigSetting {
            key: "ledger".to_string(),
            value: "false".to_string(),
            origin: Origin::DirectoryFile(config_path.clone()).to_string(),
        }
    );
    assert_eq!(
        *run_log_event,
        Event::ConfigSetting {
            key: "run-log".to_string(),
            value: "false".to_string(),
            origin: Origin::DirectoryFile(config_path).to_string(),
        }
    );
}

// --- Origin ordering and Display ---

#[test]
fn origin_ordering_is_by_precedence_lowest_first() {
    let ordered = [
        Origin::Default,
        Origin::GlobalFile(PathBuf::from("/g")),
        Origin::DirectoryFile(PathBuf::from("/d")),
        Origin::Env("MAILTO".to_string()),
        Origin::Flag("mailto".to_string()),
    ];

    for i in 0..ordered.len() {
        for j in (i + 1)..ordered.len() {
            assert!(
                ordered[i] < ordered[j],
                "{:?} should precede {:?}",
                ordered[i],
                ordered[j]
            );
        }
    }
}

#[test]
fn origin_display_matches_the_documented_forms() {
    assert_eq!(Origin::Default.to_string(), "defaults");
    assert_eq!(
        Origin::GlobalFile(PathBuf::from("/g/config.toml")).to_string(),
        "global /g/config.toml"
    );
    assert_eq!(
        Origin::DirectoryFile(PathBuf::from("/d/.borax.toml")).to_string(),
        "override /d/.borax.toml"
    );
    assert_eq!(
        Origin::Env("MAILTO".to_string()).to_string(),
        "env BORAX_MAILTO"
    );
    assert_eq!(
        Origin::Flag("mailto".to_string()).to_string(),
        "flag --mailto"
    );
}

// --- nearest_override() ---

/// Reports whether a candidate is one of PATHS, which are written with
/// `/` whatever the platform.
///
/// The comparison is by component rather than by string: the candidates
/// come from [`Path::join`], which separates with `\` on Windows, and
/// `"/proj/.borax.toml" == "/proj\\.borax.toml"` is false even though
/// both name the same file.
fn exists_at(paths: &'static [&'static str]) -> impl Fn(&Path) -> bool {
    move |path| {
        paths
            .iter()
            .any(|expected| Path::new(expected).components().eq(path.components()))
    }
}

#[test]
fn nearest_override_is_found_in_the_starting_directory() {
    let found = nearest_override(
        Path::new("/proj/sub"),
        exists_at(&["/proj/sub/.borax.toml"]),
    );
    assert_eq!(found, Some(PathBuf::from("/proj/sub/.borax.toml")));
}

#[test]
fn nearest_override_is_found_several_levels_up() {
    let found = nearest_override(
        Path::new("/proj/sub/deeper"),
        exists_at(&["/proj/.borax.toml"]),
    );
    assert_eq!(found, Some(PathBuf::from("/proj/.borax.toml")));
}

#[test]
fn nearest_override_prefers_the_closest_file() {
    let found = nearest_override(
        Path::new("/proj/sub"),
        exists_at(&["/proj/sub/.borax.toml", "/proj/.borax.toml"]),
    );
    assert_eq!(found, Some(PathBuf::from("/proj/sub/.borax.toml")));
}

#[test]
fn nearest_override_is_none_when_nothing_matches_up_to_the_root() {
    let found = nearest_override(Path::new("/proj/sub/deeper"), exists_at(&[]));
    assert_eq!(found, None);
}

#[test]
fn nearest_override_path_ends_in_the_override_file() {
    let found =
        nearest_override(Path::new("/proj/sub"), exists_at(&["/proj/.borax.toml"])).unwrap();
    assert!(found.ends_with(OVERRIDE_FILE));
}

// --- collection_root() ---

/// cli spec scenario "Collection root from config discovery": the
/// directory holding the nearest `.borax.toml` is the collection root
/// when nothing overrides it.
#[test]
fn collection_root_is_the_directory_holding_the_nearest_override_file() {
    let root = collection_root(
        Path::new("/proj/sub/deeper"),
        None,
        exists_at(&["/proj/.borax.toml"]),
    );
    assert_eq!(root, Some(PathBuf::from("/proj")));
}

#[test]
fn collection_root_is_none_when_no_override_file_exists_up_to_the_root() {
    let root = collection_root(Path::new("/proj/sub"), None, exists_at(&[]));
    assert_eq!(root, None);
}

#[test]
fn collection_root_prefers_the_nearest_override_file() {
    let root = collection_root(
        Path::new("/proj/sub"),
        None,
        exists_at(&["/proj/sub/.borax.toml", "/proj/.borax.toml"]),
    );
    assert_eq!(root, Some(PathBuf::from("/proj/sub")));
}

/// The design's "unusual layouts" case: an explicit `collection-root`
/// key wins over discovery even when a `.borax.toml` sits somewhere
/// else entirely.
#[test]
fn collection_root_configured_overrides_discovery() {
    let root = collection_root(
        Path::new("/proj/sub"),
        Some(Path::new("/archive/unusual-layout")),
        exists_at(&["/proj/.borax.toml"]),
    );
    assert_eq!(root, Some(PathBuf::from("/archive/unusual-layout")));
}

/// The override wins even when discovery would otherwise find nothing
/// at all: it does not merely break ties, it replaces the search.
#[test]
fn collection_root_configured_wins_even_with_no_override_file_anywhere() {
    let root = collection_root(
        Path::new("/proj/sub"),
        Some(Path::new("/archive")),
        exists_at(&[]),
    );
    assert_eq!(root, Some(PathBuf::from("/archive")));
}

// --- global_config_path() ---

#[cfg(unix)]
fn env(entries: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    move |name| {
        entries
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(*value))
    }
}

#[cfg(unix)]
#[test]
fn global_config_path_honors_xdg_config_home_on_unix() {
    let path = global_config_path(env(&[
        ("XDG_CONFIG_HOME", "/base/home/.config"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        path,
        Some(
            PathBuf::from("/base/home/.config")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(unix)]
#[test]
fn global_config_path_falls_back_to_home_dot_config_on_unix() {
    let path = global_config_path(env(&[("HOME", "/base/home")]));

    assert_eq!(
        path,
        Some(
            PathBuf::from("/base/home")
                .join(".config")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(unix)]
#[test]
fn global_config_path_prefers_xdg_config_home_over_home_on_unix() {
    let path = global_config_path(env(&[
        ("XDG_CONFIG_HOME", "/xdg/config"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        path,
        Some(
            PathBuf::from("/xdg/config")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(unix)]
#[test]
fn global_config_path_skips_an_empty_xdg_config_home_on_unix() {
    let path = global_config_path(env(&[("XDG_CONFIG_HOME", ""), ("HOME", "/base/home")]));

    assert_eq!(
        path,
        Some(
            PathBuf::from("/base/home")
                .join(".config")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(unix)]
#[test]
fn global_config_path_skips_a_relative_xdg_config_home_on_unix() {
    let path = global_config_path(env(&[
        ("XDG_CONFIG_HOME", "relative/config"),
        ("HOME", "/base/home"),
    ]));

    assert_eq!(
        path,
        Some(
            PathBuf::from("/base/home")
                .join(".config")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(unix)]
#[test]
fn global_config_path_is_none_when_nothing_qualifies_on_unix() {
    assert_eq!(global_config_path(env(&[])), None);
}

#[cfg(unix)]
#[test]
fn global_config_path_is_none_for_an_empty_home_with_no_xdg_config_home_on_unix() {
    assert_eq!(global_config_path(env(&[("HOME", "")])), None);
}

#[cfg(unix)]
#[test]
fn global_config_path_is_none_for_a_relative_home_with_no_xdg_config_home_on_unix() {
    assert_eq!(global_config_path(env(&[("HOME", "relative")])), None);
}

#[cfg(windows)]
fn env(entries: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    move |name| {
        entries
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(*value))
    }
}

#[cfg(windows)]
#[test]
fn global_config_path_honors_appdata_on_windows() {
    let path = global_config_path(env(&[("APPDATA", r"C:\base\Roaming")]));

    assert_eq!(
        path,
        Some(
            PathBuf::from(r"C:\base\Roaming")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(windows)]
#[test]
fn global_config_path_falls_back_to_xdg_config_home_on_windows() {
    let path = global_config_path(env(&[("XDG_CONFIG_HOME", r"C:\base\.config")]));

    assert_eq!(
        path,
        Some(
            PathBuf::from(r"C:\base\.config")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(windows)]
#[test]
fn global_config_path_prefers_appdata_over_xdg_config_home_on_windows() {
    let path = global_config_path(env(&[
        ("APPDATA", r"C:\base\Roaming"),
        ("XDG_CONFIG_HOME", r"C:\base\.config"),
    ]));

    assert_eq!(
        path,
        Some(
            PathBuf::from(r"C:\base\Roaming")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(windows)]
#[test]
fn global_config_path_skips_an_empty_appdata_on_windows() {
    let path = global_config_path(env(&[
        ("APPDATA", ""),
        ("XDG_CONFIG_HOME", r"C:\base\.config"),
    ]));

    assert_eq!(
        path,
        Some(
            PathBuf::from(r"C:\base\.config")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(windows)]
#[test]
fn global_config_path_skips_a_relative_appdata_on_windows() {
    let path = global_config_path(env(&[
        ("APPDATA", r"base\Roaming"),
        ("XDG_CONFIG_HOME", r"C:\base\.config"),
    ]));

    assert_eq!(
        path,
        Some(
            PathBuf::from(r"C:\base\.config")
                .join("borax")
                .join("config.toml")
        )
    );
}

#[cfg(windows)]
#[test]
fn global_config_path_is_none_when_nothing_qualifies_on_windows() {
    assert_eq!(global_config_path(env(&[])), None);
}

#[cfg(windows)]
#[test]
fn global_config_path_is_none_for_an_empty_xdg_config_home_with_no_appdata_on_windows() {
    assert_eq!(global_config_path(env(&[("XDG_CONFIG_HOME", "")])), None);
}

#[cfg(windows)]
#[test]
fn global_config_path_is_none_for_a_relative_xdg_config_home_with_no_appdata_on_windows() {
    assert_eq!(
        global_config_path(env(&[("XDG_CONFIG_HOME", "relative")])),
        None
    );
}

// ---------------------------------------------------------------------
// sources: only what borax can actually query
// ---------------------------------------------------------------------

// The default set is what borax has clients for. Listing a source it
// cannot query would make `borax config` advertise a capability that
// resolves nothing.
#[test]
fn the_default_sources_are_the_supported_ones() {
    let effective = resolve(Vec::new()).unwrap();
    assert_eq!(effective.config().sources, SourceName::SUPPORTED.to_vec());
}

// A source with no client is refused rather than accepted and then
// silently dropped, which would leave a run asking nobody and skipping
// every file as unresolvable.
#[test]
fn a_source_with_no_client_is_a_configuration_error() {
    for name in ["datacite", "pubmed"] {
        let layer = Layer {
            sources: Some(vec![name.to_string()]),
            ..Layer::default()
        };
        let failure = resolve(vec![(Origin::Flag("sources".to_string()), layer)]).unwrap_err();
        let message = failure.to_string();

        assert!(
            message.contains(name),
            "the error must name the unsupported source, got {message:?}"
        );
        assert!(
            message.contains("crossref"),
            "the error must name the sources that do work, got {message:?}"
        );
    }
}

#[test]
fn an_unknown_source_name_is_still_a_configuration_error() {
    let layer = Layer {
        sources: Some(vec!["nonesuch".to_string()]),
        ..Layer::default()
    };
    assert!(resolve(vec![(Origin::Flag("sources".to_string()), layer)]).is_err());
}
