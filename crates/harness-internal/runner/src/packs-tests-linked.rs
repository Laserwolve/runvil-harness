//! What a run reads of the prompts its prompt links: the pick, the
//! refusals, and the preludes read the `uses:` entries of the prompt and
//! then of each linked prompt, an entry two of them declare once; a
//! linked prompt's entry the Host cannot serve is on the roster with its
//! reason and keeps the Pack of its Pack id out of the run, though the
//! run's own prompt names that Pack; and a refusal names the linked
//! prompt that declares its Pack.

use super::*;

/// A Host holding `runvil/web`, `runvil/kit`, and `acme/poop`, each
/// offering one tool, beside `acme/offline`, inserted unavailable.
fn vendors() -> Packs {
    let mut registry = PackRegistry::new();
    for (fqn, tools) in [
        ("runvil/web", ["web/fetch"]),
        ("runvil/kit", ["kit/use"]),
        ("acme/poop", ["poop/scoop"]),
    ] {
        registry
            .insert(
                info(fqn),
                factory(listed, offering(&tools), HostServices::new()),
            )
            .unwrap();
    }
    registry
        .insert_unavailable(
            info("acme/offline"),
            make_tool_error(ErrorCode::ToolOther, "offline takes no configuration"),
        )
        .unwrap();
    Packs::new(registry)
}

#[test]
fn the_pick_reads_the_prompts_entries_then_each_linked_prompts_and_a_shared_entry_once() {
    let host = vendors();
    // `a.md` sorts before `b.md`, so its entries are read first, whatever
    // the order the prompt lists them in.
    let prompt = linking(
        "uses:\n  - web\n",
        &[
            ("b.md", "uses:\n  - acme/poop\n  - web\n"),
            ("a.md", "uses:\n  - kit\n  - web\n"),
        ],
    );
    let picked = host.pick(&prompt);
    assert_eq!(
        picked.fqns,
        [
            "runvil/web",
            "runvil/kit",
            "acme/poop",
            "acme/offline",
            "runvil/subagents",
            "runvil/user",
        ],
        "the prompt's Packs, then each linked prompt's in the order of the prompts' names, \
         then every other installed Pack, each once"
    );
    assert!(picked.unresolved.is_empty(), "{:?}", picked.unresolved);
}

#[test]
fn a_linked_prompts_entry_the_host_cannot_serve_is_on_the_roster_and_keeps_its_pack_out() {
    let host = vendors();
    let prompt = linking(
        "",
        &[(
            "review.md",
            "uses:\n  - acme/web\n  - acme/gone\n  - nope\n",
        )],
    );
    let picked = host.pick(&prompt);
    assert_eq!(
        picked.fqns,
        [
            "acme/offline",
            "acme/poop",
            "runvil/kit",
            "runvil/subagents",
            "runvil/user",
        ],
        "runvil/web holds the Pack id the linked prompt's unresolved entry names, so it \
         stays out"
    );
    let run = begin(&host, BTreeMap::new(), &prompt);
    let roster = run.roster();
    assert_eq!(
        roster.get(&name("web")),
        Some(&unavailable_for("acme/web is not installed"))
    );
    assert_eq!(
        roster.get(&name("gone")),
        Some(&unavailable_for("acme/gone is not installed"))
    );
    assert_eq!(
        roster.get(&name("nope")),
        None,
        "no vendor has it, so it is off the roster"
    );
    assert_eq!(
        snapshot_ids(&run),
        ["kit/use", "poop/scoop"],
        "the catalog holds no tool of a Pack id the roster calls unavailable"
    );
}

#[test]
fn two_prompts_that_name_one_pack_id_under_two_vendors_leave_that_pack_out_of_the_run() {
    let host = vendors();
    let prompt = linking(
        "uses:\n  - web\n",
        &[("review.md", "uses:\n  - acme/web\n")],
    );
    let picked = host.pick(&prompt);
    assert!(
        !picked.fqns.contains(&"runvil/web"),
        "the prompt's own entry resolves runvil/web, and the linked prompt's entry keeps it \
         out: {:?}",
        picked.fqns
    );
    let run = begin(&host, BTreeMap::new(), &prompt);
    assert_eq!(
        run.roster().get(&name("web")),
        Some(&unavailable_for("acme/web is not installed"))
    );
    assert_eq!(snapshot_ids(&run), ["kit/use", "poop/scoop"]);
    assert_eq!(
        host.refusals(&prompt),
        [refused(
            Some("review.md"),
            "web",
            "acme/web is not installed"
        )],
        "the Host refuses the prompt at parse for the entry it cannot serve"
    );
    assert_eq!(
        refusal(&run, &prompt),
        "the environment cannot satisfy this prompt:\n\
         - web is unavailable: acme/web is not installed (at prompt:5)",
        "the run's first step refuses it too, for the Pack its own prompt requires"
    );
}

#[test]
fn an_unresolved_pack_id_keeps_the_first_reason_an_entry_gives() {
    let host = vendors();
    // No vendor has `nope`, so the prompt's bare entry gives no reason,
    // and the linked prompt's entry for the same Pack id gives one.
    let prompt = linking(
        "uses:\n  - nope\n",
        &[("review.md", "uses:\n  - acme/nope\n")],
    );
    let picked = host.pick(&prompt);
    assert_eq!(
        picked.unresolved,
        BTreeMap::from([(name("nope"), Some("acme/nope is not installed".to_owned()))])
    );
    // Both give a reason: the bare entry's, which is read first, stays.
    let prompt = linking(
        "uses:\n  - poop\n",
        &[("review.md", "uses:\n  - zed/poop\n")],
    );
    let picked = host.pick(&prompt);
    assert_eq!(
        picked.unresolved,
        BTreeMap::from([(
            name("poop"),
            Some(
                "poop means runvil/poop, which is not installed; write acme/poop to use that \
                 Pack"
                    .to_owned()
            )
        )])
    );
}

#[test]
fn refusals_name_the_linked_prompt_that_declares_a_pack_and_read_a_shared_entry_once() {
    let host = vendors();
    let prompt = linking(
        "uses:\n  - acme/gone\n  - web\n",
        &[
            ("b.md", "uses:\n  - nope\n  - acme/offline\n  - acme/poop\n"),
            ("a.md", "uses:\n  - acme/gone\n  - nope\n"),
        ],
    );
    assert_eq!(
        host.refusals(&prompt),
        [
            refused(None, "gone", "acme/gone is not installed"),
            refused(Some("a.md"), "nope", "runvil/nope is not installed"),
            refused(Some("b.md"), "offline", "offline takes no configuration"),
        ],
        "each entry once, for the first prompt that declares it, and no refusal for an \
         installed Pack"
    );
    assert!(
        host.refusals(&linking(
            "uses:\n  - web\n",
            &[("review.md", "uses:\n  - kit\n  - acme/poop\n")]
        ))
        .is_empty(),
        "every Pack the two prompts declare is installed"
    );
}

#[test]
fn the_preludes_are_one_for_each_usable_pack_that_any_of_the_prompts_declares() {
    let with_prelude = |fqn: &'static str| (info(fqn).prelude("local pack = ...\n"), Value::Null);
    let host = host(&[
        with_prelude("runvil/first"),
        with_prelude("acme/second"),
        with_prelude("runvil/third"),
        with_prelude("runvil/fourth"),
        (info("runvil/plain"), Value::Null),
    ]);
    let prompt = linking(
        "uses:\n  - third\n",
        &[
            ("b.md", "uses:\n  - acme/second\n  - third\n  - plain\n"),
            ("a.md", "uses:\n  - first\n  - gone\n"),
        ],
    );
    let run = begin(&host, BTreeMap::new(), &prompt);
    let order: Vec<String> = run
        .preludes(&prompt)
        .iter()
        .map(|prelude| prelude.pack().to_string())
        .collect();
    assert_eq!(
        order,
        ["third", "first", "second"],
        "the prompt's, then each linked prompt's in the order of the prompts' names: a Pack \
         two prompts declare once, and none for `fourth`, which no prompt declares, for \
         `plain`, which has no prelude, or for `gone`, which the Host lacks"
    );
}
