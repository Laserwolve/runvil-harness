//! Running a prompt from another prompt with `execute`.

use std::collections::BTreeMap;

use runvil_engine::ErrorCode;
use runvil_engine::ids::ChainId;
use serde_json::{Value, json};

use super::support::{Scenario, TestResult, prompt};

/// Declares a required string `name` and an integer `times` whose default
/// is 1.
const DECLARED: &str = "args: { type: object, properties: { name: { type: string }, times: { type: integer, default: 1 } }, required: [name] }";

/// The prompt `greet.md`: it takes [`DECLARED`] and returns a greeting.
fn greet() -> String {
    prompt(
        DECLARED,
        &[(
            "Greet",
            "return 'hello ' .. args.name .. ' x' .. args.times",
        )],
    )
}

/// A prompt that lists `greet.md` and whose one section runs `lua`.
fn listing_greet(lua: &str) -> String {
    prompt("prompts: [greet.md]", &[("Plan", lua)])
}

#[test]
fn a_prompt_executes_a_prompt_it_lists_and_gets_its_text() -> TestResult {
    let source = listing_greet(
        "local one = execute('greet.md', { name = 'Ada' })
         local two = execute('greet.md', { name = 'Bo', times = 2 })
         return one .. ', ' .. two",
    );
    let trace = Scenario::default()
        .prompt("greet.md", greet())
        .run(&source)?;
    assert_eq!(trace.text()?, "hello Ada x1, hello Bo x2");
    Ok(())
}

#[test]
fn arguments_the_executed_prompt_refuses_raise_in_the_caller_and_end_the_run_when_uncaught()
-> TestResult {
    let caught = listing_greet(
        "local ok, err = pcall(execute, 'greet.md', { times = 2 })
         return tostring(ok) .. ' ' .. err.code",
    );
    let trace = Scenario::default()
        .prompt("greet.md", greet())
        .run(&caught)?;
    assert_eq!(trace.text()?, "false args_invalid");

    let uncaught = listing_greet("return execute('greet.md', { name = 7 })");
    let trace = Scenario::default()
        .prompt("greet.md", greet())
        .run(&uncaught)?;
    let error = trace.error()?;
    assert_eq!(error.code, ErrorCode::ArgsInvalid, "{error:?}");
    assert_eq!(
        error.located_message(),
        "argument `name` must be a string, got integer (at prompt:11)",
        "the failure is placed on the caller's `execute`"
    );
    Ok(())
}

#[test]
fn a_prompt_that_is_not_listed_is_not_executed() -> TestResult {
    let source = prompt(
        "",
        &[(
            "Plan",
            "local ok, err = pcall(execute, 'greet.md', { name = 'Ada' })
             return tostring(ok) .. ' ' .. err.code",
        )],
    );
    let trace = Scenario::default().run(&source)?;
    assert_eq!(trace.text()?, "false lua_runtime");
    Ok(())
}

#[test]
fn each_store_effect_names_its_store_and_the_two_prompts_share_no_file() -> TestResult {
    let source = listing_greet(
        "store.write('note.md', 'plan')
         local greeted = execute('greet.md', { name = 'Ada' })
         return greeted .. ' | ' .. store.read('note.md')",
    );
    let keeps_a_note = prompt(
        DECLARED,
        &[(
            "Greet",
            "local had = tostring(store.exists('note.md'))
             store.write('note.md', 'greet ' .. args.name)
             return had .. ' ' .. store.read('note.md')",
        )],
    );
    let trace = Scenario::default()
        .prompt("greet.md", keeps_a_note)
        .run(&source)?;
    assert_eq!(trace.text()?, "false greet Ada | plan");

    // The paths each store's records wrote, by the store's id read back
    // as a chain id.
    let mut written: BTreeMap<ChainId, Vec<Value>> = BTreeMap::new();
    for logged in &trace.effects {
        let Some(vfs) = logged.effect_of("Vfs") else {
            continue;
        };
        let store: ChainId = serde_json::from_value(vfs["store"].clone())?;
        if let Some(write) = vfs["op"].get("Write") {
            written.entry(store).or_default().push(write.clone());
        }
    }
    let root = ChainId::root();
    assert_eq!(
        written,
        BTreeMap::from([
            (
                root.clone(),
                vec![json!({ "path": "note.md", "contents": "plan" })]
            ),
            (
                root.child(0),
                vec![json!({ "path": "note.md", "contents": "greet Ada" })]
            ),
        ]),
        "the run's own store is the root chain's, and the executed prompt's is its chain's"
    );
    Ok(())
}
