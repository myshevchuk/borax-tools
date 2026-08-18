#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use borax_core::rename::{
    CollisionPolicy, PlanInput, PlanItem, PlannedAction, Planner, SkipReason, plan,
};

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

// ---------------------------------------------------------------------
// incremental planning
// ---------------------------------------------------------------------

/// One batch call, as the arguments it was made with.
struct Scenario {
    name: &'static str,
    existing: BTreeMap<String, Option<String>>,
    items: Vec<PlanInput>,
    policy: CollisionPolicy,
}

fn snapshot(entries: &[(&str, Option<&str>)]) -> BTreeMap<String, Option<String>> {
    entries
        .iter()
        .map(|(name, hash)| (name.to_string(), hash.map(str::to_string)))
        .collect()
}

/// The letter ladder exhausted through `z`, as
/// `suffix_ladder_moves_past_single_letters_into_double_letters` sets it up.
fn ladder_exhausted() -> Vec<(String, Option<String>)> {
    let mut entries = vec![("k.pdf".to_string(), Some("x0".to_string()))];
    entries.extend((b'a'..=b'z').map(|letter| {
        (
            format!("k{}.pdf", letter as char),
            Some(format!("x{letter}")),
        )
    }));
    entries
}

/// Every shape the batch tests above exercise, as a table the
/// incremental planner is held to.
fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "simple rename, nothing existing",
            existing: snapshot(&[]),
            items: vec![input("dl/a.pdf", "smith2024.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "three independent renames",
            existing: snapshot(&[]),
            items: vec![
                input("a.pdf", "one.pdf", "h1"),
                input("b.pdf", "two.pdf", "h2"),
                input("c.pdf", "three.pdf", "h3"),
            ],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "source equals target, recorded hash differs",
            existing: snapshot(&[("smith2024.pdf", Some("other-hash"))]),
            items: vec![input("smith2024.pdf", "smith2024.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "existing target holds a twin",
            existing: snapshot(&[("smith2024.pdf", Some("h1"))]),
            items: vec![input("dl/a.pdf", "smith2024.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "existing hash unknown, suffix policy",
            existing: snapshot(&[("smith2024.pdf", None)]),
            items: vec![input("dl/a.pdf", "smith2024.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "existing hash unknown, skip policy",
            existing: snapshot(&[("smith2024.pdf", None)]),
            items: vec![input("dl/a.pdf", "smith2024.pdf", "h1")],
            policy: CollisionPolicy::Skip,
        },
        Scenario {
            name: "existing hash differs",
            existing: snapshot(&[("smith2024.pdf", Some("other"))]),
            items: vec![input("dl/a.pdf", "smith2024.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "ladder advances past a taken letter",
            existing: snapshot(&[("smith2024.pdf", Some("x")), ("smith2024a.pdf", Some("y"))]),
            items: vec![input("dl/a.pdf", "smith2024.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "suffix on an extensionless target",
            existing: snapshot(&[("note", None)]),
            items: vec![input("dl/a", "note", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "ladder past z into two letters",
            existing: ladder_exhausted().into_iter().collect(),
            items: vec![input("dl/a.pdf", "k.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "two inputs want one name, suffix policy",
            existing: snapshot(&[]),
            items: vec![
                input("a.pdf", "smith2024.pdf", "h1"),
                input("b.pdf", "smith2024.pdf", "h2"),
            ],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "two inputs want one name, skip policy",
            existing: snapshot(&[]),
            items: vec![
                input("a.pdf", "smith2024.pdf", "h1"),
                input("b.pdf", "smith2024.pdf", "h2"),
            ],
            policy: CollisionPolicy::Skip,
        },
        Scenario {
            name: "two inputs want one name and share a hash",
            existing: snapshot(&[]),
            items: vec![
                input("a.pdf", "smith2024.pdf", "same-hash"),
                input("b.pdf", "smith2024.pdf", "same-hash"),
            ],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "case-insensitive against existing, ascii",
            existing: snapshot(&[("Smith2024.pdf", Some("x"))]),
            items: vec![input("dl/a.pdf", "smith2024.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "case-insensitive against existing, non-ascii",
            existing: snapshot(&[("\u{dc}BER.pdf", None)]),
            items: vec![input("dl/a.pdf", "\u{fc}ber.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "case-only self rename",
            existing: snapshot(&[("Smith2024.pdf", Some("h"))]),
            items: vec![input("Smith2024.pdf", "smith2024.pdf", "h")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "vacated slot, suffix policy",
            existing: snapshot(&[("a.pdf", Some("ha")), ("b.pdf", Some("hb"))]),
            items: vec![input("a.pdf", "x.pdf", "ha"), input("b.pdf", "a.pdf", "hb")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "vacated slot, skip policy",
            existing: snapshot(&[("a.pdf", Some("ha")), ("b.pdf", Some("hb"))]),
            items: vec![input("a.pdf", "x.pdf", "ha"), input("b.pdf", "a.pdf", "hb")],
            policy: CollisionPolicy::Skip,
        },
        Scenario {
            name: "three inputs want one name",
            existing: snapshot(&[]),
            items: vec![
                input("a.pdf", "t.pdf", "h1"),
                input("b.pdf", "t.pdf", "h2"),
                input("c.pdf", "t.pdf", "h3"),
            ],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "suffixed candidate collides case-insensitively",
            existing: snapshot(&[("T.pdf", None), ("TA.pdf", None)]),
            items: vec![input("dl/a.pdf", "t.pdf", "h1")],
            policy: CollisionPolicy::Suffix,
        },
        Scenario {
            name: "mixed batch, some colliding",
            existing: snapshot(&[("jones2023.pdf", Some("other"))]),
            items: vec![
                input("a.pdf", "smith2024.pdf", "h1"),
                input("b.pdf", "smith2024.pdf", "h2"),
                input("c.pdf", "jones2023.pdf", "h3"),
            ],
            policy: CollisionPolicy::Suffix,
        },
    ]
}

/// Drive `items` through a fresh [`Planner`] one at a time.
fn incrementally(scenario: &Scenario) -> Vec<PlanItem> {
    let mut planner = Planner::new(scenario.existing.clone());
    scenario
        .items
        .iter()
        .map(|item| planner.plan(item, scenario.policy))
        .collect()
}

#[test]
fn planner_matches_the_batch_planner_on_every_batch_scenario() {
    for scenario in scenarios() {
        assert_eq!(
            incrementally(&scenario),
            plan(&scenario.items, &scenario.existing, scenario.policy),
            "{}",
            scenario.name
        );
    }
}

#[test]
fn planner_starts_from_the_existing_snapshot() {
    let mut planner = Planner::new(snapshot(&[("smith2024.pdf", Some("x"))]));

    assert_eq!(
        planner
            .plan(
                &input("dl/a.pdf", "smith2024.pdf", "h1"),
                CollisionPolicy::Suffix
            )
            .action,
        PlannedAction::Rename {
            to: "smith2024a.pdf".to_string()
        }
    );
}

#[test]
fn planner_carries_a_claim_between_calls() {
    let mut planner = Planner::new(snapshot(&[]));

    let first = planner.plan(&input("a.pdf", "t.pdf", "h1"), CollisionPolicy::Suffix);
    let second = planner.plan(&input("b.pdf", "t.pdf", "h2"), CollisionPolicy::Suffix);

    assert_eq!(
        first.action,
        PlannedAction::Rename {
            to: "t.pdf".to_string()
        }
    );
    assert_eq!(
        second.action,
        PlannedAction::Rename {
            to: "ta.pdf".to_string()
        }
    );
}

#[test]
fn planner_carries_a_suffixed_claim_between_calls() {
    let mut planner = Planner::new(snapshot(&[]));

    let actions: Vec<PlannedAction> = ["a.pdf", "b.pdf", "c.pdf"]
        .iter()
        .map(|source| {
            planner
                .plan(&input(source, "t.pdf", source), CollisionPolicy::Suffix)
                .action
        })
        .collect();

    assert_eq!(
        actions,
        vec![
            PlannedAction::Rename {
                to: "t.pdf".to_string()
            },
            PlannedAction::Rename {
                to: "ta.pdf".to_string()
            },
            PlannedAction::Rename {
                to: "tb.pdf".to_string()
            },
        ]
    );
}

#[test]
fn planner_carries_a_claim_between_calls_under_skip_policy() {
    let mut planner = Planner::new(snapshot(&[]));

    planner.plan(&input("a.pdf", "t.pdf", "h1"), CollisionPolicy::Skip);
    let second = planner.plan(&input("b.pdf", "t.pdf", "h2"), CollisionPolicy::Skip);

    assert_eq!(
        second.action,
        PlannedAction::Skip {
            reason: SkipReason::TargetCollision
        }
    );
}

#[test]
fn planner_does_not_reuse_a_slot_an_earlier_call_vacated() {
    let mut planner = Planner::new(snapshot(&[("a.pdf", Some("ha")), ("b.pdf", Some("hb"))]));

    planner.plan(&input("a.pdf", "x.pdf", "ha"), CollisionPolicy::Suffix);
    let second = planner.plan(&input("b.pdf", "a.pdf", "hb"), CollisionPolicy::Suffix);

    assert_eq!(
        second.action,
        PlannedAction::Rename {
            to: "aa.pdf".to_string()
        }
    );
}

#[test]
fn planner_exempts_only_the_input_being_planned() {
    let mut planner = Planner::new(snapshot(&[("Smith2024.pdf", Some("h"))]));

    // The first input owns `Smith2024.pdf`, so its own slot is free to
    // it. That exemption belongs to the call, not to the planner: the
    // second input would otherwise be renamed onto a file that exists.
    let first = planner.plan(
        &input("Smith2024.pdf", "smith2024.pdf", "h"),
        CollisionPolicy::Suffix,
    );
    let second = planner.plan(
        &input("other.pdf", "Smith2024.pdf", "h2"),
        CollisionPolicy::Suffix,
    );

    assert_eq!(
        first.action,
        PlannedAction::Rename {
            to: "smith2024.pdf".to_string()
        }
    );
    assert_eq!(
        second.action,
        PlannedAction::Rename {
            to: "Smith2024a.pdf".to_string()
        }
    );
}

#[test]
fn planner_reports_a_twin_as_already_named() {
    let mut planner = Planner::new(snapshot(&[("smith2024.pdf", Some("h1"))]));

    assert_eq!(
        planner
            .plan(
                &input("dl/a.pdf", "smith2024.pdf", "h1"),
                CollisionPolicy::Suffix
            )
            .action,
        PlannedAction::AlreadyNamed
    );
}
