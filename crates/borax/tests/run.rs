#![allow(clippy::unwrap_used)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use borax::config::{ConfigError, Layer, Origin, resolve};
use borax::run::{config_for, inputs};
use tempfile::tempdir;

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// A `read` fake serving the TOML text paired with a path in `entries`,
/// and reporting [`io::ErrorKind::NotFound`] for every other path.
fn fake_read(
    entries: &'static [(&'static str, &'static str)],
) -> impl Fn(&Path) -> io::Result<String> {
    move |path| {
        entries
            .iter()
            .find(|(candidate, _)| Path::new(candidate) == path)
            .map(|(_, content)| content.to_string())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }
}

/// `pairs` as the owned `(String, String)` list [`config_for`] takes
/// for the process environment.
fn env_vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

// ---------------------------------------------------------------------
// inputs
// ---------------------------------------------------------------------

#[test]
fn a_single_file_path_passes_through_unchanged_whatever_its_extension() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    fs::write(&path, b"").unwrap();

    let result = inputs(&[path.clone()]);

    assert_eq!(result, vec![path], "got {result:?}");
}

#[test]
fn a_directory_expands_to_its_pdf_files_sorted_by_path_dropping_others() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("b.pdf"), b"").unwrap();
    fs::write(dir.path().join("a.pdf"), b"").unwrap();
    fs::write(dir.path().join("notes.txt"), b"").unwrap();

    let result = inputs(&[dir.path().to_path_buf()]);

    assert_eq!(
        result,
        vec![dir.path().join("a.pdf"), dir.path().join("b.pdf")],
        "got {result:?}"
    );
}

#[test]
fn expansion_is_recursive_into_nested_subdirectories() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("sub/deeper")).unwrap();
    fs::write(dir.path().join("sub/deeper/nested.pdf"), b"").unwrap();

    let result = inputs(&[dir.path().to_path_buf()]);

    assert_eq!(
        result,
        vec![dir.path().join("sub/deeper/nested.pdf")],
        "got {result:?}"
    );
}

#[test]
fn extension_matching_ignores_case() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("one.PDF"), b"").unwrap();
    fs::write(dir.path().join("two.Pdf"), b"").unwrap();

    let result = inputs(&[dir.path().to_path_buf()]);

    assert_eq!(
        result,
        vec![dir.path().join("one.PDF"), dir.path().join("two.Pdf")],
        "got {result:?}"
    );
}

#[test]
fn several_paths_are_expanded_in_the_order_given_not_globally_sorted() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    // "zzz" would sort after "aaa" globally, but the first argument's
    // directory must be expanded in full before the second's.
    fs::write(a.join("zzz.pdf"), b"").unwrap();
    fs::write(b.join("aaa.pdf"), b"").unwrap();

    let result = inputs(&[a.clone(), b.clone()]);

    assert_eq!(
        result,
        vec![a.join("zzz.pdf"), b.join("aaa.pdf")],
        "got {result:?}"
    );
}

#[test]
fn a_directly_named_file_also_reached_via_a_directory_appears_once_at_first_position() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.pdf");
    fs::write(&target, b"").unwrap();
    fs::write(dir.path().join("other.pdf"), b"").unwrap();

    let result = inputs(&[target.clone(), dir.path().to_path_buf()]);

    assert_eq!(
        result,
        vec![target, dir.path().join("other.pdf")],
        "got {result:?}"
    );
}

#[test]
fn a_path_that_does_not_exist_contributes_nothing() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.pdf");

    let result = inputs(&[missing]);

    assert_eq!(result, Vec::<PathBuf>::new(), "got {result:?}");
}

#[cfg(unix)]
#[test]
fn an_unreadable_directory_contributes_nothing_rather_than_panicking() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("hidden.pdf"), b"").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    // A process whose effective privileges bypass directory modes (root,
    // typically) makes this case unobservable rather than wrong, so it
    // is skipped instead of asserted falsely.
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!(
            "skipping an_unreadable_directory_contributes_nothing_rather_than_panicking: \
             directory permissions were not enforced (running as root?)"
        );
        return;
    }

    let result = inputs(&[locked.clone()]);

    // Restored before the temp directory's own cleanup, which needs to
    // read and remove what is inside it.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(result, Vec::<PathBuf>::new(), "got {result:?}");
}

#[test]
fn an_empty_paths_slice_yields_an_empty_vector() {
    let result = inputs(&[]);

    assert_eq!(result, Vec::<PathBuf>::new(), "got {result:?}");
}

// ---------------------------------------------------------------------
// config_for: no configuration at all
// ---------------------------------------------------------------------

#[test]
fn with_no_files_no_environment_and_no_flags_the_result_matches_the_built_in_defaults() {
    let start = Path::new("/proj");

    let effective = config_for(start, vec![], &env_vars(&[]), &fake_read(&[])).unwrap();

    assert_eq!(effective, resolve(vec![]).unwrap(), "got {effective:?}");
}

// ---------------------------------------------------------------------
// config_for: the global configuration file
// ---------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn the_global_configuration_file_is_picked_up_from_xdg_config_home() {
    let start = Path::new("/proj");
    let environment = env_vars(&[("XDG_CONFIG_HOME", "/xdg-config")]);
    let global_path = PathBuf::from("/xdg-config/borax/config.toml");
    let read = fake_read(&[(
        "/xdg-config/borax/config.toml",
        "mailto = \"global@example.org\"",
    )]);

    let effective = config_for(start, vec![], &environment, &read).unwrap();

    assert_eq!(
        effective.config().mailto.as_deref(),
        Some("global@example.org")
    );
    assert_eq!(
        effective.origin("mailto"),
        Some(&Origin::GlobalFile(global_path))
    );
}

// ---------------------------------------------------------------------
// config_for: the nearest directory override beats the global file
// ---------------------------------------------------------------------

/// The cli spec's "Per-directory template override" scenario: a global
/// configuration file sets `templates.default`, a `.borax.toml` nearer
/// the run overrides it, and the override wins with its own origin.
#[cfg(unix)]
#[test]
fn the_nearest_directory_override_beats_the_global_file_and_reports_its_own_origin() {
    let start = Path::new("/proj/sub");
    let environment = env_vars(&[("XDG_CONFIG_HOME", "/xdg-config")]);
    let dir_path = PathBuf::from("/proj/sub/.borax.toml");
    let read = fake_read(&[
        (
            "/xdg-config/borax/config.toml",
            "[templates]\ndefault = \"[auth:lower][year]\"",
        ),
        (
            "/proj/sub/.borax.toml",
            "[templates]\ndefault = \"[year]-[auth]\"",
        ),
    ]);

    let effective = config_for(start, vec![], &environment, &read).unwrap();

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

// ---------------------------------------------------------------------
// config_for: the environment beats both files
// ---------------------------------------------------------------------

#[test]
fn an_environment_variable_beats_the_configuration_files() {
    let start = Path::new("/proj");
    let environment = env_vars(&[("BORAX_MAILTO", "env@example.org")]);

    let effective = config_for(start, vec![], &environment, &fake_read(&[])).unwrap();

    assert_eq!(
        effective.config().mailto.as_deref(),
        Some("env@example.org")
    );
    assert_eq!(
        effective.origin("mailto"),
        Some(&Origin::Env("MAILTO".to_string()))
    );
}

// ---------------------------------------------------------------------
// config_for: a flag beats everything
// ---------------------------------------------------------------------

#[test]
fn a_flag_beats_the_environment_and_the_configuration_files() {
    let start = Path::new("/proj");
    let environment = env_vars(&[("BORAX_MAILTO", "env@example.org")]);
    let flags = vec![(
        Origin::Flag("mailto".to_string()),
        Layer {
            mailto: Some("flag@example.org".to_string()),
            ..Layer::default()
        },
    )];

    let effective = config_for(start, flags, &environment, &fake_read(&[])).unwrap();

    assert_eq!(
        effective.config().mailto.as_deref(),
        Some("flag@example.org")
    );
    assert_eq!(
        effective.origin("mailto"),
        Some(&Origin::Flag("mailto".to_string()))
    );
}

// ---------------------------------------------------------------------
// config_for: an absent file contributes no layer
// ---------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_configuration_file_that_is_absent_contributes_no_layer_and_the_run_still_succeeds() {
    let start = Path::new("/proj");
    // XDG_CONFIG_HOME names a candidate, but `read` reports every path
    // as not found: the global file and every `.borax.toml` up to the
    // root are all absent.
    let environment = env_vars(&[("XDG_CONFIG_HOME", "/xdg-config")]);

    let effective = config_for(start, vec![], &environment, &fake_read(&[])).unwrap();

    assert_eq!(effective, resolve(vec![]).unwrap(), "got {effective:?}");
}

// ---------------------------------------------------------------------
// config_for: a present but invalid file
// ---------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_configuration_file_present_but_not_valid_toml_is_unreadable_naming_that_path() {
    let start = Path::new("/proj");
    let environment = env_vars(&[("XDG_CONFIG_HOME", "/xdg-config")]);
    let global_path = PathBuf::from("/xdg-config/borax/config.toml");
    let read = fake_read(&[("/xdg-config/borax/config.toml", "not valid toml {{{")]);

    let result = config_for(start, vec![], &environment, &read);

    match result {
        Err(ConfigError::Unreadable { path, .. }) => assert_eq!(path, global_path),
        other => panic!("expected Unreadable naming {global_path:?}, got {other:?}"),
    }
}
