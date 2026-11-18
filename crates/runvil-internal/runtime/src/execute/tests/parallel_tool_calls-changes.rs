//! A call to a tool that changes files runs alone: the batch stops before
//! it, so the calls the model listed before it are answered first, it is
//! issued by itself, and the calls listed after it are issued once it has
//! its answer. The calls on either side of it still run together.

use super::*;
use crate::execute::tests::fixtures::{echo_tool, scoped_tool};
use crate::execute::tests::models_loop::loop_scenario;

/// `tools/edit`, which answers its `value` argument and whose calls
/// change files.
#[track_caller]
fn edit_tool() -> FnTool {
    let (tool, _calls) = scoped_tool("edit", "Edit the file the value names.");
    tool.changes_files()
}

/// The `value` argument of each call a step issued, in issue order.
fn values(calls: &[(EffectId, Effect)]) -> Vec<&str> {
    calls.iter().map(|(_, effect)| call_value(effect)).collect()
}

#[test]
fn a_call_that_changes_files_is_issued_alone_between_the_calls_listed_around_it() {
    let md = one_section(
        "tools.offer('tools/echo')\n\
         tools.offer('tools/edit')\n\
         messages:user('use them all')\n\
         models.loop()\n\
         local ids = {}\n\
         for i = 3, 8 do ids[#ids + 1] = messages[i].tool_call_id end\n\
         return table.concat(ids, ',')",
    );
    let mut run = loop_scenario(md)
        .offer("echo", echo_tool())
        .offer("edit", edit_tool())
        .into_run();
    let (render, effect) = only_effect(run.step());
    run.resume(render, pass_through(&effect));
    let (round, _) = only_effect(run.step());
    run.resume(
        round,
        calls_reply(&[
            ("c1", "echo", json!({ "value": "a" })),
            ("c2", "echo", json!({ "value": "b" })),
            ("c3", "edit", json!({ "value": "c" })),
            ("c4", "edit", json!({ "value": "d" })),
            ("c5", "echo", json!({ "value": "e" })),
            ("c6", "echo", json!({ "value": "f" })),
        ]),
    );

    let (before, _) = pending(run.step());
    assert_eq!(
        values(&before),
        ["a", "b"],
        "the calls listed before the first change are issued together, and no change with them"
    );
    run.resume(before[1].0, tool_text("echoed b"));
    let (issued, _) = pending(run.step());
    assert!(
        issued.is_empty(),
        "the first change waits for every call listed before it: {issued:?}"
    );
    run.resume(before[0].0, tool_text("echoed a"));

    let (first, effect) = only_effect(run.step());
    assert_eq!(call_value(&effect), "c", "the first change is issued alone");
    run.resume(first, tool_text("edited c"));
    let (second, effect) = only_effect(run.step());
    assert_eq!(
        call_value(&effect),
        "d",
        "the second change is issued alone, once the first has its answer"
    );
    run.resume(second, tool_text("edited d"));

    let (after, _) = pending(run.step());
    assert_eq!(
        values(&after),
        ["e", "f"],
        "the calls listed after the last change are issued together, once it has its answer"
    );
    for (id, _) in &after {
        run.resume(*id, tool_text("echoed"));
    }
    let (next, effect) = only_effect(run.step());
    assert_eq!(
        tool_messages(&effect),
        [
            "echoed a", "echoed b", "edited c", "edited d", "echoed", "echoed"
        ],
        "every result reaches the round in the model's order"
    );
    run.resume(next, chat_answer(&text("done")));
    let result = match run.step() {
        Step::Done { result, .. } => result,
        other @ Step::Pending { .. } => panic!("the reply ends the section: {other:?}"),
    };
    assert_eq!(text_of(result), "c1,c2,c3,c4,c5,c6");
}

#[test]
fn a_script_calls_a_tool_that_changes_files_as_it_calls_any_other() {
    let md = one_section("return tools.call('tools/edit', { value = 'x' })");
    let mut run = loop_scenario(md).offer("edit", edit_tool()).into_run();
    let (call, effect) = only_effect(run.step());
    assert_eq!(call_value(&effect), "x");
    run.resume(call, tool_text("edited x"));
    let result = match run.step() {
        Step::Done { result, .. } => result,
        other @ Step::Pending { .. } => panic!("the call's answer ends the section: {other:?}"),
    };
    assert_eq!(text_of(result), "edited x");
}
