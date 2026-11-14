//! `execute`: a prompt runs another prompt it lists as a chain it waits
//! for. The tests here cover how the executed prompt runs and what comes
//! back; the submodules cover its arguments (`args`), its store
//! (`store`), what each prompt sees of the other (`isolation`), each
//! prompt's Packs and models (`packs`), and where an `execute` may be
//! written (`places`).

use runvil_prompt::test_support::PromptSource;

use super::*;

mod args;
mod isolation;
mod packs;
mod places;
mod store;

/// The source of the prompt `prompt`, which lists `listed` and whose one
/// section, `Main`, runs `lua`.
fn main_listing(listed: &[&str], lua: &str) -> String {
    listing("t", listed).section("Main", lua).build()
}

/// A prompt source named `name` that lists `listed`, with no section yet.
fn listing(name: &str, listed: &[&str]) -> PromptSource {
    let source = PromptSource::new().name(name);
    if listed.is_empty() {
        source
    } else {
        source.front(&format!("prompts: [{}]", listed.join(", ")))
    }
}

/// The source of the prompt named `name` whose one section, `Only`, runs
/// `lua`.
fn child(name: &str, lua: &str) -> String {
    PromptSource::new().name(name).section("Only", lua).build()
}

/// The names of the sections the run started, in order, the preamble as
/// the empty name.
fn started(trace: &Trace) -> Vec<String> {
    trace
        .records()
        .into_iter()
        .filter(|record| record.detail == "Section started")
        .map(|record| record.section)
        .collect()
}

#[test]
fn the_executed_prompt_runs_its_preamble_first_and_then_falls_through_its_sections() {
    let main = main_listing(
        &["child.md"],
        "local text = execute('child.md')
         return 'main got ' .. text",
    );
    let child = PromptSource::new()
        .name("child")
        .preamble("var.trail = 'preamble'")
        .section("First", "var.trail = var.trail .. '>first'")
        .section("Second", "return var.trail .. '>second'")
        .build();
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(trace.text(), "main got preamble>first>second");
    assert_eq!(
        started(&trace),
        ["Main", "", "First", "Second"],
        "the executed prompt's sections report under their own names"
    );
}

#[test]
fn a_return_in_the_executed_prompts_preamble_ends_that_prompt_and_not_its_caller() {
    let main = listing("t", &["child.md"])
        .section("Main", "var.got = execute('child.md')")
        .section("Next", "return 'next after ' .. var.got")
        .build();
    let child = PromptSource::new()
        .name("child")
        .preamble("return 'the preamble'")
        .section("Never", "error('no section runs after the return')")
        .build();
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(trace.text(), "next after the preamble");
}

#[test]
fn an_execute_gives_the_executed_prompts_text_or_nil_when_it_ended_with_none() {
    let main = main_listing(
        &["text.md", "falls.md", "bare.md", "empty.md"],
        "local out = {}
         for _, name in ipairs({ 'text.md', 'falls.md', 'bare.md', 'empty.md' }) do
           out[#out + 1] = name .. '=' .. tostring(execute(name))
         end
         return table.concat(out, ' ')",
    );
    let trace = Scenario::new(main)
        .prompt("text.md", child("text", "return 7"))
        .prompt("falls.md", child("falls", "local unused = 1"))
        .prompt("bare.md", child("bare", "return"))
        .prompt("empty.md", PromptSource::new().name("empty").build())
        .run();
    assert_eq!(
        trace.text(),
        "text.md=7 falls.md=nil bare.md=nil empty.md=nil",
        "a returned value comes back as text, and a prompt that falls off its last section, \
         returns nothing, or has no section gives nil"
    );
}

#[test]
fn a_failed_executed_prompt_raises_in_its_caller_where_pcall_catches_it() {
    let main = main_listing(
        &["child.md"],
        "local ok, err = pcall(execute, 'child.md')
         return tostring(ok) .. '|' .. err.kind .. '|' .. err.message",
    );
    let failing = child("child", "error('the child gave up')");
    let trace = Scenario::new(main).prompt("child.md", failing).run();
    let text = trace.text();
    assert!(
        text.starts_with("false|lua|") && text.contains("the child gave up"),
        "the caller catches the executed prompt's failure: {text}"
    );
}

#[test]
fn an_uncaught_failure_of_the_executed_prompt_ends_the_run_placed_in_that_prompts_file() {
    let main = main_listing(&["child.md"], "return execute('child.md')");
    // The raise is on file line 11 of the executed prompt.
    let failing = child("child", "local fine = true\nerror('the child gave up')");
    let trace = Scenario::new(main).prompt("child.md", failing).run();
    let error = trace.error();
    assert_eq!(error.code, ErrorCode::LuaRuntime, "{error:?}");
    let at = error.at.as_ref().expect("the failure is placed");
    assert_eq!(
        (at.file.as_str(), at.start_line.get()),
        ("child.md", 11),
        "the failure names the executed prompt's file and the line that raised: {error:?}"
    );
}

#[test]
fn a_run_built_over_a_linked_prompt_holds_no_prompt_that_one_lists() {
    // A parse links `c.md` to the prompt it returns, not to `b.md`, so a
    // run built over `b.md` as read from `linked` holds `b.md` alone.
    let main = main_listing(&["b.md"], "return execute('b.md')");
    let b = listing("b", &["c.md", "b.md"])
        .section(
            "Only",
            "if args.text == 'again' then return 'itself' end
             assert(execute('b.md', 'again') == 'itself', 'the run executes the prompt it holds')
             return execute('c.md')",
        )
        .build();
    let finder = move |name: &str| -> std::result::Result<String, crate::Error> {
        Ok(match name {
            "b.md" => b.clone(),
            "c.md" => child("c", "return 'c'"),
            _ => main.clone(),
        })
    };
    let parsed = Prompt::parse(&finder, "prompt").expect("the three prompts parse");
    let linked = parsed
        .linked()
        .find(|linked| linked.file() == "b.md")
        .expect("the parse linked b.md");
    let mut run = Run::new(
        Arc::clone(linked),
        "{}",
        RunParams::new(
            crate::test_support::scenario::TEST_SEED,
            crate::test_support::scenario::TEST_STARTED_AT,
        ),
    );
    let Step::Done { result, .. } = run.step() else {
        panic!("the run issues no effect");
    };
    let error = result.expect_err("the run holds no c.md to execute");
    assert_eq!(error.code, ErrorCode::Internal, "{error:?}");
    assert!(
        error
            .message
            .contains("the run holds no prompt `c.md`, which `b.md` lists"),
        "the run executes the prompt it was built over, and no other: {error:?}"
    );
}

#[test]
fn the_executed_prompt_starts_on_a_var_of_its_own_and_leaves_its_callers_var_and_args_alone() {
    let main = main_listing(
        &["child.md"],
        "var.mine = 'kept'
         local seen = execute('child.md', { topic = 'tea' })
         return seen .. ' / ' .. var.mine .. ' ' .. tostring(var.theirs) .. ' ' .. args.text",
    );
    let child = PromptSource::new()
        .name("child")
        .front("args: { type: object, properties: { topic: { type: string } } }")
        .section(
            "Only",
            "local before = tostring(var.mine)
             var.theirs = 'set by the child'
             var.mine = 'changed by the child'
             return before .. ' ' .. args.topic .. ' ' .. tostring(args.text)",
        )
        .build();
    let trace = Scenario::new(main)
        .args(r#"{"text":"caller text"}"#)
        .prompt("child.md", child)
        .run();
    assert_eq!(
        trace.text(),
        "nil tea nil / kept nil caller text",
        "the executed prompt reads none of its caller's `var` or `args`, and the caller reads \
         none of its writes"
    );
}
