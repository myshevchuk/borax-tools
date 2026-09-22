#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use borax::cli::{Cli, Command, Settings, flag_layers};
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
        Command::resolve(vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")]),
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
        Command::rename(vec![PathBuf::from("f.pdf")], false),
        "got {:?}",
        cli.command
    );
}

#[test]
fn rename_with_apply_parses_apply_as_true() {
    let cli = parse(&["rename", "--apply", "f.pdf"]);

    assert_eq!(
        cli.command,
        Command::rename(vec![PathBuf::from("f.pdf")], true),
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
        Command::bib(vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")]),
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
fn config_takes_no_arguments() {
    let cli = parse(&["config"]);
    assert_eq!(cli.command, Command::config(), "got {:?}", cli.command);
}

#[test]
fn cache_without_clear_parses_clear_as_false() {
    let cli = parse(&["cache"]);
    assert_eq!(cli.command, Command::cache(false), "got {:?}", cli.command);
}

#[test]
fn cache_with_clear_parses_clear_as_true() {
    let cli = parse(&["cache", "--clear"]);
    assert_eq!(cli.command, Command::cache(true), "got {:?}", cli.command);
}

#[test]
fn an_unknown_flag_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "resolve", "--bogus", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");
}

// ---------------------------------------------------------------------
// the accepted surface: each subcommand takes every setting it consumes
// ---------------------------------------------------------------------

#[test]
fn resolve_accepts_every_setting_it_consumes() {
    let cli = parse(&[
        "resolve",
        "--sources",
        "crossref,arxiv",
        "--mailto",
        "me@example.org",
        "--page-limit",
        "3",
        "--min-interval-ms",
        "500",
        "--cache",
        "--concurrency",
        "4",
        "--run-log",
        "f.pdf",
    ]);

    assert_eq!(
        cli.settings(),
        Settings {
            sources: Some(vec!["crossref".to_string(), "arxiv".to_string()]),
            mailto: Some("me@example.org".to_string()),
            page_limit: Some(3),
            min_interval_ms: Some(500),
            cache: true,
            concurrency: Some(4),
            run_log: true,
            ..Settings::default()
        },
        "got {:?}",
        cli.settings()
    );
}

#[test]
fn rename_accepts_every_setting_it_consumes() {
    let cli = parse(&[
        "rename",
        "--apply",
        "--sources",
        "crossref",
        "--mailto",
        "me@example.org",
        "--page-limit",
        "5",
        "--min-interval-ms",
        "250",
        "--no-cache",
        "--collision",
        "suffix",
        "--bib",
        "refs.bib",
        "--duplicates",
        "skip",
        "--sidecars",
        "--record",
        "--run-log",
        "f.pdf",
    ]);

    assert_eq!(
        cli.settings(),
        Settings {
            sources: Some(vec!["crossref".to_string()]),
            mailto: Some("me@example.org".to_string()),
            page_limit: Some(5),
            min_interval_ms: Some(250),
            no_cache: true,
            collision: Some("suffix".to_string()),
            bib: Some(PathBuf::from("refs.bib")),
            duplicates: Some("skip".to_string()),
            sidecars: true,
            record: true,
            run_log: true,
            ..Settings::default()
        },
        "got {:?}",
        cli.settings()
    );
}

#[test]
fn bib_accepts_every_setting_it_consumes() {
    let cli = parse(&[
        "bib",
        "--sources",
        "crossref",
        "--mailto",
        "me@example.org",
        "--page-limit",
        "2",
        "--min-interval-ms",
        "100",
        "--cache",
        "--bib",
        "refs.bib",
        "--duplicates",
        "update",
        "--no-sidecars",
        "--no-run-log",
        "f.pdf",
    ]);

    assert_eq!(
        cli.settings(),
        Settings {
            sources: Some(vec!["crossref".to_string()]),
            mailto: Some("me@example.org".to_string()),
            page_limit: Some(2),
            min_interval_ms: Some(100),
            cache: true,
            bib: Some(PathBuf::from("refs.bib")),
            duplicates: Some("update".to_string()),
            no_sidecars: true,
            no_run_log: true,
            ..Settings::default()
        },
        "got {:?}",
        cli.settings()
    );
}

#[test]
fn config_accepts_every_setting() {
    let cli = parse(&[
        "config",
        "--sources",
        "crossref,arxiv",
        "--mailto",
        "me@example.org",
        "--page-limit",
        "3",
        "--min-interval-ms",
        "500",
        "--cache",
        "--collision",
        "suffix",
        "--bib",
        "refs.bib",
        "--duplicates",
        "skip",
        "--sidecars",
        "--concurrency",
        "4",
        "--record",
        "--run-log",
    ]);

    assert_eq!(
        cli.settings(),
        Settings {
            sources: Some(vec!["crossref".to_string(), "arxiv".to_string()]),
            mailto: Some("me@example.org".to_string()),
            page_limit: Some(3),
            min_interval_ms: Some(500),
            cache: true,
            collision: Some("suffix".to_string()),
            bib: Some(PathBuf::from("refs.bib")),
            duplicates: Some("skip".to_string()),
            sidecars: true,
            concurrency: Some(4),
            record: true,
            run_log: true,
            ..Settings::default()
        },
        "got {:?}",
        cli.settings()
    );
}

#[test]
fn cache_accepts_the_run_log_pair() {
    let cli = parse(&["cache", "--clear", "--run-log"]);

    assert_eq!(
        cli.settings(),
        Settings {
            run_log: true,
            ..Settings::default()
        },
        "got {:?}",
        cli.settings()
    );
}

// ---------------------------------------------------------------------
// the refused surface: an inapplicable setting is an unknown argument
// ---------------------------------------------------------------------

#[test]
fn cache_refuses_no_cache_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "cache", "--no-cache"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--no-cache"), "got {message:?}");
}

#[test]
fn cache_refuses_mailto_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "cache", "--mailto", "me@example.org"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--mailto"), "got {message:?}");
}

#[test]
fn rename_refuses_concurrency_as_an_unknown_argument() {
    let result =
        <Cli as Parser>::try_parse_from(["borax", "rename", "--concurrency", "8", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--concurrency"), "got {message:?}");
}

#[test]
fn bib_refuses_no_record_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "bib", "--no-record", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--no-record"), "got {message:?}");
}

#[test]
fn bib_refuses_collision_as_an_unknown_argument() {
    let result =
        <Cli as Parser>::try_parse_from(["borax", "bib", "--collision", "suffix", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--collision"), "got {message:?}");
}

#[test]
fn resolve_refuses_bib_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "resolve", "--bib", "out.bib", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--bib"), "got {message:?}");
}

#[test]
fn config_refuses_template_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "config", "--template", "[auth][year]"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--template"), "got {message:?}");
}

#[test]
fn rename_refuses_template_as_an_unknown_argument() {
    let result =
        <Cli as Parser>::try_parse_from(["borax", "rename", "--template", "[auth][year]", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--template"), "got {message:?}");
}

// ---------------------------------------------------------------------
// a subcommand's help lists that subcommand's settings
// ---------------------------------------------------------------------

// ---------------------------------------------------------------------
// Cli::format
// ---------------------------------------------------------------------

#[test]
fn format_is_json_when_the_json_flag_is_given() {
    let cli = parse(&["--json", "config"]);
    assert_eq!(cli.format(), Format::Json);
}

#[test]
fn format_is_human_without_the_json_flag() {
    let cli = parse(&["config"]);
    assert_eq!(cli.format(), Format::Human);
}

// ---------------------------------------------------------------------
// Command::name
// ---------------------------------------------------------------------

#[test]
fn each_command_variant_reports_its_own_name() {
    assert_eq!(Command::resolve(vec![]).name(), "resolve");
    assert_eq!(Command::rename(vec![], false).name(), "rename");
    assert_eq!(Command::bib(vec![]).name(), "bib");
    assert_eq!(Command::config().name(), "config");
    assert_eq!(Command::cache(false).name(), "cache");
}

#[test]
fn the_command_names_are_pairwise_distinct() {
    let names = [
        Command::resolve(vec![]).name(),
        Command::rename(vec![], false).name(),
        Command::bib(vec![]).name(),
        Command::config().name(),
        Command::cache(false).name(),
        Command::status(None, false).name(),
        Command::validate(None).name(),
        Command::reconcile(None, false).name(),
    ];

    let unique: std::collections::BTreeSet<_> = names.iter().collect();

    assert_eq!(unique.len(), names.len(), "got {names:?}");
}

#[test]
fn name_reports_the_subcommand_as_the_user_typed_it() {
    assert_eq!(parse(&["resolve", "f.pdf"]).command.name(), "resolve");
    assert_eq!(parse(&["rename", "f.pdf"]).command.name(), "rename");
    assert_eq!(parse(&["bib", "f.pdf"]).command.name(), "bib");
    assert_eq!(parse(&["config"]).command.name(), "config");
    assert_eq!(parse(&["cache"]).command.name(), "cache");
}

// ---------------------------------------------------------------------
// Command::paths
// ---------------------------------------------------------------------

#[test]
fn paths_returns_the_resolve_variants_paths() {
    let command = Command::resolve(vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")]);

    assert_eq!(
        command.paths(),
        &[PathBuf::from("a.pdf"), PathBuf::from("b.pdf")]
    );
}

#[test]
fn paths_returns_the_rename_variants_paths() {
    let command = Command::rename(vec![PathBuf::from("f.pdf")], true);

    assert_eq!(command.paths(), &[PathBuf::from("f.pdf")]);
}

#[test]
fn paths_returns_the_bib_variants_paths() {
    let command = Command::bib(vec![PathBuf::from("f.pdf")]);

    assert_eq!(command.paths(), &[PathBuf::from("f.pdf")]);
}

#[test]
fn paths_is_empty_for_config_and_cache() {
    assert!(Command::config().paths().is_empty());
    assert!(Command::cache(false).paths().is_empty());
}

// ---------------------------------------------------------------------
// a setting flag follows its subcommand; --json does not
// ---------------------------------------------------------------------

#[test]
fn a_setting_flag_before_the_subcommand_is_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from([
        "borax",
        "--mailto",
        "a@b.example",
        "rename",
        "--apply",
        "f.pdf",
    ]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--mailto"), "got {message:?}");
}

#[test]
fn the_same_setting_flag_after_the_subcommand_parses() {
    let cli = parse(&["rename", "--apply", "--mailto", "a@b.example", "f.pdf"]);

    assert_eq!(
        cli.settings().mailto.as_deref(),
        Some("a@b.example"),
        "got {:?}",
        cli.settings()
    );
}

#[test]
fn json_before_or_after_the_subcommand_parses_identically() {
    let before = parse(&["--json", "rename", "--apply", "f.pdf"]);
    let after = parse(&["rename", "--apply", "f.pdf", "--json"]);

    assert_eq!(before.command, after.command);
    assert_eq!(before.settings(), after.settings());
    assert_eq!(before.json, after.json);
}

// ---------------------------------------------------------------------
// boolean pairs are refused together, per subcommand
// ---------------------------------------------------------------------

#[test]
fn rename_refuses_cache_and_no_cache_together() {
    let result =
        <Cli as Parser>::try_parse_from(["borax", "rename", "--cache", "--no-cache", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("--cache") && message.contains("--no-cache"),
        "got {message:?}"
    );
}

#[test]
fn rename_refuses_sidecars_and_no_sidecars_together() {
    let result = <Cli as Parser>::try_parse_from([
        "borax",
        "rename",
        "--sidecars",
        "--no-sidecars",
        "f.pdf",
    ]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("--sidecars") && message.contains("--no-sidecars"),
        "got {message:?}"
    );
}

#[test]
fn rename_refuses_record_and_no_record_together() {
    let result =
        <Cli as Parser>::try_parse_from(["borax", "rename", "--record", "--no-record", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("--record") && message.contains("--no-record"),
        "got {message:?}"
    );
}

#[test]
fn rename_refuses_run_log_and_no_run_log_together() {
    let result =
        <Cli as Parser>::try_parse_from(["borax", "rename", "--run-log", "--no-run-log", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("--run-log") && message.contains("--no-run-log"),
        "got {message:?}"
    );
}

// ---------------------------------------------------------------------
// flag_layers
// ---------------------------------------------------------------------

#[test]
fn flag_layers_of_default_settings_is_empty() {
    let layers = flag_layers(&Settings::default());
    assert!(layers.is_empty(), "got {layers:?}");
}

/// No flag reaches `templates`: `--template` is gone, and nothing else
/// in the accepted surface can produce a layer that names it. This is
/// the test that keeps the door shut if someone re-adds the flag.
#[test]
fn no_flag_can_produce_a_templates_layer() {
    let layers = flag_layers(
        &parse(&[
            "config",
            "--sources",
            "crossref,arxiv",
            "--mailto",
            "me@example.org",
            "--page-limit",
            "3",
            "--min-interval-ms",
            "500",
            "--cache",
            "--collision",
            "suffix",
            "--bib",
            "refs.bib",
            "--duplicates",
            "skip",
            "--sidecars",
            "--concurrency",
            "4",
            "--record",
            "--run-log",
        ])
        .settings(),
    );

    for (origin, layer) in &layers {
        assert!(layer.templates.is_none(), "got {layers:?}");
        assert_ne!(
            origin,
            &Origin::Flag("template".to_string()),
            "got {layers:?}"
        );
    }
}

#[test]
fn sources_alone_sets_the_sources_list() {
    let layers = flag_layers(&parse(&["config", "--sources", "crossref,arxiv"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--mailto", "me@example.org"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--collision", "skip"]).settings());

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("collision".to_string()),
            Layer {
                rename: Some(RenameLayer {
                    collision: Some("skip".to_string()),
                    batch: None,
                    skip_named: None,
                }),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn bib_alone_sets_bib_path() {
    let layers = flag_layers(&parse(&["config", "--bib", "refs.bib"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--duplicates", "update"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--sidecars"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--no-sidecars"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--page-limit", "3"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--concurrency", "2"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--min-interval-ms", "500"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--cache"]).settings());

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
    let layers = flag_layers(&parse(&["config", "--no-cache"]).settings());

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
        "config",
        "--mailto",
        "me@example.org",
        "--collision",
        "skip",
        "--page-limit",
        "3",
        "--cache",
    ])
    .settings();

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
                    batch: None,
                    skip_named: None,
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
        "config",
        "--mailto",
        "flag@example.org",
        "--collision",
        "skip",
    ])
    .settings();
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
    let result =
        <Cli as Parser>::try_parse_from(["borax", "config", "--sidecars", "--no-sidecars"]);
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn cache_and_no_cache_together_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "config", "--cache", "--no-cache"]);
    assert!(result.is_err(), "got {result:?}");
}

// ---------------------------------------------------------------------
// flag_layers: --record / --no-record and --run-log / --no-run-log
//
// design "Config hardening": "Every config-settable boolean flag has an
// auto-generated --no-* negation" — the record and run-log booleans this
// change adds follow the same two-flag shape as sidecars and cache.
// ---------------------------------------------------------------------

#[test]
fn record_alone_sets_record_true() {
    let layers = flag_layers(&parse(&["config", "--record"]).settings());

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("record".to_string()),
            Layer {
                record: Some(true),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn no_record_alone_sets_record_false() {
    let layers = flag_layers(&parse(&["config", "--no-record"]).settings());

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("no-record".to_string()),
            Layer {
                record: Some(false),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn record_and_no_record_together_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "config", "--record", "--no-record"]);
    assert!(result.is_err(), "got {result:?}");
}

#[test]
fn run_log_alone_sets_run_log_true() {
    let layers = flag_layers(&parse(&["config", "--run-log"]).settings());

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("run-log".to_string()),
            Layer {
                run_log: Some(true),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

#[test]
fn no_run_log_alone_sets_run_log_false() {
    let layers = flag_layers(&parse(&["config", "--no-run-log"]).settings());

    assert_eq!(
        layers,
        vec![(
            Origin::Flag("no-run-log".to_string()),
            Layer {
                run_log: Some(false),
                ..Layer::default()
            },
        )],
        "got {layers:?}"
    );
}

// ---------------------------------------------------------------------
// 2.2: the `--batch` / `--no-batch` pair belongs to `rename` and
// `config`, like every other setting the "subcommand accepts only the
// settings it consumes" requirement governs.
// ---------------------------------------------------------------------

#[test]
fn rename_batch_parses() {
    let cli = parse(&["rename", "--batch", "f.pdf"]);
    assert!(cli.settings().batch, "got {:?}", cli.settings());
}

#[test]
fn rename_no_batch_parses() {
    let cli = parse(&["rename", "--no-batch", "f.pdf"]);
    assert!(cli.settings().no_batch, "got {:?}", cli.settings());
}

#[test]
fn config_batch_parses() {
    let cli = parse(&["config", "--batch"]);
    assert!(cli.settings().batch, "got {:?}", cli.settings());
}

#[test]
fn config_no_batch_parses() {
    let cli = parse(&["config", "--no-batch"]);
    assert!(cli.settings().no_batch, "got {:?}", cli.settings());
}

#[test]
fn resolve_refuses_batch_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "resolve", "--batch", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--batch"), "got {message:?}");
}

#[test]
fn bib_refuses_batch_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "bib", "--batch", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--batch"), "got {message:?}");
}

/// design D1: "`--apply --no-batch` refused as a usage error before the
/// run starts" — the pair asks both for a plan carried out unasked and
/// for every move to be asked about, so it is refused rather than
/// silently resolved one way or the other.
#[test]
fn rename_refuses_apply_and_no_batch_together() {
    let result =
        <Cli as Parser>::try_parse_from(["borax", "rename", "--apply", "--no-batch", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("--apply") && message.contains("--no-batch"),
        "got {message:?}"
    );
}

#[test]
fn rename_refuses_batch_and_no_batch_together() {
    let result =
        <Cli as Parser>::try_parse_from(["borax", "rename", "--batch", "--no-batch", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("--batch") && message.contains("--no-batch"),
        "got {message:?}"
    );
}

// ---------------------------------------------------------------------
// 4.1: the `--skip-named` / `--no-skip-named` pair belongs to `rename`
// and `config`, like the batch pair above.
// ---------------------------------------------------------------------

#[test]
fn rename_skip_named_parses() {
    let cli = parse(&["rename", "--skip-named", "f.pdf"]);
    assert!(cli.settings().skip_named, "got {:?}", cli.settings());
}

#[test]
fn rename_no_skip_named_parses() {
    let cli = parse(&["rename", "--no-skip-named", "f.pdf"]);
    assert!(cli.settings().no_skip_named, "got {:?}", cli.settings());
}

#[test]
fn config_skip_named_parses() {
    let cli = parse(&["config", "--skip-named"]);
    assert!(cli.settings().skip_named, "got {:?}", cli.settings());
}

#[test]
fn config_no_skip_named_parses() {
    let cli = parse(&["config", "--no-skip-named"]);
    assert!(cli.settings().no_skip_named, "got {:?}", cli.settings());
}

#[test]
fn resolve_refuses_skip_named_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "resolve", "--skip-named", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--skip-named"), "got {message:?}");
}

#[test]
fn bib_refuses_skip_named_as_an_unknown_argument() {
    let result = <Cli as Parser>::try_parse_from(["borax", "bib", "--skip-named", "f.pdf"]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(message.contains("--skip-named"), "got {message:?}");
}

#[test]
fn rename_refuses_skip_named_and_no_skip_named_together() {
    let result = <Cli as Parser>::try_parse_from([
        "borax",
        "rename",
        "--skip-named",
        "--no-skip-named",
        "f.pdf",
    ]);
    assert!(result.is_err(), "got {result:?}");

    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("--skip-named") && message.contains("--no-skip-named"),
        "got {message:?}"
    );
}

#[test]
fn run_log_and_no_run_log_together_is_a_parse_error() {
    let result = <Cli as Parser>::try_parse_from(["borax", "config", "--run-log", "--no-run-log"]);
    assert!(result.is_err(), "got {result:?}");
}

// ---------------------------------------------------------------------
// the CLI overrides a configured record/run-log value in both directions
// ---------------------------------------------------------------------

#[test]
fn no_record_flag_overrides_a_configured_record_true() {
    let settings = parse(&["config", "--no-record"]).settings();
    let file_layer = layer_from_toml("record = true", Path::new("/config.toml")).unwrap();

    let mut layers = vec![(
        Origin::GlobalFile(PathBuf::from("/config.toml")),
        file_layer,
    )];
    layers.extend(flag_layers(&settings));

    let effective = resolve(layers).unwrap();

    assert!(!effective.config().record);
    assert_eq!(
        effective.origin("record"),
        Some(&Origin::Flag("no-record".to_string()))
    );
}

#[test]
fn record_flag_overrides_a_configured_record_false() {
    let settings = parse(&["config", "--record"]).settings();
    let file_layer = layer_from_toml("record = false", Path::new("/config.toml")).unwrap();

    let mut layers = vec![(
        Origin::GlobalFile(PathBuf::from("/config.toml")),
        file_layer,
    )];
    layers.extend(flag_layers(&settings));

    let effective = resolve(layers).unwrap();

    assert!(effective.config().record);
    assert_eq!(
        effective.origin("record"),
        Some(&Origin::Flag("record".to_string()))
    );
}

#[test]
fn no_run_log_flag_overrides_a_configured_run_log_true() {
    let settings = parse(&["config", "--no-run-log"]).settings();
    let file_layer = layer_from_toml("run-log = true", Path::new("/config.toml")).unwrap();

    let mut layers = vec![(
        Origin::GlobalFile(PathBuf::from("/config.toml")),
        file_layer,
    )];
    layers.extend(flag_layers(&settings));

    let effective = resolve(layers).unwrap();

    assert!(!effective.config().run_log);
    assert_eq!(
        effective.origin("run-log"),
        Some(&Origin::Flag("no-run-log".to_string()))
    );
}

#[test]
fn run_log_flag_overrides_a_configured_run_log_false() {
    let settings = parse(&["config", "--run-log"]).settings();
    let file_layer = layer_from_toml("run-log = false", Path::new("/config.toml")).unwrap();

    let mut layers = vec![(
        Origin::GlobalFile(PathBuf::from("/config.toml")),
        file_layer,
    )];
    layers.extend(flag_layers(&settings));

    let effective = resolve(layers).unwrap();

    assert!(effective.config().run_log);
    assert_eq!(
        effective.origin("run-log"),
        Some(&Origin::Flag("run-log".to_string()))
    );
}
