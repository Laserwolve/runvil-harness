//! The arguments an `execute` passes: a table of named fields or a
//! string, checked against the executed prompt's `args` declaration, which
//! fills its defaults; none is the empty object. A value the declaration
//! refuses raises `args_invalid` in the caller before the executed prompt
//! starts, and neither that refusal nor a name the caller does not list
//! takes a chain id.

use super::*;

/// The source of `search.md`: it requires a string `query`, defaults an
/// integer `limit` to 3, and returns both.
fn search() -> String {
    // `front` trims each line, so the declaration is one flow mapping.
    PromptSource::new()
        .name("search")
        .front(
            "args: { type: object, properties: { query: { type: string }, \
             limit: { type: integer, default: 3 } }, required: [query] }",
        )
        .section("Only", "return args.query .. '|' .. args.limit")
        .build()
}

/// The source of `opts.md`: it declares one optional integer `depth`,
/// which defaults to 2, and returns it.
fn opts() -> String {
    PromptSource::new()
        .name("opts")
        .front("args: { type: object, properties: { depth: { type: integer, default: 2 } } }")
        .section("Only", "return 'depth ' .. args.depth")
        .build()
}

/// The source of `echo.md`: it has no `args:` key and returns its `text`
/// argument, or `nil` when it has none.
fn echo() -> String {
    child("echo", "return tostring(args.text)")
}

/// A scenario over a prompt that lists `search.md`, `opts.md`, and
/// `echo.md` and whose one section runs `lua`.
fn over_three(lua: &str) -> Scenario {
    Scenario::new(main_listing(&["search.md", "opts.md", "echo.md"], lua))
        .prompt("search.md", search())
        .prompt("opts.md", opts())
        .prompt("echo.md", echo())
}

#[test]
fn a_table_of_named_fields_is_the_executed_prompts_arguments_with_its_defaults_filled() {
    let trace = over_three(
        "return execute('search.md', { query = 'tea' })
           .. ' ' .. execute('search.md', { query = 'tea', limit = 7 })",
    )
    .run();
    assert_eq!(trace.text(), "tea|3 tea|7");
}

#[test]
fn a_string_is_the_one_field_text_of_the_executed_prompts_arguments() {
    let trace = over_three(
        "local ok, err = pcall(execute, 'opts.md', 'hello')
         return execute('echo.md', 'hello') .. ' ' .. execute('echo.md', { text = 'named' })
           .. ' ' .. tostring(ok) .. ' ' .. err.code",
    )
    .run();
    assert_eq!(
        trace.text(),
        "hello named false args_invalid",
        "a prompt that declares no `text` refuses a string"
    );
}

#[test]
fn an_execute_that_passes_no_arguments_gives_the_empty_object_and_the_declared_defaults() {
    let trace = over_three(
        "local ok, err = pcall(execute, 'search.md')
         return execute('opts.md') .. ' ' .. execute('echo.md') .. ' ' .. tostring(ok)
           .. ' ' .. err.code",
    )
    .args(r#"{"text":"the caller's own"}"#)
    .run();
    assert_eq!(
        trace.text(),
        "depth 2 nil false args_invalid",
        "the executed prompt reads none of its caller's arguments, and one with a required \
         field refuses the empty object"
    );
}

#[test]
fn the_callers_own_args_pass_on_when_the_caller_gives_them() {
    let trace = over_three("return execute('echo.md', args)")
        .args(r#"{"text":"the caller's own"}"#)
        .run();
    assert_eq!(trace.text(), "the caller's own");
}

#[test]
fn a_prompt_with_no_args_key_takes_a_text_or_nothing_and_refuses_any_other_field() {
    let trace = over_three(
        "local ok, err = pcall(execute, 'echo.md', { query = 'tea' })
         return tostring(ok) .. '|' .. err.code .. '|' .. err.message",
    )
    .run();
    assert_eq!(
        trace.text(),
        format!(
            "false|args_invalid|prompt:11: argument `query` is not declared; the declared \
             arguments are `text`. {}",
            ErrorCode::ArgsInvalid.prose()
        )
    );
}

#[test]
fn each_refused_argument_raises_args_invalid_in_the_caller_and_the_executed_prompt_does_not_start()
{
    for (given, message) in [
        ("{ limit = 1 }", "argument `query` is required"),
        (
            "{ query = 5 }",
            "argument `query` must be a string, got integer",
        ),
        (
            "{ query = 'tea', limit = 'many' }",
            "argument `limit` must be an integer, got string",
        ),
        (
            "{ query = 'tea', extra = true }",
            "argument `extra` is not declared; the declared arguments are `limit`, `query`",
        ),
    ] {
        let trace = over_three(&format!(
            "local ok, err = pcall(execute, 'search.md', {given})
             return tostring(ok) .. '|' .. err.kind .. '|' .. err.code .. '|' .. err.message"
        ))
        .run();
        assert_eq!(
            trace.text(),
            format!(
                "false|lua|args_invalid|prompt:11: {message}. {}",
                ErrorCode::ArgsInvalid.prose()
            ),
            "{given}"
        );
        assert_eq!(
            started(&trace),
            ["Main"],
            "{given}: no section of the executed prompt started"
        );
    }
}

#[test]
fn an_uncaught_refusal_ends_the_run_as_args_invalid_placed_on_the_callers_execute() {
    let trace = over_three("local unused = 1\nreturn execute('search.md', { query = 5 })").run();
    let error = trace.error();
    assert_eq!(error.code, ErrorCode::ArgsInvalid, "{error:?}");
    assert_eq!(
        error.located_message(),
        "argument `query` must be a string, got integer (at prompt:12)",
        "the failure names the caller's line, since the caller passed the arguments"
    );
    assert_eq!(
        (
            error.facts.get(&runvil_types::error::ErrorFact::Reason),
            error.facts.get(&runvil_types::error::ErrorFact::Name),
        ),
        (Some(&json!("type")), Some(&json!("query"))),
        "the failure keeps the check's facts"
    );
}

#[test]
fn an_execute_refused_before_its_chain_starts_takes_no_chain_id() {
    // Each refusal comes before the executed chain takes its id, so the
    // one execute that starts a chain is the caller's first child, `0.0`,
    // whose first section entry is `0.0.0`.
    let main = main_listing(
        &["search.md", "id.md"],
        "local refused = 0
         for _, try in ipairs({
           function() return execute('search.md') end,
           function() return execute('search.md', { query = 5 }) end,
           function() return execute('unlisted.md') end,
           function() return execute(7) end,
           function() return execute('id.md', { 1, 2 }) end,
         }) do
           if not pcall(try) then refused = refused + 1 end
         end
         return refused .. ' refused, then ' .. execute('id.md')",
    );
    let trace = Scenario::new(main)
        .prompt("search.md", search())
        .prompt("id.md", child("id", "return sys.id"))
        .run();
    assert_eq!(trace.text(), "5 refused, then 0.0.0");
}
