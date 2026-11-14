//! What each prompt of a run reaches of the other: nothing but the
//! `execute` itself. A section path names a section of the prompt it is
//! written in, so one name in both prompts is two sections, and the
//! executed prompt's `call`, `jump`, `require`, `tasks.spawn`, and
//! `prompt` reads find no section of its caller. A prompt executes only
//! the prompts its own `prompts:` key lists, and prompts that execute
//! each other stop at the call-depth cap.

use super::*;

/// The prose every `LuaRuntime` failure ends with.
fn lua_prose() -> &'static str {
    ErrorCode::LuaRuntime.prose()
}

#[test]
fn one_section_name_in_both_prompts_names_each_prompts_own_section() {
    let main = listing("t", &["child.md"])
        .section(
            "Main",
            "return call('../Helper') .. ' | ' .. execute('child.md') .. ' | '
               .. call('../Helper') .. ' | ' .. prompt.section('/Notes')",
        )
        .section("Helper", "return 'caller helper'")
        .prose("Notes", "Caller notes.")
        .build();
    let child = PromptSource::new()
        .name("child")
        .section(
            "Main",
            "return call('../Helper') .. ' + ' .. prompt.section('/Notes')",
        )
        .section("Helper", "return 'child helper'")
        .prose("Notes", "Child notes.")
        .build();
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(
        trace.text(),
        "caller helper | child helper + Child notes. | caller helper | Caller notes."
    );
}

#[test]
fn the_executed_prompt_reaches_no_section_of_its_caller() {
    let main = listing("t", &["child.md"])
        .section("Main", "return execute('child.md')")
        .section("Secret", "return 'the caller keeps this'")
        .build();
    let child = child(
        "child",
        "local lines = {}
         for _, try in ipairs({
           { 'call', call, '../Secret' },
           { 'spawn', tasks.spawn, '../Secret' },
           { 'read', prompt.section, '/Secret' },
           { 'read sibling', prompt.section, '../Secret' },
           { 'require', require, '/Secret' },
         }) do
           local ok, err = pcall(try[2], try[3])
           lines[#lines + 1] = try[1] .. ': ' .. tostring(ok) .. ' ' .. tostring(err)
         end
         lines[#lines + 1] = 'read or: ' .. tostring(prompt.section_or('/Secret'))
         return table.concat(lines, '\\n')",
    );
    let trace = Scenario::new(main).prompt("child.md", child).run();
    let text = trace.text();
    let lines: Vec<&str> = text.lines().collect();
    let [called, spawned, read, sibling, required, read_or] = lines.as_slice() else {
        panic!("six lines: {text}");
    };
    // The search is of the executed prompt's own sections: a sibling of
    // its section `Only`, or its top level.
    let no_sibling = "section `../Secret` not found: `/Only` has no sibling `Secret`";
    let no_top = "section `/Secret` not found: the top level has no section `Secret`";
    for (line, miss) in [
        (called, no_sibling),
        (spawned, no_sibling),
        (read, no_top),
        (sibling, no_sibling),
        (required, no_top),
    ] {
        assert!(
            line.contains(&format!("false child.md:18: {miss}")),
            "the miss is placed in the executed prompt's own file: {line}"
        );
    }
    assert_eq!(*read_or, "read or: nil");
}

#[test]
fn a_jump_in_the_executed_prompt_to_a_section_of_its_caller_fails_that_prompt() {
    let main = listing("t", &["child.md"])
        .section(
            "Main",
            "local ok, err = pcall(execute, 'child.md')
             return tostring(ok) .. '|' .. err.message",
        )
        .section("Secret", "return 'the caller keeps this'")
        .build();
    let child = child("child", "jump('../Secret')");
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(
        trace.text(),
        format!(
            "false|child.md:10: section `../Secret` not found: `/Only` has no sibling `Secret`. \
             {}",
            lua_prose()
        )
    );
}

#[test]
fn a_prompt_executes_only_the_prompts_its_own_list_names() {
    // The run's prompt lists `b.md`, which lists `c.md`, which lists
    // nothing: `b.md` may execute `c.md`, the run's prompt may not, and
    // `c.md` may execute no prompt, though the run holds all three.
    let main = main_listing(
        &["b.md"],
        "local via_b = execute('b.md')
         local ok, err = pcall(execute, 'c.md')
         return via_b .. '|' .. tostring(ok) .. '|' .. err.message",
    );
    let b = listing("b", &["c.md"])
        .section("Only", "return 'b got ' .. execute('c.md')")
        .build();
    let c = child(
        "c",
        "local ok, err = pcall(execute, 'b.md')
         return 'c (' .. tostring(ok) .. ' ' .. err.message .. ')'",
    );
    let trace = Scenario::new(main)
        .prompt("b.md", b)
        .prompt("c.md", c)
        .run();
    assert_eq!(
        trace.text(),
        format!(
            "b got c (false c.md:10: prompt `b.md` is not listed: `c.md` lists no prompt under \
             `prompts:`. {prose})|false|prompt:12: prompt `c.md` is not listed: `prompt` lists \
             `b.md` under `prompts:`. {prose}",
            prose = lua_prose()
        )
    );
}

#[test]
fn a_name_the_caller_does_not_list_raises_naming_every_name_it_lists() {
    let main = main_listing(
        &["b.md", "c.md"],
        "local ok, err = pcall(execute, 'd.md')
         return err.kind .. '|' .. err.code .. '|' .. err.message",
    );
    let trace = Scenario::new(main)
        .prompt("b.md", child("b", "return 'b'"))
        .prompt("c.md", child("c", "return 'c'"))
        .run();
    assert_eq!(
        trace.text(),
        format!(
            "lua|lua_runtime|prompt:11: prompt `d.md` is not listed: `prompt` lists `b.md`, \
             `c.md` under `prompts:`. {}",
            lua_prose()
        )
    );
}

/// A section body that executes `other` one level deeper, passing its
/// depth as text, and reports the depth at which the `execute` is
/// refused.
fn descending(other: &str) -> String {
    format!(
        "local depth = tonumber(args.text or '0')
         local ok, deeper = pcall(execute, '{other}', tostring(depth + 1))
         if ok then return deeper end
         return 'depth ' .. depth .. ' stopped: ' .. deeper.code"
    )
}

#[test]
fn two_prompts_that_execute_each_other_stop_at_the_call_depth_cap() {
    // The run's prompt is found under `prompt`, the name `b.md` lists.
    let main = main_listing(&["b.md"], &descending("b.md"));
    let b = listing("b", &["prompt"])
        .section("Only", &descending("prompt"))
        .build();
    let trace = Scenario::new(main).prompt("b.md", b).run();
    assert_eq!(
        trace.text(),
        "depth 8 stopped: call_depth_exceeded",
        "each execute runs one level deeper, and the ninth level is refused"
    );
}

#[test]
fn a_prompt_that_lists_itself_executes_itself_until_the_call_depth_cap() {
    let main = main_listing(&["prompt"], &descending("prompt"));
    let trace = Scenario::new(main).run();
    assert_eq!(trace.text(), "depth 8 stopped: call_depth_exceeded");
}

#[test]
fn an_execute_past_the_call_depth_cap_ends_the_run_when_no_prompt_catches_it() {
    let deeper = |other: &str| format!("return execute('{other}')");
    let main = main_listing(&["b.md"], &deeper("b.md"));
    let b = listing("b", &["prompt"])
        .section("Only", &deeper("prompt"))
        .build();
    let trace = Scenario::new(main).prompt("b.md", b).run();
    let error = trace.error();
    assert_eq!(error.code, ErrorCode::CallDepthExceeded, "{error:?}");
}

#[test]
fn a_call_and_an_execute_count_toward_the_one_call_depth_cap() {
    // Each level is one `call` and one `execute`, so the cap of 8 is spent
    // after four of each.
    let main = listing("t", &["prompt"])
        .section(
            "Main",
            "local depth = tonumber(args.text or '0')
             local ok, deeper = pcall(call, '../Again', tostring(depth + 1))
             if ok then return deeper end
             return 'call refused at depth ' .. depth",
        )
        .section(
            "Again",
            "local depth = tonumber(args.text)
             local ok, deeper = pcall(execute, 'prompt', tostring(depth + 1))
             if ok then return deeper end
             return 'execute refused at depth ' .. depth",
        )
        .build();
    let trace = Scenario::new(main).run();
    assert_eq!(trace.text(), "call refused at depth 8");
}
