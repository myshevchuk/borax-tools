#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use borax_core::rename::{CollisionPolicy, PlanInput, PlanItem, PlannedAction, SkipReason, plan};

fn input(source: &str, target: &str, hash: &str) -> PlanInput {
    PlanInput {
        source: source.to_string(),
        target: target.to_string(),
        content_hash: hash.to_string(),
    }
}

fn empty() -> BTreeMap<String, Option<String>> {
    BTreeMap::new()
}

// ---------------------------------------------------------------------
// basic renames and ordering
// ---------------------------------------------------------------------

#[test]
fn simple_rename_with_no_existing_files() {
    let items = vec![input("dl/a.pdf", "smith2024.pdf", "h1")];
    let result = plan(&items, &empty(), CollisionPolicy::Suffix);

    assert_eq!(
        result,
        vec![PlanItem {
            source: "dl/a.pdf".to_string(),
            action: PlannedAction::Rename {
                to: "smith2024.pdf".to_string()
            },
        }]
    );
}

#[test]
fn output_preserves_input_order_and_pairs_each_source() {
    let items = vec![
        input("a.pdf", "one.pdf", "h1"),
        input("b.pdf", "two.pdf", "h2"),
        input("c.pdf", "three.pdf", "h3"),
    ];
    let result = plan(&items, &empty(), CollisionPolicy::Suffix);

    let sources: Vec<&str> = result.iter().map(|item| item.source.as_str()).collect();
    assert_eq!(sources, vec!["a.pdf", "b.pdf", "c.pdf"]);
}

// ---------------------------------------------------------------------
// already-named
// ---------------------------------------------------------------------

#[test]
fn already_named_when_source_equals_target_even_with_different_recorded_hash() {
    let items = vec![input("smith2024.pdf", "smith2024.pdf", "h1")];
    let mut existing = empty();
    existing.insert("smith2024.pdf".to_string(), Some("other-hash".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(result[0].action, PlannedAction::AlreadyNamed);
}

#[test]
fn already_named_when_existing_target_has_identical_content_hash() {
    let items = vec![input("dl/a.pdf", "smith2024.pdf", "h1")];
    let mut existing = empty();
    existing.insert("smith2024.pdf".to_string(), Some("h1".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(result[0].action, PlannedAction::AlreadyNamed);
}

#[test]
fn unknown_existing_hash_is_never_identical_suffix_policy() {
    let items = vec![input("dl/a.pdf", "smith2024.pdf", "h1")];
    let mut existing = empty();
    existing.insert("smith2024.pdf".to_string(), None);

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "smith2024a.pdf".to_string()
        }
    );
}

#[test]
fn unknown_existing_hash_is_never_identical_skip_policy() {
    let items = vec![input("dl/a.pdf", "smith2024.pdf", "h1")];
    let mut existing = empty();
    existing.insert("smith2024.pdf".to_string(), None);

    let result = plan(&items, &existing, CollisionPolicy::Skip);

    assert_eq!(
        result[0].action,
        PlannedAction::Skip {
            reason: SkipReason::TargetCollision
        }
    );
}

#[test]
fn existing_different_hash_is_a_collision() {
    let items = vec![input("dl/a.pdf", "smith2024.pdf", "h1")];
    let mut existing = empty();
    existing.insert("smith2024.pdf".to_string(), Some("other".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "smith2024a.pdf".to_string()
        }
    );
}

// ---------------------------------------------------------------------
// suffix ladder
// ---------------------------------------------------------------------

#[test]
fn suffix_ladder_advances_past_first_taken_letter() {
    let items = vec![input("dl/a.pdf", "smith2024.pdf", "h1")];
    let mut existing = empty();
    existing.insert("smith2024.pdf".to_string(), Some("x".to_string()));
    existing.insert("smith2024a.pdf".to_string(), Some("y".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "smith2024b.pdf".to_string()
        }
    );
}

#[test]
fn suffix_goes_before_the_extension() {
    let items = vec![input("dl/a.pdf", "smith2024.pdf", "h1")];
    let mut existing = empty();
    existing.insert("smith2024.pdf".to_string(), Some("x".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "smith2024a.pdf".to_string()
        }
    );
}

#[test]
fn suffix_on_extensionless_target() {
    let items = vec![input("dl/a", "note", "h1")];
    let mut existing = empty();
    existing.insert("note".to_string(), None);

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "notea".to_string()
        }
    );
}

#[test]
fn suffix_ladder_moves_past_single_letters_into_double_letters() {
    let items = vec![input("dl/a.pdf", "k.pdf", "h1")];
    let mut existing = empty();
    existing.insert("k.pdf".to_string(), Some("x0".to_string()));
    for letter in b'a'..=b'z' {
        let name = format!("k{}.pdf", letter as char);
        existing.insert(name, Some(format!("x{letter}")));
    }

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "kaa.pdf".to_string()
        }
    );
}

// ---------------------------------------------------------------------
// batch-internal collisions
// ---------------------------------------------------------------------

#[test]
fn batch_internal_collision_suffix_policy() {
    let items = vec![
        input("a.pdf", "smith2024.pdf", "h1"),
        input("b.pdf", "smith2024.pdf", "h2"),
    ];

    let result = plan(&items, &empty(), CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "smith2024.pdf".to_string()
        }
    );
    assert_eq!(
        result[1].action,
        PlannedAction::Rename {
            to: "smith2024a.pdf".to_string()
        }
    );
}

#[test]
fn batch_internal_collision_skip_policy() {
    let items = vec![
        input("a.pdf", "smith2024.pdf", "h1"),
        input("b.pdf", "smith2024.pdf", "h2"),
    ];

    let result = plan(&items, &empty(), CollisionPolicy::Skip);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "smith2024.pdf".to_string()
        }
    );
    assert_eq!(
        result[1].action,
        PlannedAction::Skip {
            reason: SkipReason::TargetCollision
        }
    );
}

#[test]
fn batch_internal_collision_with_identical_hashes_still_collides() {
    let items = vec![
        input("a.pdf", "smith2024.pdf", "same-hash"),
        input("b.pdf", "smith2024.pdf", "same-hash"),
    ];

    let result = plan(&items, &empty(), CollisionPolicy::Suffix);

    assert_eq!(
        result[1].action,
        PlannedAction::Rename {
            to: "smith2024a.pdf".to_string()
        }
    );
}

// ---------------------------------------------------------------------
// case-insensitivity
// ---------------------------------------------------------------------

#[test]
fn collision_is_case_insensitive_against_existing_ascii() {
    let items = vec![input("dl/a.pdf", "smith2024.pdf", "h1")];
    let mut existing = empty();
    existing.insert("Smith2024.pdf".to_string(), Some("x".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "smith2024a.pdf".to_string()
        }
    );
}

#[test]
fn collision_is_case_insensitive_for_simple_non_ascii_folding() {
    let items = vec![input("dl/a.pdf", "\u{fc}ber.pdf", "h1")];
    let mut existing = empty();
    existing.insert("\u{dc}BER.pdf".to_string(), None);

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "\u{fc}bera.pdf".to_string()
        }
    );
}

#[test]
fn case_only_self_rename_is_exempt_from_collision_and_already_named() {
    let items = vec![input("Smith2024.pdf", "smith2024.pdf", "h")];
    let mut existing = empty();
    existing.insert("Smith2024.pdf".to_string(), Some("h".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "smith2024.pdf".to_string()
        }
    );
}

// ---------------------------------------------------------------------
// vacated slots and batch-claimed suffixes
// ---------------------------------------------------------------------

#[test]
fn vacated_slots_are_not_reused_suffix_policy() {
    let items = vec![input("a.pdf", "x.pdf", "ha"), input("b.pdf", "a.pdf", "hb")];
    let mut existing = empty();
    existing.insert("a.pdf".to_string(), Some("ha".to_string()));
    existing.insert("b.pdf".to_string(), Some("hb".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "x.pdf".to_string()
        }
    );
    assert_eq!(
        result[1].action,
        PlannedAction::Rename {
            to: "aa.pdf".to_string()
        }
    );
}

#[test]
fn vacated_slots_are_not_reused_skip_policy() {
    let items = vec![input("a.pdf", "x.pdf", "ha"), input("b.pdf", "a.pdf", "hb")];
    let mut existing = empty();
    existing.insert("a.pdf".to_string(), Some("ha".to_string()));
    existing.insert("b.pdf".to_string(), Some("hb".to_string()));

    let result = plan(&items, &existing, CollisionPolicy::Skip);

    assert_eq!(
        result[1].action,
        PlannedAction::Skip {
            reason: SkipReason::TargetCollision
        }
    );
}

#[test]
fn suffixed_candidates_are_claimed_batch_internally() {
    let items = vec![
        input("a.pdf", "t.pdf", "h1"),
        input("b.pdf", "t.pdf", "h2"),
        input("c.pdf", "t.pdf", "h3"),
    ];

    let result = plan(&items, &empty(), CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "t.pdf".to_string()
        }
    );
    assert_eq!(
        result[1].action,
        PlannedAction::Rename {
            to: "ta.pdf".to_string()
        }
    );
    assert_eq!(
        result[2].action,
        PlannedAction::Rename {
            to: "tb.pdf".to_string()
        }
    );
}

#[test]
fn suffix_collision_with_existing_checked_case_insensitively() {
    let items = vec![input("dl/a.pdf", "t.pdf", "h1")];
    let mut existing = empty();
    existing.insert("T.pdf".to_string(), None);
    existing.insert("TA.pdf".to_string(), None);

    let result = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(
        result[0].action,
        PlannedAction::Rename {
            to: "tb.pdf".to_string()
        }
    );
}

// ---------------------------------------------------------------------
// determinism
// ---------------------------------------------------------------------

#[test]
fn plan_is_deterministic_across_repeated_calls() {
    let items = vec![
        input("a.pdf", "smith2024.pdf", "h1"),
        input("b.pdf", "smith2024.pdf", "h2"),
        input("c.pdf", "jones2023.pdf", "h3"),
    ];
    let mut existing = empty();
    existing.insert("jones2023.pdf".to_string(), Some("other".to_string()));

    let first = plan(&items, &existing, CollisionPolicy::Suffix);
    let second = plan(&items, &existing, CollisionPolicy::Suffix);

    assert_eq!(first, second);
}
