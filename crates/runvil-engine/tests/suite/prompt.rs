//! Reading what a prompt declares before it runs.

use runvil_engine::Prompt;
use runvil_engine::model::{ModelRole, ThinkingMode};
use runvil_engine::{Error, ErrorCode};
use serde_json::json;

use super::prepare::refusal;
use super::support::{Built, Scenario, TestResult, model, prompt};

const CONTRACT: &str = "
    uses: [text]
    args: { type: object, properties: { times: { type: integer, default: 1 }, name: { type: string } }, required: [name] }
    models: { fast: {} }
";

#[test]
fn the_frontmatter_lists_every_declaration_and_gives_its_arguments_as_json_schema() -> TestResult {
    let source = prompt(CONTRACT, &[("Greet", "return 'done'")]);
    let parsed = Prompt::parse(&|_: &str| Ok(source.clone()), "prompt")?;
    let frontmatter = parsed.frontmatter();
    assert_eq!(frontmatter.name(), "greeter");
    assert_eq!(
        frontmatter.description(),
        "A prompt the facade suite builds."
    );

    let packs: Vec<_> = frontmatter
        .uses()
        .iter()
        .map(|entry| (entry.vendor(), entry.id().to_string()))
        .collect();
    assert_eq!(packs, [("runvil", "text".to_owned())]);
    let roles: Vec<_> = ModelRole::ALL
        .into_iter()
        .filter_map(|role| {
            frontmatter
                .role_requirements(role)
                .map(|declared| (role, declared.min_context()))
        })
        .collect();
    assert_eq!(roles, [(ModelRole::Fast, None)]);

    assert_eq!(
        frontmatter.args().as_json(),
        &json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "times": { "type": "integer", "default": 1 },
            },
            "required": ["name"],
            "additionalProperties": false,
        }),
        "the declaration reads back as plain JSON Schema, each closed object saying so"
    );

    let misspelled = source.replace("uses:", "use:");
    let parsed = Prompt::parse(&|_: &str| Ok(misspelled.clone()), "prompt");
    assert_eq!(
        parsed.err().map(|error| error.code),
        Some(ErrorCode::FrontmatterInvalid)
    );
    Ok(())
}

#[test]
fn the_prompt_is_named_by_the_name_its_finder_was_asked_for() -> TestResult {
    let source = prompt(CONTRACT, &[("Greet", "return 'done'")]);
    let parsed = Prompt::parse(&|_: &str| Ok(source.clone()), "greeter.md")?;
    assert_eq!(parsed.file(), "greeter.md");
    Ok(())
}

/// A finder that serves `main` as `greeter.md` and `review` as
/// `review.md`, and has no other prompt.
fn two_prompts(main: String, review: String) -> impl Fn(&str) -> Result<String, Error> {
    move |name: &str| match name {
        "greeter.md" => Ok(main.clone()),
        "review.md" => Ok(review.clone()),
        other => Err(Error::new(
            ErrorCode::NotFound,
            format!("no prompt named {other}"),
            "No prompt has that name.",
        )),
    }
}

#[test]
fn a_parse_finds_each_prompt_the_lists_reach_and_the_prompt_holds_them() -> TestResult {
    let main = prompt("prompts: [review.md]", &[("Greet", "return 'done'")]);
    let review = prompt(
        "uses: [text]\nprompts: [greeter.md]",
        &[("Review", "return 'reviewed'")],
    );
    let parsed = Prompt::parse(&two_prompts(main, review), "greeter.md")?;
    assert_eq!(parsed.frontmatter().prompts(), ["review.md"]);

    let linked: Vec<_> = parsed
        .linked()
        .map(|linked| {
            let packs = linked.frontmatter().uses().len();
            (linked.file(), packs, linked.linked().count())
        })
        .collect();
    assert_eq!(
        linked,
        [("review.md", 1, 0)],
        "the parsed prompt holds the listed prompt, parsed, and is no linked prompt of itself"
    );
    Ok(())
}

#[test]
fn a_listed_prompt_the_finder_lacks_fails_the_parse_where_it_is_listed() {
    let main = prompt(
        "prompts: [review.md, gone.md]",
        &[("Greet", "return 'done'")],
    );
    let review = prompt("", &[("Review", "return 'reviewed'")]);
    let error = Prompt::parse(&two_prompts(main, review), "greeter.md")
        .expect_err("the finder has no gone.md");
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(error.message, "no prompt named gone.md");
    let at = error.at.expect("the failure is placed");
    assert_eq!(
        (at.file.as_str(), at.start_line.get()),
        ("greeter.md", 5),
        "the `prompts:` line of the prompt that lists the name"
    );
}

#[test]
fn a_role_check_by_hand_finds_the_same_shortfall_the_run_reports() -> TestResult {
    let source = prompt(
        "models: { fast: {}, small: { thinking: false, min_context: 100000 } }",
        &[("Greet", "return 'done'")],
    );
    let model = model("canned", 200_000, ThinkingMode::Always)?;
    let scenario = Scenario::default().model(model.clone());
    let Built { prompt, .. } = scenario.build(&source)?;

    let mut misses = Vec::new();
    for role in ModelRole::ALL {
        let Some(declared) = prompt.frontmatter().role_requirements(role) else {
            continue;
        };
        let thinking_miss = match declared.thinking() {
            Some(true) => model.thinking() == ThinkingMode::Never,
            Some(false) => model.thinking() == ThinkingMode::Always,
            None => false,
        };
        let context_miss = declared
            .min_context()
            .is_some_and(|min| min > model.context());
        if thinking_miss || context_miss {
            misses.push((role, thinking_miss, context_miss));
        }
    }
    assert_eq!(misses, [(ModelRole::Small, true, false)]);

    assert_eq!(
        refusal(scenario, &source)?,
        "the environment cannot satisfy this prompt:\n\
         - role 'small': requires 'thinking: false'; \
         the thinking capability of the model filling it is Always (at prompt:5)",
        "only the small role falls short, on its thinking requirement"
    );
    Ok(())
}
