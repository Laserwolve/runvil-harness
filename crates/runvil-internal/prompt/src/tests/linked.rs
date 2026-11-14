//! Tests for the `prompts` key and the prompts a parse reaches through it:
//! which prompts are found, how often and in what order, what a parsed
//! prompt holds of them, and how a list or a listed prompt fails.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::*;
use crate::PromptFinder;

/// A finder over named sources that logs each name it is asked for.
struct Sources {
    sources: BTreeMap<&'static str, String>,
    asked: Mutex<Vec<String>>,
}

impl Sources {
    /// A finder that serves each `(name, source)` pair.
    fn new(sources: &[(&'static str, String)]) -> Sources {
        Sources {
            sources: sources.iter().cloned().collect(),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// Every name the finder was asked for, in order.
    fn asked(&self) -> Vec<String> {
        self.asked
            .lock()
            .expect("the finder's log is not poisoned")
            .clone()
    }
}

impl PromptFinder for Sources {
    fn find(&self, name: &str) -> std::result::Result<String, Error> {
        self.asked
            .lock()
            .expect("the finder's log is not poisoned")
            .push(name.to_owned());
        self.sources.get(name).cloned().ok_or_else(|| missing(name))
    }
}

/// The failure of a find for a name the finder has no source for, which
/// has no location.
fn missing(name: &str) -> Error {
    Error::new(
        ErrorCode::NotFound,
        format!("no prompt named {name}"),
        "No prompt has that name.",
    )
}

/// The source of the prompt whose frontmatter name is `name` and whose
/// `prompts:` key, on file line 5, lists `listed`.
fn listing(name: &str, listed: &[&str]) -> String {
    PromptSource::new()
        .name(name)
        .front(&format!("prompts: [{}]", listed.join(", ")))
        .prose("Only", "text")
        .build()
}

/// The file name of each prompt `prompt` links, in the order it gives
/// them.
fn linked_names(prompt: &Prompt) -> Vec<&str> {
    prompt.linked().map(|linked| linked.file()).collect()
}

#[test]
fn a_prompt_with_no_prompts_key_lists_nothing_and_links_nothing() {
    let prompt = parse_ok(&PromptSource::new().prose("Only", "text").build());
    assert!(prompt.frontmatter().prompts().is_empty());
    assert_eq!(prompt.linked().count(), 0);
}

#[test]
fn the_frontmatter_gives_its_prompts_in_declaration_order() {
    let sources = Sources::new(&[
        ("a.md", listing("a", &["c.md", "b.md"])),
        ("b.md", listing("b", &[])),
        ("c.md", listing("c", &[])),
    ]);
    let prompt = Prompt::parse(&sources, "a.md").expect("the three prompts parse");
    assert_eq!(prompt.frontmatter().prompts(), ["c.md", "b.md"]);
}

#[test]
fn a_chain_of_three_is_parsed_through_each_list() {
    let sources = Sources::new(&[
        ("a.md", listing("a", &["b.md"])),
        ("b.md", listing("b", &["c.md"])),
        ("c.md", listing("c", &[])),
    ]);
    let prompt = Prompt::parse(&sources, "a.md").expect("the chain parses");
    assert_eq!(prompt.file(), "a.md");
    assert_eq!(
        linked_names(&prompt),
        ["b.md", "c.md"],
        "the main prompt holds the prompt it lists and the prompt that one lists"
    );
    let names: Vec<&str> = prompt
        .linked()
        .map(|linked| linked.frontmatter().name())
        .collect();
    assert_eq!(names, ["b", "c"], "each is parsed from its own source");
    assert_eq!(sources.asked(), ["a.md", "b.md", "c.md"]);
}

#[test]
fn a_linked_prompt_holds_no_linked_prompts_of_its_own() {
    let sources = Sources::new(&[
        ("a.md", listing("a", &["b.md"])),
        ("b.md", listing("b", &["c.md"])),
        ("c.md", listing("c", &[])),
    ]);
    let prompt = Prompt::parse(&sources, "a.md").expect("the chain parses");
    for linked in prompt.linked() {
        assert_eq!(
            linked.linked().count(),
            0,
            "{} keeps its list and holds no prompt",
            linked.file()
        );
    }
    let b = prompt.linked().next().expect("a.md links b.md");
    assert_eq!(b.frontmatter().prompts(), ["c.md"]);
}

#[test]
fn a_cycle_back_to_the_main_prompt_finds_it_once() {
    let sources = Sources::new(&[
        ("a.md", listing("a", &["b.md"])),
        ("b.md", listing("b", &["a.md"])),
    ]);
    let prompt = Prompt::parse(&sources, "a.md").expect("the cycle parses");
    assert_eq!(
        linked_names(&prompt),
        ["b.md"],
        "the main prompt is no linked prompt of itself"
    );
    assert_eq!(sources.asked(), ["a.md", "b.md"]);
}

#[test]
fn a_cycle_between_listed_prompts_finds_each_once() {
    let sources = Sources::new(&[
        ("a.md", listing("a", &["b.md"])),
        ("b.md", listing("b", &["c.md"])),
        ("c.md", listing("c", &["b.md", "c.md"])),
    ]);
    let prompt = Prompt::parse(&sources, "a.md").expect("the cycle parses");
    assert_eq!(linked_names(&prompt), ["b.md", "c.md"]);
    assert_eq!(sources.asked(), ["a.md", "b.md", "c.md"]);
}

#[test]
fn a_prompt_that_two_lists_name_is_parsed_once() {
    let sources = Sources::new(&[
        ("a.md", listing("a", &["b.md", "c.md"])),
        ("b.md", listing("b", &["d.md"])),
        ("c.md", listing("c", &["d.md"])),
        ("d.md", listing("d", &[])),
    ]);
    let prompt = Prompt::parse(&sources, "a.md").expect("the diamond parses");
    assert_eq!(linked_names(&prompt), ["b.md", "c.md", "d.md"]);
    assert_eq!(sources.asked(), ["a.md", "b.md", "c.md", "d.md"]);
}

#[test]
fn a_prompt_that_lists_itself_links_nothing() {
    let sources = Sources::new(&[("a.md", listing("a", &["a.md"]))]);
    let prompt = Prompt::parse(&sources, "a.md").expect("the prompt parses");
    assert_eq!(prompt.frontmatter().prompts(), ["a.md"]);
    assert_eq!(prompt.linked().count(), 0);
    assert_eq!(sources.asked(), ["a.md"]);
}

#[test]
fn the_lists_are_read_in_the_order_the_parse_reached_their_prompts() {
    // `a` lists `c` before `b`, so `c`'s list is read before `b`'s, and
    // the linked prompts still come back in name order.
    let sources = Sources::new(&[
        ("a.md", listing("a", &["c.md", "b.md"])),
        ("b.md", listing("b", &["d.md"])),
        ("c.md", listing("c", &["e.md"])),
        ("d.md", listing("d", &[])),
        ("e.md", listing("e", &[])),
    ]);
    let prompt = Prompt::parse(&sources, "a.md").expect("the tree parses");
    assert_eq!(sources.asked(), ["a.md", "c.md", "b.md", "e.md", "d.md"]);
    assert_eq!(linked_names(&prompt), ["b.md", "c.md", "d.md", "e.md"]);
}

#[test]
fn two_parses_of_the_same_sources_are_equal_and_a_changed_linked_prompt_is_not() {
    let sources =
        |b: &str| Sources::new(&[("a.md", listing("a", &["b.md"])), ("b.md", listing(b, &[]))]);
    let first = Prompt::parse(&sources("b"), "a.md").expect("the prompts parse");
    let second = Prompt::parse(&sources("b"), "a.md").expect("the prompts parse");
    assert_eq!(first, second);
    let other = Prompt::parse(&sources("other"), "a.md").expect("the prompts parse");
    assert_ne!(first, other, "the linked prompts are part of the value");
}

#[test]
fn an_empty_entry_is_refused_on_the_prompts_line() {
    let error = parse_err(&listing("a", &["b.md", "\"\""]));
    assert_eq!(error.code, ErrorCode::FrontmatterInvalid);
    assert_eq!(
        error.message,
        "invalid frontmatter: a prompts entry must not be empty"
    );
    assert_eq!(location(&error), Some(("prompt", 5, 1, 6, 1)));
    assert_eq!(
        error.facts,
        BTreeMap::from([(ErrorFact::Reason, "prompt_empty".into())])
    );
}

#[test]
fn a_name_listed_twice_is_refused_on_the_prompts_line() {
    let error = parse_err(&listing("a", &["b.md", "c.md", "b.md"]));
    assert_eq!(error.code, ErrorCode::FrontmatterInvalid);
    assert_eq!(
        error.message,
        "invalid frontmatter: prompt b.md is listed more than once under prompts"
    );
    assert_eq!(location(&error), Some(("prompt", 5, 1, 6, 1)));
    assert_eq!(
        error.facts,
        BTreeMap::from([
            (ErrorFact::Reason, "prompt_duplicate".into()),
            (ErrorFact::Name, "b.md".into()),
        ]),
        "the repeated name is the name, not the prompt's"
    );
}

#[test]
fn a_prompts_key_that_is_no_list_of_names_fails_to_decode() {
    for value in ["b.md", "{ b.md: 1 }", "[[b.md]]"] {
        let source = PromptSource::new()
            .front(&format!("prompts: {value}"))
            .prose("Only", "text")
            .build();
        let error = parse_err(&source);
        assert_eq!(
            error.code,
            ErrorCode::FrontmatterInvalid,
            "{value}: {error}"
        );
        assert!(
            error.message.starts_with("invalid frontmatter: prompts"),
            "the decode failure names the key: {value}: {error}"
        );
    }
}

#[test]
fn a_listed_prompt_the_finder_lacks_fails_the_parse_on_the_listing_prompts_line() {
    let sources = Sources::new(&[
        ("a.md", listing("a", &["b.md"])),
        ("b.md", listing("b", &["gone.md"])),
    ]);
    let error = Prompt::parse(&sources, "a.md").expect_err("gone.md is not found");
    let mut expected = missing("gone.md");
    expected.at = Some(runvil_types::error::SourceLoc::line(
        "b.md",
        std::num::NonZeroU32::new(5).expect("5 is non-zero"),
    ));
    assert_eq!(
        error, expected,
        "the finder's failure, placed where the name was listed"
    );
    assert_eq!(sources.asked(), ["a.md", "b.md", "gone.md"]);
}

#[test]
fn a_finder_failure_for_a_listed_prompt_keeps_the_location_it_has() {
    let mut refusal = missing("b.md");
    refusal.at = Some(runvil_types::error::SourceLoc::line(
        "prompts.toml",
        std::num::NonZeroU32::new(3).expect("3 is non-zero"),
    ));
    let main = listing("a", &["b.md"]);
    let finder = |name: &str| match name {
        "a.md" => Ok(main.clone()),
        _ => Err(refusal.clone()),
    };
    let error = Prompt::parse(&finder, "a.md").expect_err("b.md is not found");
    assert_eq!(error, refusal);
}

#[test]
fn a_finder_failure_for_a_list_that_no_line_declares_has_no_location() {
    // A flow mapping declares `prompts` on no line of its own.
    let main = "---\n{ name: a, description: d, prompts: [b.md] }\n---\n\n## Only\n\ntext\n";
    let finder = |name: &str| match name {
        "a.md" => Ok(main.to_owned()),
        other => Err(missing(other)),
    };
    let error = Prompt::parse(&finder, "a.md").expect_err("b.md is not found");
    assert_eq!(error, missing("b.md"));
}

#[test]
fn a_listed_prompt_that_fails_to_parse_fails_the_parse_in_its_own_file() {
    let broken = "---\nname: b\ndescription: d\n---\n\n## S\n\n```lua\nreturn 1\n";
    let sources = Sources::new(&[
        ("a.md", listing("a", &["b.md", "c.md"])),
        ("b.md", broken.to_owned()),
        ("c.md", listing("c", &[])),
    ]);
    let error = Prompt::parse(&sources, "a.md").expect_err("b.md has an open fence");
    assert_eq!(error.code, ErrorCode::FenceInvalid, "{error:?}");
    assert_eq!(location(&error), Some(("b.md", 8, 1, 9, 1)));
    assert_eq!(
        error.facts.get(&ErrorFact::Name),
        Some(&"b".into()),
        "the failure names the listed prompt, not the one that lists it"
    );
    assert_eq!(
        sources.asked(),
        ["a.md", "b.md"],
        "the parse stops at the first failure"
    );
}

#[test]
fn a_refused_list_of_a_listed_prompt_is_located_in_that_prompt() {
    let sources = Sources::new(&[
        ("a.md", listing("a", &["b.md"])),
        ("b.md", listing("b", &["c.md", "c.md"])),
    ]);
    let error = Prompt::parse(&sources, "a.md").expect_err("b.md lists c.md twice");
    assert_eq!(error.code, ErrorCode::FrontmatterInvalid);
    assert_eq!(location(&error), Some(("b.md", 5, 1, 6, 1)));
    assert_eq!(
        error.prose,
        "b.md:5: invalid frontmatter: prompt c.md is listed more than once under prompts. The \
         frontmatter is invalid. Fix the YAML between the `---` lines so it matches the \
         frontmatter schema."
    );
}
