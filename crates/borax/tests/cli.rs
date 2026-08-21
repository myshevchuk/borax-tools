#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use borax::cli::{Cli, Command, LedgerAction, Settings, flag_layers};
use borax::config::{
    BibLayer, ExtractionLayer, Layer, NetworkLayer, Origin, RenameLayer, layer_from_toml, resolve,
};
use borax::event::Format;
use borax_core::rename::CollisionPolicy;
use clap::{CommandFactory, Parser};

// ---------------------------------------------------------------------
// parse
// ---------------------------------------------------------------------

/// Parse `args` (without the program name) into a [`Cli`], panicking on
/// any parse error.
fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["borax"];
    full.extend_from_slice(args);
    <Cli as Parser>::try_parse_from(full).unwrap()
}

// ---------------------------------------------------------------------
// the clap surface
// ---------------------------------------------------------------------

#[test]
fn the_clap_command_is_internally_consistent() {
    <Cli as CommandFactory>::command().debug_assert();
}

// ---------------------------------------------------------------------
// subcommand parsing
// ---------------------------------------------------------------------

#[test]
fn resolve_parses_multiple_paths_in_order() {
    let cli = parse(&["resolve", "a.pdf", "b.pdf"]);

    assert_eq!(
        cli.command,
        Command::Resolve {
            paths: vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")],
        },
        "got {:?}",
        cli.command
    );
}

#[test]
fn resolve_with_no_paths_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "resolve"]);
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn rename_without_apply_parses_apply_as_false() {
    let cli = parse(&["rename", "f.pdf"]);

    assert_eq!(
        cli.command,
        Command::Rename {
            paths: vec![PathBuf::from("f.pdf")],
            apply: false,
        },
        "got {:?}",
        cli.command
    );
}

#[test]
fn rename_with_apply_parses_apply_as_true() {
    let cli = parse(&["rename", "--apply", "f.pdf"]);

    assert_eq!(
        cli.command,
        Command::Rename {
            paths: vec![PathBuf::from("f.pdf")],
            apply: true,
        },
        "got {:?}",
        cli.command
    );
}

#[test]
fn rename_with_no_paths_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "rename", "--apply"]);
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn bib_parses_multiple_paths_in_order() {
    let cli = parse(&["bib", "a.pdf", "b.pdf"]);

    assert_eq!(
        cli.command,
        Command::Bib {
            paths: vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")],
        },
        "got {:?}",
        cli.command
    );
}

#[test]
fn bib_with_no_paths_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "bib"]);
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn undo_takes_no_arguments() {
    let cli = parse(&["undo"]);
    assert_eq!(cli.command, Command::Undo, "got {:?}", cli.command);
}

#[test]
fn config_takes_no_arguments() {
    let cli = parse(&["config"]);
    assert_eq!(cli.command, Command::Config, "got {:?}", cli.command);
}

#[test]
fn cache_without_clear_parses_clear_as_false() {
    let cli = parse(&["cache"]);
    assert_eq!(
        cli.command,
        Command::Cache { clear: false },
        "got {:?}",
        cli.command
    );
}

#[test]
fn cache_with_clear_parses_clear_as_true() {
    let cli = parse(&["cache", "--clear"]);
    assert_eq!(
        cli.command,
        Command::Cache { clear: true },
        "got {:?}",
        cli.command
    );
}

#[test]
fn ledger_rebuild_parses_with_no_paths() {
    let cli = parse(&["ledger", "rebuild"]);

    assert_eq!(
        cli.command,
        Command::Ledger {
            action: LedgerAction::Rebuild,
        },
        "got {:?}",
        cli.command
    );
}

#[test]
fn ledger_with_no_action_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "ledger"]);
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn ledger_rebuild_takes_no_positional_arguments() {
    let result = <Cli as Parser>::try_parse_from(["borax", "ledger", "rebuild", "extra.pdf"]);
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn ledger_rebuild_reports_its_own_command_name() {
    assert_eq!(
        Command::Ledger {
            action: LedgerAction::Rebuild,
        }
        .name(),
        "ledger rebuild"
    );
}

#[test]
fn ledger_rebuild_has_no_paths() {
    let command = Command::Ledger {
        action: LedgerAction::Rebuild,
    };
    assert!(command.paths().is_empty());
}

#[test]
fn an_unknown_flag_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "resolve", "--bogus", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");
}

// ---------------------------------------------------------------------
// Cli::format
// ---------------------------------------------------------------------

#[test]
fn format_is_json_when_the_json_flag_is_given() {
    let cli = parse(&["--json", "undo"]);
    assert_eq!(cli.format(), Format::Json);
}

#[test]
fn format_is_human_without_the_json_flag() {
    let cli = parse(&["undo"]);
    assert_eq!(cli.format(), Format::Human);
}

// ---------------------------------------------------------------------
// Command::name
// ---------------------------------------------------------------------

#[test]
fn each_command_variant_reports_its_own_name() {
    assert_eq!(Command::Resolve { paths: vec![] }.name(), "resolve");
    assert_eq!(
        Command::Rename {
            paths: vec![],
            apply: false,
        }
        .name(),
        "rename"
    );
    assert_eq!(Command::Bib { paths: vec![] }.name(), "bib");
    assert_eq!(Command::Undo.name(), "undo");
    assert_eq!(Command::Config.name(), "config");
    assert_eq!(Command::Cache { clear: false }.name(), "cache");
}

#[test]
fn the_six_command_names_are_pairwise_distinct() {
    let names = [
        Command::Resolve { paths: vec![] }.name(),
        Command::Rename {
            paths: vec![],
            apply: false,
        }
        .name(),
        Command::Bib { paths: vec![] }.name(),
        Command::Undo.name(),
        Command::Config.name(),
        Command::Cache { clear: false }.name(),
    ];

    let unique: std::collections::BTreeSet<_> = names.iter().collect();

    assert_eq!(unique.len(), names.len(), "got {names:?}");
}

#[test]
fn name_reports_the_subcommand_as_the_user_typed_it() {
    assert_eq!(parse(&["resolve", "f.pdf"]).command.name(), "resolve");
    assert_eq!(parse(&["rename", "f.pdf"]).command.name(), "rename");
    assert_eq!(parse(&["bib", "f.pdf"]).command.name(), "bib");
    assert_eq!(parse(&["undo"]).command.name(), "undo");
    assert_eq!(parse(&["config"]).command.name(), "config");
    assert_eq!(parse(&["cache"]).command.name(), "cache");
}

// ---------------------------------------------------------------------
// Command::paths
// ---------------------------------------------------------------------

#[test]
fn paths_returns_the_resolve_variants_paths() {
    let command = Command::Resolve {
        paths: vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")],
    };

    assert_eq!(
        command.paths(),
        &[PathBuf::from("a.pdf"), PathBuf::from("b.pdf")]
    );
}

#[test]
fn paths_returns_the_rename_variants_paths() {
    let command = Command::Rename {
        paths: vec![PathBuf::from("f.pdf")],
        apply: true,
    };

    assert_eq!(command.paths(), &[PathBuf::from("f.pdf")]);
}

#[test]
fn paths_returns_the_bib_variants_paths() {
    let command = Command::Bib {
        paths: vec![PathBuf::from("f.pdf")],
    };

    assert_eq!(command.paths(), &[PathBuf::from("f.pdf")]);
}

#[test]
fn paths_is_empty_for_undo_config_and_cache() {
    assert!(Command::Undo.paths().is_empty());
    assert!(Command::Config.paths().is_empty());
    assert!(Command::Cache { clear: false }.paths().is_empty());
}

// ---------------------------------------------------------------------
// global flags are position-independent
// ---------------------------------------------------------------------

#[test]
fn a_global_flag_before_the_subcommand_parses_identically_to_after() {
    let before = parse(&["--mailto", "a@b.example", "rename", "--apply", "f.pdf"]);
    let after = parse(&["rename", "--apply", "--mailto", "a@b.example", "f.pdf"]);

    assert_eq!(before.command, after.command);
    assert_eq!(before.settings, after.settings);
    assert_eq!(before.json, after.json);
}

// ---------------------------------------------------------------------
// flag_layers
// ---------------------------------------------------------------------

#[test]
fn flag_layers_of_default_settings_is_empty() {
    let layers = flag_layers(&Settings::default());
    assert!(layers.is_empty(), "got {layers:?}");
}

#[test]
fn template_alone_sets_the_default_template_key() {
    let layers = flag_layers(&parse(&["--template", "[auth][year]", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("template".to_string()),
            Layer {
                templates: Some(BTreeMap::from([(
                    "default".to_string(),
                    "[auth][year]".to_string(),
                )])),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn sources_alone_sets_the_sources_list() {
    let layers = flag_layers(&parse(&["--sources", "crossref,arxiv", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("sources".to_string()),
            Layer {
                sources: Some(vec!["crossref".to_string(), "arxiv".to_string()]),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn mailto_alone_sets_mailto() {
    let layers = flag_layers(&parse(&["--mailto", "me@example.org", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("mailto".to_string()),
            Layer {
                mailto: Some("me@example.org".to_string()),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn collision_alone_sets_rename_collision() {
    let layers = flag_layers(&parse(&["--collision", "skip", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("collision".to_string()),
            Layer {
                rename: Some(RenameLayer {
                    collision: Some("skip".to_string()),
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn bib_alone_sets_bib_path() {
    let layers = flag_layers(&parse(&["--bib", "refs.bib", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("bib".to_string()),
            Layer {
                bib: Some(BibLayer {
                    path: Some(PathBuf::from("refs.bib")),
                    ..BibLayer::default()
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn duplicates_alone_sets_bib_duplicates() {
    let layers = flag_layers(&parse(&["--duplicates", "update", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("duplicates".to_string()),
            Layer {
                bib: Some(BibLayer {
                    duplicates: Some("update".to_string()),
                    ..BibLayer::default()
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn sidecars_alone_sets_bib_sidecars_true() {
    let layers = flag_layers(&parse(&["--sidecars", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("sidecars".to_string()),
            Layer {
                bib: Some(BibLayer {
                    sidecars: Some(true),
                    ..BibLayer::default()
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn no_sidecars_alone_sets_bib_sidecars_false() {
    let layers = flag_layers(&parse(&["--no-sidecars", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("no-sidecars".to_string()),
            Layer {
                bib: Some(BibLayer {
                    sidecars: Some(false),
                    ..BibLayer::default()
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn page_limit_alone_sets_extraction_page_limit() {
    let layers = flag_layers(&parse(&["--page-limit", "3", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("page-limit".to_string()),
            Layer {
                extraction: Some(ExtractionLayer {
                    page_limit: Some(3)
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn concurrency_alone_sets_network_concurrency() {
    let layers = flag_layers(&parse(&["--concurrency", "2", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("concurrency".to_string()),
            Layer {
                network: Some(NetworkLayer {
                    concurrency: Some(2),
                    ..NetworkLayer::default()
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn min_interval_ms_alone_sets_network_min_interval_ms() {
    let layers = flag_layers(&parse(&["--min-interval-ms", "500", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("min-interval-ms".to_string()),
            Layer {
                network: Some(NetworkLayer {
                    min_interval_ms: Some(500),
                    ..NetworkLayer::default()
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn cache_alone_sets_network_cache_true() {
    let layers = flag_layers(&parse(&["--cache", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("cache".to_string()),
            Layer {
                network: Some(NetworkLayer {
                    cache: Some(true),
                    ..NetworkLayer::default()
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn no_cache_alone_sets_network_cache_false() {
    let layers = flag_layers(&parse(&["--no-cache", "undo"]).settings);

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("no-cache".to_string()),
            Layer {
                network: Some(NetworkLayer {
                    cache: Some(false),
                    ..NetworkLayer::default()
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn several_flags_together_each_produce_their_own_single_setting_layer() {
    let settings = parse(&[
        "--mailto",
        "me@example.org",
        "--collision",
        "skip",
        "--page-limit",
        "3",
        "--cache",
        "undo",
    ])
    .settings;

    let layers = flag_layers(&settings);

    // Four flags given, four layers — no two of them set the same key.
    assert_eq!(layers.len(), 4, "got {layers:?}");
    assert!(
        layers.contains(&(
            Origin::Flag("mailto".to_string()),
            Layer {
                mailto: Some("me@example.org".to_string()),
                ..Layer::default()
            },
        )),
        "got {layers:?}"
    );
    assert!(
        layers.contains(&(
            Origin::Flag("collision".to_string()),
            Layer {
                rename: Some(RenameLayer {
                    collision: Some("skip".to_string()),
                }),
                ..Layer::default()
            },
        )),
        "got {layers:?}"
    );
    assert!(
        layers.contains(&(
            Origin::Flag("page-limit".to_string()),
            Layer {
                extraction: Some(ExtractionLayer {
                    page_limit: Some(3)
                }),
                ..Layer::default()
            },
        )),
        "got {layers:?}"
    );
    assert!(
        layers.contains(&(
            Origin::Flag("cache".to_string()),
            Layer {
                network: Some(NetworkLayer {
                    cache: Some(true),
                    ..NetworkLayer::default()
                }),
                ..Layer::default()
            },
        )),
        "got {layers:?}"
    );
}

// The spec's precedence order end to end: flags from `flag_layers` sit
// above a file layer, and both the winning value and its reported
// origin must be the flag's.
#[test]
fn flags_from_flag_layers_outrank_a_toml_layer_and_report_the_flag_origin() {
    let settings = parse(&[
        "--mailto",
        "flag@example.org",
        "--collision",
        "skip",
        "undo",
    ])
    .settings;
    let file_layer = layer_from_toml(
        r#"
        mailto = "file@example.org"

        [rename]
        collision = "suffix"
        "#,
        Path::new("/config.toml"),
    )
    .unwrap();

    let mut layers = vec![(
        Origin::GlobalFile(PathBuf::from("/config.toml")),
        file_layer,
    )];
    layers.extend(flag_layers(&settings));

    let effective = resolve(layers).unwrap();

    assert_eq!(
        effective.config().mailto.as_deref(),
        Some("flag@example.org")
    );
    assert_eq!(
        effective.origin("mailto"),
        Some(&Origin::Flag("mailto".to_string()))
    );
    assert_eq!(effective.config().collision, CollisionPolicy::Skip);
    assert_eq!(
        effective.origin("rename.collision"),
        Some(&Origin::Flag("collision".to_string()))
    );
}

#[test]
fn sidecars_and_no_sidecars_together_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "--sidecars", "--no-sidecars", "undo"]);
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn cache_and_no_cache_together_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "--cache", "--no-cache", "undo"]);
    assert!(result.is_err(), "got {result:?}");
}
