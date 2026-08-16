#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::slice;

use borax::config::{ConfigError, Layer, Origin, resolve};
use borax::run::{Configs, config_for, inputs, start_directory};
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

    let result = inputs(slice::from_ref(&path));

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

// A path the user typed is kept even when nothing is there, so the
// pipeline reports it as unreadable. Dropping it would let a typo
// produce an empty batch that exits 0, indistinguishable from a clean
// run over files that were all fine.
#[test]
fn a_named_path_that_does_not_exist_is_kept_so_the_run_reports_it() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.pdf");

    let result = inputs(slice::from_ref(&missing));

    assert_eq!(result, vec![missing], "got {result:?}");
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

    let result = inputs(slice::from_ref(&locked));

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

// ---------------------------------------------------------------------
// config_for: a file that is there but unreadable
// ---------------------------------------------------------------------

/// A reader answering `NotFound` for every path but `denied`, which
/// fails with a permission error the way an unreadable file does.
fn read_denying(denied: &'static str) -> impl Fn(&Path) -> io::Result<String> {
    move |path| match path == Path::new(denied) {
        true => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "permission denied",
        )),
        false => Err(io::Error::new(io::ErrorKind::NotFound, "no such file")),
    }
}

// An override file that exists but cannot be read is not the same as no
// override file: silently running on settings the user wrote and borax
// could not read is the outcome the module set out to avoid.
#[test]
fn an_override_file_that_cannot_be_read_ends_the_run() {
    let start = Path::new("/lib");
    let override_path = "/lib/.borax.toml";

    let result = config_for(start, vec![], &env_vars(&[]), &read_denying(override_path));

    let failure = result.unwrap_err().to_string();
    assert!(
        failure.contains(".borax.toml"),
        "the error must name the file it could not read, got {failure:?}"
    );
}

// ---------------------------------------------------------------------
// start_directory
// ---------------------------------------------------------------------

#[test]
fn a_path_that_names_a_directory_is_itself_the_starting_point() {
    let paths = vec![PathBuf::from("/library")];
    let working = Path::new("/elsewhere");

    let start = start_directory(&paths, &|path| path == Path::new("/library"), working);

    assert_eq!(start, PathBuf::from("/library"), "got {start:?}");
}

#[test]
fn a_path_that_names_a_file_starts_at_its_parent_directory() {
    let paths = vec![PathBuf::from("/library/paper.pdf")];
    let working = Path::new("/elsewhere");

    let start = start_directory(&paths, &|_| false, working);

    assert_eq!(start, PathBuf::from("/library"), "got {start:?}");
}

#[test]
fn only_the_first_path_is_consulted_when_several_are_given() {
    let paths = vec![PathBuf::from("/first"), PathBuf::from("/second/paper.pdf")];
    let working = Path::new("/elsewhere");

    let start = start_directory(&paths, &|path| path == Path::new("/first"), working);

    assert_eq!(start, PathBuf::from("/first"), "got {start:?}");
}

#[test]
fn no_paths_at_all_starts_at_the_working_directory() {
    let working = Path::new("/elsewhere");

    let start = start_directory(&[], &|_| false, working);

    assert_eq!(start, working, "got {start:?}");
}

#[test]
fn a_bare_relative_filename_with_no_directory_component_starts_at_the_working_directory() {
    let paths = vec![PathBuf::from("paper.pdf")];
    let working = Path::new("/elsewhere");

    let start = start_directory(&paths, &|_| false, working);

    assert_eq!(start, working, "got {start:?}");
}

// A typo'd path is neither an existing file nor an existing directory,
// so `is_directory` answers false for it the same as it would for a
// real file. It starts at its parent rather than falling back to
// `working`, so a mistyped name does not silently change where
// configuration comes from.
#[test]
fn a_path_that_is_neither_an_existing_file_nor_directory_is_treated_as_a_file() {
    let paths = vec![PathBuf::from("/library/typo.pdf")];
    let working = Path::new("/elsewhere");

    let start = start_directory(&paths, &|_| false, working);

    assert_eq!(start, PathBuf::from("/library"), "got {start:?}");
}

// ---------------------------------------------------------------------
// Configs::resolve: each file's own directory, not the run's
// ---------------------------------------------------------------------

/// The defect `Configs` exists to fix: resolving once for the whole run
/// from the first path made the answer depend on argument order. Both
/// orders here must reach the same pair of configurations.
#[test]
fn two_files_in_different_trees_each_use_their_own_override_whichever_order_they_are_given() {
    let alpha = PathBuf::from("/library/alpha/paper.pdf");
    let beta = PathBuf::from("/library/beta/paper.pdf");
    let working = Path::new("/library");
    let read = fake_read(&[
        (
            "/library/alpha/.borax.toml",
            "mailto = \"alpha@example.org\"",
        ),
        ("/library/beta/.borax.toml", "mailto = \"beta@example.org\""),
    ]);

    let forward = Configs::resolve(
        &[alpha.clone(), beta.clone()],
        working,
        vec![],
        &env_vars(&[]),
        &read,
    )
    .unwrap();
    let reversed = Configs::resolve(
        &[beta.clone(), alpha.clone()],
        working,
        vec![],
        &env_vars(&[]),
        &read,
    )
    .unwrap();

    for configs in [&forward, &reversed] {
        assert_eq!(
            configs.for_path(&alpha).config().mailto.as_deref(),
            Some("alpha@example.org")
        );
        assert_eq!(
            configs.for_path(&beta).config().mailto.as_deref(),
            Some("beta@example.org")
        );
    }
}

// ---------------------------------------------------------------------
// Configs::resolve: files sharing a directory share its configuration
// ---------------------------------------------------------------------

#[test]
fn two_files_in_the_same_directory_get_the_same_configuration_from_its_override() {
    let x = PathBuf::from("/library/alpha/x.pdf");
    let y = PathBuf::from("/library/alpha/y.pdf");
    let working = Path::new("/elsewhere");
    let dir_path = PathBuf::from("/library/alpha/.borax.toml");
    let read = fake_read(&[(
        "/library/alpha/.borax.toml",
        "[templates]\ndefault = \"[year]-[auth]\"",
    )]);

    let configs = Configs::resolve(
        &[x.clone(), y.clone()],
        working,
        vec![],
        &env_vars(&[]),
        &read,
    )
    .unwrap();

    assert_eq!(configs.for_path(&x), configs.for_path(&y));
    assert_eq!(
        configs.for_path(&x).origin("templates.default"),
        Some(&Origin::DirectoryFile(dir_path))
    );
}

// ---------------------------------------------------------------------
// Configs::resolve: no override in a file's own directory climbs upward
// ---------------------------------------------------------------------

#[test]
fn a_file_in_a_directory_with_no_override_uses_the_nearest_ancestors_override() {
    let path = PathBuf::from("/library/a/b/paper.pdf");
    let working = Path::new("/elsewhere");
    let ancestor_path = PathBuf::from("/library/a/.borax.toml");
    let read = fake_read(&[("/library/a/.borax.toml", "mailto = \"a@example.org\"")]);

    let configs = Configs::resolve(
        slice::from_ref(&path),
        working,
        vec![],
        &env_vars(&[]),
        &read,
    )
    .unwrap();

    assert_eq!(
        configs.for_path(&path).config().mailto.as_deref(),
        Some("a@example.org")
    );
    assert_eq!(
        configs.for_path(&path).origin("mailto"),
        Some(&Origin::DirectoryFile(ancestor_path))
    );
}

// ---------------------------------------------------------------------
// Configs::for_path: a path the resolver never saw falls back to run()
// ---------------------------------------------------------------------

#[test]
fn for_path_on_a_path_the_resolver_never_saw_returns_the_same_as_run() {
    let seen = PathBuf::from("/library/alpha/paper.pdf");
    let working = Path::new("/elsewhere");
    // `/library/gamma` has an override too, so a resolver that lazily
    // climbed from an unseen path would answer differently from `run()`
    // here — this is what proves the fallback does not do that.
    let read = fake_read(&[
        (
            "/library/alpha/.borax.toml",
            "mailto = \"alpha@example.org\"",
        ),
        (
            "/library/gamma/.borax.toml",
            "mailto = \"gamma@example.org\"",
        ),
    ]);

    let configs = Configs::resolve(
        slice::from_ref(&seen),
        working,
        vec![],
        &env_vars(&[]),
        &read,
    )
    .unwrap();

    let unseen = Path::new("/library/gamma/other.pdf");
    assert_eq!(configs.for_path(unseen), configs.run());
    assert_ne!(
        configs.for_path(unseen).config().mailto.as_deref(),
        Some("gamma@example.org")
    );
}

// ---------------------------------------------------------------------
// Configs::run: resolved from the working directory, not an input path
// ---------------------------------------------------------------------

#[test]
fn run_is_resolved_from_working_not_from_any_input_path() {
    let input = PathBuf::from("/library/alpha/paper.pdf");
    let working = Path::new("/elsewhere");
    let read = fake_read(&[
        (
            "/library/alpha/.borax.toml",
            "mailto = \"alpha@example.org\"",
        ),
        ("/elsewhere/.borax.toml", "mailto = \"working@example.org\""),
    ]);

    let configs = Configs::resolve(
        slice::from_ref(&input),
        working,
        vec![],
        &env_vars(&[]),
        &read,
    )
    .unwrap();

    assert_eq!(
        configs.run().config().mailto.as_deref(),
        Some("working@example.org")
    );
    assert_eq!(
        configs.for_path(&input).config().mailto.as_deref(),
        Some("alpha@example.org")
    );
}

// ---------------------------------------------------------------------
// Configs::resolve: higher layers still outrank a directory override
// ---------------------------------------------------------------------

#[test]
fn an_environment_variable_beats_a_directory_override_in_configs_resolve() {
    let input = PathBuf::from("/library/alpha/paper.pdf");
    let working = Path::new("/elsewhere");
    let environment = env_vars(&[("BORAX_MAILTO", "env@example.org")]);
    let read = fake_read(&[(
        "/library/alpha/.borax.toml",
        "mailto = \"alpha@example.org\"",
    )]);

    let configs = Configs::resolve(
        slice::from_ref(&input),
        working,
        vec![],
        &environment,
        &read,
    )
    .unwrap();

    assert_eq!(
        configs.for_path(&input).config().mailto.as_deref(),
        Some("env@example.org")
    );
    assert_eq!(
        configs.for_path(&input).origin("mailto"),
        Some(&Origin::Env("MAILTO".to_string()))
    );
}

#[test]
fn a_flag_beats_a_directory_override_in_configs_resolve() {
    let input = PathBuf::from("/library/alpha/paper.pdf");
    let working = Path::new("/elsewhere");
    let flags = vec![(
        Origin::Flag("mailto".to_string()),
        Layer {
            mailto: Some("flag@example.org".to_string()),
            ..Layer::default()
        },
    )];
    let read = fake_read(&[(
        "/library/alpha/.borax.toml",
        "mailto = \"alpha@example.org\"",
    )]);

    let configs = Configs::resolve(
        slice::from_ref(&input),
        working,
        flags,
        &env_vars(&[]),
        &read,
    )
    .unwrap();

    assert_eq!(
        configs.for_path(&input).config().mailto.as_deref(),
        Some("flag@example.org")
    );
    assert_eq!(
        configs.for_path(&input).origin("mailto"),
        Some(&Origin::Flag("mailto".to_string()))
    );
}

// ---------------------------------------------------------------------
// Configs::uniform
// ---------------------------------------------------------------------

#[test]
fn uniform_returns_the_same_effective_for_every_path_and_for_run() {
    let effective =
        config_for(Path::new("/proj"), vec![], &env_vars(&[]), &fake_read(&[])).unwrap();

    let configs = Configs::uniform(effective.clone());

    assert_eq!(
        configs.for_path(Path::new("/anywhere/at/all.pdf")),
        &effective
    );
    assert_eq!(configs.run(), &effective);
}

// ---------------------------------------------------------------------
// Configs::resolve: one bad override file ends the whole run
// ---------------------------------------------------------------------

// The broken file sits in the second directory reached, not the first —
// proving the run stops on any of them, not only the one it looks at
// soonest.
#[test]
fn a_directory_override_that_will_not_parse_fails_the_whole_resolve() {
    let good = PathBuf::from("/library/alpha/a.pdf");
    let bad = PathBuf::from("/library/beta/b.pdf");
    let working = Path::new("/elsewhere");
    let broken_path = PathBuf::from("/library/beta/.borax.toml");
    let read = fake_read(&[
        (
            "/library/alpha/.borax.toml",
            "mailto = \"alpha@example.org\"",
        ),
        ("/library/beta/.borax.toml", "not valid toml {{{"),
    ]);

    let result = Configs::resolve(&[good, bad], working, vec![], &env_vars(&[]), &read);

    match result {
        Err(ConfigError::Unreadable { path, .. }) => assert_eq!(path, broken_path),
        other => panic!("expected Unreadable naming {broken_path:?}, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Configs::resolve: a directory shared by many files is read once
// ---------------------------------------------------------------------

/// A `read` fake identical to [`fake_read`], but recording every path it
/// is asked about in `calls`, so a test can count how many times a given
/// file was read.
fn counting_read(
    entries: &'static [(&'static str, &'static str)],
    calls: Rc<RefCell<Vec<PathBuf>>>,
) -> impl Fn(&Path) -> io::Result<String> {
    move |path| {
        calls.borrow_mut().push(path.to_path_buf());
        entries
            .iter()
            .find(|(candidate, _)| Path::new(candidate) == path)
            .map(|(_, content)| content.to_string())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }
}

#[test]
fn a_directory_shared_by_many_files_is_read_once_not_once_per_file() {
    let override_path = "/library/alpha/.borax.toml";
    let paths: Vec<PathBuf> = (0..100)
        .map(|n| PathBuf::from(format!("/library/alpha/file{n}.pdf")))
        .collect();
    let working = Path::new("/elsewhere");
    let calls = Rc::new(RefCell::new(Vec::new()));
    // The literal is inlined rather than named, so the slice is
    // promoted to `'static` as `fake_read`'s callers do it.
    let read = counting_read(
        &[(
            "/library/alpha/.borax.toml",
            "mailto = \"alpha@example.org\"",
        )],
        calls.clone(),
    );

    let configs = Configs::resolve(&paths, working, vec![], &env_vars(&[]), &read).unwrap();

    assert_eq!(
        configs.for_path(&paths[0]).config().mailto.as_deref(),
        Some("alpha@example.org")
    );

    // The same run over a single file in that directory, to compare
    // against. The invariant is that resolving costs the same whether a
    // directory holds one file or a hundred — not that it costs exactly
    // one read, which `config_for` does not promise: it probes for the
    // override file and then reads it.
    let one = Rc::new(RefCell::new(Vec::new()));
    let read_one = counting_read(
        &[(
            "/library/alpha/.borax.toml",
            "mailto = \"alpha@example.org\"",
        )],
        one.clone(),
    );
    Configs::resolve(&paths[..1], working, vec![], &env_vars(&[]), &read_one).unwrap();

    let reads = |calls: &Rc<RefCell<Vec<PathBuf>>>| {
        calls
            .borrow()
            .iter()
            .filter(|path| **path == Path::new(override_path))
            .count()
    };
    assert_eq!(
        reads(&calls),
        reads(&one),
        "a hundred files in one directory cost {} reads of {override_path}, one file cost {}",
        reads(&calls),
        reads(&one),
    );
}
