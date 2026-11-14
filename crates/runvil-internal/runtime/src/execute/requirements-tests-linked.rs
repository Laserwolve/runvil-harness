//! The run's start checks over the prompts its prompt links: a linked
//! prompt's missing or unavailable Pack and its unmet model role refuse
//! the run in a line that names the prompt, a Pack two prompts declare
//! lists once, each kind of gap lists the run's own prompt before the
//! linked ones, the refusal is placed in the first prompt that caused a
//! gap, and the caller's gaps list once whatever the prompts declare.

use super::*;

/// The prompt `prompt`, whose frontmatter adds `front` and then lists
/// each of `linked` by name, with each linked prompt adding its own
/// frontmatter. Every prompt's added lines start on its file line 5.
fn linking(front: &str, linked: &[(&str, &str)]) -> Prompt {
    let names: Vec<&str> = linked.iter().map(|(name, _)| *name).collect();
    let main = source(&format!("{front}prompts: [{}]\n", names.join(", ")));
    let sources: BTreeMap<String, String> = linked
        .iter()
        .map(|(name, front)| ((*name).to_owned(), source(front)))
        .collect();
    let finder = move |name: &str| -> Result<String, crate::Error> {
        Ok(sources.get(name).unwrap_or(&main).clone())
    };
    Prompt::parse(&finder, "prompt").expect("the requirements test prompts parse")
}

/// A role map whose one model, the `default`, has a 32000-token context
/// and `thinking`.
fn small(thinking: ThinkingMode) -> BTreeMap<ModelRole, runvil_types::models::ModelInfo> {
    BTreeMap::from([(
        ModelRole::Default,
        test_model_with("small", 32_000, thinking),
    )])
}

/// Frontmatter that requires a `strong` role of 200000 tokens.
const NEEDS_CONTEXT: &str = "models: { strong: { min_context: 200000 } }\n";

#[test]
fn a_linked_prompts_missing_and_unavailable_packs_refuse_the_run_naming_the_prompt() {
    let ctx = params().with_packs([unavailable("kit", "the kit server is down"), usable("web")]);
    let prompt = linking(
        "uses: [web]\n",
        &[("review.md", "uses: [web, gone, kit]\n")],
    );
    assert_eq!(
        refusal(prompt, ctx),
        Some(
            "the environment cannot satisfy this prompt:\n\
             - review.md: missing required Pack: gone\n\
             - review.md: kit is unavailable: the kit server is down (at review.md:5)"
                .to_owned()
        )
    );
}

#[test]
fn a_linked_prompts_unmet_model_role_refuses_the_run_naming_the_prompt() {
    let review = "models: { strong: { min_context: 200000, thinking: true } }\n";
    let prompt = linking("", &[("review.md", review)]);
    assert_eq!(
        refusal(prompt, params().with_models(small(ThinkingMode::Never))),
        Some(
            "the environment cannot satisfy this prompt:\n\
             - review.md: role 'strong': requires a context of at least 200000 tokens; \
             the model filling it provides 32000\n\
             - review.md: role 'strong': requires 'thinking: true'; \
             the thinking capability of the model filling it is Never (at review.md:5)"
                .to_owned()
        )
    );
}

#[test]
fn a_run_whose_params_meet_every_prompt_it_links_starts() {
    let ctx = params()
        .with_packs([usable("web"), usable("kit")])
        .with_models(small(ThinkingMode::Switchable));
    let prompt = linking(
        "uses: [web]\n",
        &[(
            "review.md",
            "uses: [kit]\nmodels: { small: { min_context: 32000, thinking: false } }\n",
        )],
    );
    assert_eq!(refusal(prompt, ctx), None);
}

#[test]
fn each_kind_of_gap_lists_the_runs_own_prompt_then_each_linked_prompt_by_name() {
    let ctx = params()
        .with_tools(catalog(&["adhoc/grab"]))
        .with_packs([
            unavailable("kit", "the kit server is down"),
            unavailable("shared", "shared is off"),
        ])
        .with_models(small(ThinkingMode::Always));
    // `a.md` is listed last and its gaps come before those of `b.md`.
    // `shared` and `kit` are each declared twice and listed once, for the
    // first prompt that declares them.
    let prompt = linking(
        &format!("uses: [web, shared]\n{NEEDS_CONTEXT}"),
        &[
            ("b.md", "uses: [gone, shared, kit]\n"),
            (
                "a.md",
                "uses: [kit, other]\nmodels: { small: { thinking: false } }\n",
            ),
        ],
    );
    let reserved = reserved_line("adhoc");
    let expected = [
        "the environment cannot satisfy this prompt:",
        "- missing required Pack: web",
        "- a.md: missing required Pack: other",
        "- b.md: missing required Pack: gone",
        "- shared is unavailable: shared is off",
        "- a.md: kit is unavailable: the kit server is down",
        &reserved,
        "- role 'strong': requires a context of at least 200000 tokens; \
         the model filling it provides 32000",
        "- a.md: role 'small': requires 'thinking: false'; \
         the thinking capability of the model filling it is Always (at prompt:5)",
    ];
    assert_eq!(refusal(prompt, ctx), Some(expected.join("\n")));
}

#[test]
fn the_refusal_is_placed_in_the_first_prompt_that_caused_a_gap() {
    let models = || params().with_models(small(ThinkingMode::Switchable));
    let cases = [
        (
            linking(NEEDS_CONTEXT, &[("review.md", "uses: [web]\n")]),
            models().with_packs([]),
            Some(" (at prompt:5)"),
            "the run's own prompt comes first, though the linked prompt's Pack line lists \
             before its model line",
        ),
        (
            // `models:` is file line 5 of `a.md` and `uses:` file line 6.
            linking(
                "",
                &[
                    ("b.md", "uses: [web]\n"),
                    ("a.md", &format!("{NEEDS_CONTEXT}uses: [web]\n")),
                ],
            ),
            models().with_packs([]),
            Some(" (at a.md:6)"),
            "the first linked prompt by name, on the key of its Pack gap",
        ),
        (
            linking(
                "uses: [web]\n",
                &[("a.md", "uses: [web]\n"), ("b.md", NEEDS_CONTEXT)],
            ),
            models().with_packs([usable("web")]),
            Some(" (at b.md:5)"),
            "a prompt whose requirements are met is passed over",
        ),
        (
            linking("uses: [web]\n", &[("review.md", "uses: [web]\n")]),
            models().with_packs([]),
            Some(" (at prompt:5)"),
            "a Pack both declare is the gap of the first that declares it",
        ),
        (
            linking("", &[("review.md", "uses: [web]\n")]),
            params()
                .with_tools(catalog(&["adhoc/grab"]))
                .with_packs([usable("web")]),
            None,
            "a reserved id is the caller's gap, whatever the prompts declare",
        ),
    ];
    for (prompt, ctx, suffix, why) in cases {
        let message = refusal(prompt, ctx).expect("the run is refused");
        assert_eq!(
            message.find(" (at ").map(|at| &message[at..]),
            suffix,
            "{why}: {message}"
        );
    }
}

#[test]
fn a_role_map_with_no_default_lists_once_whatever_roles_the_prompts_declare() {
    let no_default = BTreeMap::from([(
        ModelRole::Strong,
        test_model_with("big", 100_000, ThinkingMode::Switchable),
    )]);
    let prompt = linking(NEEDS_CONTEXT, &[("review.md", NEEDS_CONTEXT)]);
    assert_eq!(
        refusal(prompt, params().with_models(no_default)),
        Some(
            "the environment cannot satisfy this prompt:\n\
             - role 'default': requires a model; the caller's role map has no 'default' entry"
                .to_owned()
        ),
        "no role is filled, so no prompt's role requirement is checked"
    );
}

#[test]
fn a_run_given_no_roster_checks_no_declared_pack_of_a_linked_prompt() {
    let prompt = linking("", &[("review.md", "uses: [web]\n")]);
    assert_eq!(refusal(prompt, params()), None);
    let prompt = linking("", &[("review.md", "uses: [web]\n")]);
    assert_eq!(
        refusal(prompt, params().with_packs([])),
        Some(
            "the environment cannot satisfy this prompt:\n\
             - review.md: missing required Pack: web (at review.md:5)"
                .to_owned()
        ),
        "an empty roster is a roster"
    );
}
