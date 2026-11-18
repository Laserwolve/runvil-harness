//! A tool's file requests at the run's boundary: `Run::request_file`
//! issues each one as a `Vfs` effect whose asker is the open tool call,
//! under the task that made the call and with the call's own access, and
//! the next step returns it ahead of the effects that step issues. Its
//! answer, a receipt, a failure, or a drop, resumes no chain and reports
//! no event, and the call stays open. A request for a call whose chain
//! stopped waiting is discarded, one for any other id ends the run, and
//! so does a `Vfs` answer of the other form. The chain's abort withdraws
//! the effect with its call, and `Done` waits for its answer.

use runvil_prompt::test_support::prompt;
use runvil_types::tools::{ToolCatalog, ToolInfo, ToolOutput};
use runvil_vfs::{Access, Origin, VfsError, VfsRef};

use super::*;

/// A run of the prompt whose sections are `sections`, with `tools/echo`
/// as its whole catalog and `vfs` as the files its tool calls work in.
#[track_caller]
fn files_run(sections: &[(&str, &str)], vfs: &VfsRef) -> Run {
    let source = prompt("", sections);
    let parsed = Prompt::parse(&|_: &str| Ok(source.clone()), "prompt")
        .expect("the files test prompt parses");
    let echo = ToolInfo::new(
        ToolId::parse("tools/echo").expect("a valid tool id"),
        "Echoes its value.",
        json!({ "type": "object" }),
    );
    let catalog = ToolCatalog::new([echo]).expect("one tool is a valid catalog");
    let params = run_params().with_tools(catalog).with_vfs(vfs.clone());
    Run::new(Arc::new(parsed), "{}", params)
}

/// A run whose one section calls `tools/echo` once and returns its
/// answer.
#[track_caller]
fn one_call_run(vfs: &VfsRef) -> Run {
    files_run(&[("Only", "return tools.call('tools/echo', {})")], vfs)
}

/// A main section that spawns `Worker`, whose one tool call stays out,
/// and then runs `after`.
fn beside_a_worker(after: &str) -> [(&'static str, String); 2] {
    [
        (
            "Main",
            format!("local t = tasks.spawn('../Worker')\nstore.exists('one')\n{after}"),
        ),
        ("Worker", "return tools.call('tools/echo', {})".to_owned()),
    ]
}

/// [`files_run`] over [`beside_a_worker`]`(after)`.
#[track_caller]
fn worker_run(after: &str, vfs: &VfsRef) -> Run {
    let sections = beside_a_worker(after);
    let sections: Vec<(&str, &str)> = sections
        .iter()
        .map(|(name, lua)| (*name, lua.as_str()))
        .collect();
    files_run(&sections, vfs)
}

/// A write of `text` to `path`.
fn write(path: &str, text: &str) -> VfsOp {
    VfsOp::Write {
        path: path.to_owned(),
        contents: text.as_bytes().to_vec(),
    }
}

/// Steps `run` and returns what the pending step issued, reported, and
/// withdrew.
#[track_caller]
fn stepped(run: &mut Run) -> (Vec<IssuedEffect>, Vec<Event>, Vec<EffectId>) {
    match run.step() {
        Step::Pending {
            effects,
            events,
            withdrawn,
            ..
        } => (effects, events, withdrawn),
        other @ Step::Done { .. } => panic!("the run goes on: {other:?}"),
    }
}

/// Asserts that the next step of `run` issues, reports, and withdraws
/// nothing: the run still waits on what is out.
#[track_caller]
fn assert_still_waiting(run: &mut Run) {
    let (effects, events, withdrawn) = stepped(run);
    assert!(effects.is_empty(), "nothing new is issued: {effects:?}");
    assert!(events.is_empty(), "nothing is reported: {events:?}");
    assert!(withdrawn.is_empty(), "nothing is withdrawn: {withdrawn:?}");
}

/// Steps `run` to its end and returns its result.
#[track_caller]
fn finished(run: &mut Run) -> Result<String, Error> {
    match run.step() {
        Step::Done { result, .. } => result,
        other @ Step::Pending { .. } => panic!("the run is over: {other:?}"),
    }
}

/// The tool call among `effects`: its id and its provenance.
#[track_caller]
fn tool_call(effects: &[IssuedEffect]) -> (EffectId, Provenance) {
    effects
        .iter()
        .find_map(|issued| match &issued.effect {
            Effect::ToolCall { .. } => Some((issued.id, issued.provenance.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the step issued a tool call: {effects:?}"))
}

/// The access a tool's file effect holds.
#[track_caller]
fn file_access(issued: &IssuedEffect) -> &Arc<Access> {
    match &issued.effect {
        Effect::Vfs {
            asker: VfsAsker::Tool { .. },
            access,
            ..
        } => access,
        other => panic!("the effect is a tool's file effect: {other:?}"),
    }
}

/// The one `store` effect among `effects`.
#[track_caller]
fn store_call(effects: &[IssuedEffect]) -> EffectId {
    effects
        .iter()
        .find_map(|issued| match &issued.effect {
            Effect::Vfs {
                asker: VfsAsker::Lua { .. },
                ..
            } => Some(issued.id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the step issued a store call: {effects:?}"))
}

/// The answer of a tool that returns `text`.
fn echoed(text: &str) -> EffectAnswer {
    EffectAnswer::ToolCall(Ok(ToolOutput::trusted(text)))
}

/// The answer to a `store.exists` that finds nothing.
fn absent() -> EffectAnswer {
    EffectAnswer::Vfs(Ok(VfsOutcome::Bool(false)))
}

/// Asserts that `error` is the internal error whose message holds
/// `phrase`.
#[track_caller]
fn assert_internal(error: &Error, phrase: &str) {
    assert_eq!(error.code, ErrorCode::Internal, "{error:?}");
    let message = error.located_message();
    assert!(message.contains(phrase), "{message}");
}

#[test]
fn a_file_request_is_issued_as_a_vfs_effect_that_names_the_call_and_the_call_stays_open()
-> Result<(), VfsError> {
    let vfs = VfsRef::default();
    let mut run = one_call_run(&vfs);
    let (effects, ..) = stepped(&mut run);
    let (call, made_by) = tool_call(&effects);

    let op = write("/note.md", "from the tool");
    run.request_file(call, op.clone());
    let (effects, events, withdrawn) = stepped(&mut run);
    let [issued] = effects.as_slice() else {
        panic!("the request is the step's one effect: {effects:?}");
    };
    let Effect::Vfs {
        asker,
        access,
        op: issued_op,
    } = &issued.effect
    else {
        panic!("the request is a Vfs effect: {issued:?}");
    };
    assert_eq!(*asker, VfsAsker::Tool { call });
    assert_eq!(*issued_op, op);
    assert_eq!(
        issued.provenance.task, made_by.task,
        "the effect is stamped under the task that made the call"
    );
    assert!(issued.provenance.seq > made_by.seq, "{issued:?}");
    assert!(events.is_empty() && withdrawn.is_empty());

    // The caller performs the operation, hands the outcome to the tool,
    // and answers with the receipt. The chain stays parked on the call.
    let outcome = access.perform(issued_op)?;
    assert_eq!(outcome, VfsOutcome::Unit);
    run.resume(
        issued.id,
        EffectAnswer::VfsReceipt(Ok(VfsReceipt::of(&outcome))),
    );
    assert_still_waiting(&mut run);

    run.resume(call, echoed("echoed"));
    assert_eq!(finished(&mut run).as_deref(), Ok("echoed"));
    let after = vfs.acquire(Origin::new("the caller's own access"))?;
    assert_eq!(after.read_string("/note.md")?, "from the tool");
    Ok(())
}

#[test]
fn the_answer_to_a_tools_file_effect_resumes_no_chain_whether_a_failure_or_a_drop() {
    let mut run = one_call_run(&VfsRef::default());
    let (effects, ..) = stepped(&mut run);
    let (call, ..) = tool_call(&effects);
    for path in ["/a.md", "/b.md", "/c.md"] {
        run.request_file(call, write(path, "x"));
    }
    let (effects, ..) = stepped(&mut run);
    let [first, second, third] = effects.as_slice() else {
        panic!("each request is one effect, in request order: {effects:?}");
    };
    let identity = file_access(first).id();
    assert!(
        file_access(second).id() == identity && file_access(third).id() == identity,
        "every request of one call runs under the call's one identity"
    );
    // A conflict in the answer to a `store` call ends the run. In the
    // answer to a tool's request it is the tool's to handle.
    let conflict = VfsError::Conflict {
        path: "/a.md".to_owned(),
        detail: "two identities claimed /a.md".to_owned(),
    };
    run.resume(first.id, EffectAnswer::VfsReceipt(Err(conflict)));
    assert_still_waiting(&mut run);
    run.resume(second.id, EffectAnswer::Dropped);
    assert_still_waiting(&mut run);
    let missing = VfsError::NotFound {
        path: "/c.md".to_owned(),
    };
    run.resume(third.id, EffectAnswer::VfsReceipt(Err(missing)));
    assert_still_waiting(&mut run);

    run.resume(call, echoed("the call still answers"));
    assert_eq!(
        finished(&mut run).as_deref(),
        Ok("the call still answers"),
        "no answer to a file effect ended or resumed the run"
    );
}

#[test]
fn a_file_effect_goes_out_ahead_of_the_effects_its_step_issues() {
    let mut run = worker_run(
        "store.exists('two')\n\
         local _task, _ok, result = tasks.join_any({ t })\n\
         return result",
        &VfsRef::default(),
    );
    let (effects, ..) = stepped(&mut run);
    let (call, made_by) = tool_call(&effects);
    let first_store = store_call(&effects);

    // The request arrives before the step that runs the main chain on.
    run.request_file(call, write("/note.md", "x"));
    run.resume(first_store, absent());
    let (effects, ..) = stepped(&mut run);
    let [file, store] = effects.as_slice() else {
        panic!("the file effect, then the main chain's next store call: {effects:?}");
    };
    assert!(
        matches!(&file.effect, Effect::Vfs { asker: VfsAsker::Tool { call: asked }, .. } if *asked == call),
        "{file:?}"
    );
    assert_eq!(
        file.provenance.task, made_by.task,
        "the worker's task made the call, so the effect is stamped under it"
    );
    assert_ne!(file.provenance.task, store.provenance.task);
    assert!(
        matches!(
            &store.effect,
            Effect::Vfs {
                asker: VfsAsker::Lua { .. },
                ..
            }
        ),
        "{store:?}"
    );

    run.resume(file.id, EffectAnswer::VfsReceipt(Ok(VfsReceipt::Done)));
    run.resume(store.id, absent());
    // The main chain goes on to wait for the worker, whose call is open.
    let (effects, _, withdrawn) = stepped(&mut run);
    assert!(effects.is_empty(), "nothing new is issued: {effects:?}");
    assert!(withdrawn.is_empty(), "nothing is withdrawn: {withdrawn:?}");
    run.resume(call, echoed("echoed"));
    assert_eq!(finished(&mut run).as_deref(), Ok("echoed"));
}

#[test]
fn a_file_request_for_a_call_whose_chain_stopped_waiting_is_discarded() {
    let mut run = worker_run(
        "tasks.cancel(t)\n\
         tasks.join_any({ t })\n\
         store.exists('two')\n\
         return 'main done'",
        &VfsRef::default(),
    );
    let (effects, ..) = stepped(&mut run);
    let (call, ..) = tool_call(&effects);
    run.resume(store_call(&effects), absent());
    // The main chain cancels the worker, which withdraws its call, and
    // goes on to a second store call.
    let (effects, _, withdrawn) = stepped(&mut run);
    assert_eq!(withdrawn, [call]);
    let second_store = store_call(&effects);

    run.request_file(call, write("/late.md", "x"));
    assert_still_waiting(&mut run);

    run.resume(call, EffectAnswer::Dropped);
    run.resume(second_store, absent());
    assert_eq!(finished(&mut run).as_deref(), Ok("main done"));
}

/// Asserts that `run`, whose one outstanding effect is `outstanding`,
/// was ended by a file request that named no open tool call.
#[track_caller]
fn assert_ended_as_no_call(run: &mut Run, outstanding: EffectId) {
    let (effects, events, withdrawn) = stepped(run);
    assert!(effects.is_empty(), "no effect is issued: {effects:?}");
    assert!(ended(&events), "the request ended the run: {events:?}");
    assert_eq!(withdrawn, [outstanding]);
    run.resume(outstanding, EffectAnswer::Dropped);
    let error = finished(run).expect_err("the request fails the run");
    assert_internal(&error, "no outstanding tool call");
}

#[test]
fn a_file_request_for_an_id_that_names_no_open_tool_call_ends_the_run_with_an_internal_error() {
    let op = || write("/note.md", "x");

    // An id the run never issued.
    let mut run = one_call_run(&VfsRef::default());
    let (effects, ..) = stepped(&mut run);
    let (call, ..) = tool_call(&effects);
    run.request_file(EffectId(call.0 + 99), op());
    assert_ended_as_no_call(&mut run, call);

    // The id of an effect that is no tool call.
    let mut run = files_run(&[("Only", "return store.exists('x')")], &VfsRef::default());
    let (effects, ..) = stepped(&mut run);
    let store = store_call(&effects);
    run.request_file(store, op());
    assert_ended_as_no_call(&mut run, store);

    // A call whose answer already arrived.
    let mut run = files_run(
        &[(
            "Only",
            "tools.call('tools/echo', {})\nreturn tools.call('tools/echo', {})",
        )],
        &VfsRef::default(),
    );
    let (effects, ..) = stepped(&mut run);
    let (answered, ..) = tool_call(&effects);
    run.resume(answered, echoed("first"));
    let (effects, ..) = stepped(&mut run);
    let (open, ..) = tool_call(&effects);
    run.request_file(answered, op());
    assert_ended_as_no_call(&mut run, open);
}

#[test]
fn a_vfs_answer_of_the_other_form_ends_the_run_with_an_internal_error() {
    // The whole outcome for an operation that a tool call asked for.
    let mut run = one_call_run(&VfsRef::default());
    let (effects, ..) = stepped(&mut run);
    let (call, ..) = tool_call(&effects);
    run.request_file(call, write("/note.md", "x"));
    let (file, _) = only_effect(run.step());
    run.resume(file, EffectAnswer::Vfs(Ok(VfsOutcome::Unit)));
    let (_, events, withdrawn) = stepped(&mut run);
    assert!(ended(&events), "the mismatch ended the run: {events:?}");
    assert_eq!(withdrawn, [call]);
    run.resume(call, EffectAnswer::Dropped);
    let error = finished(&mut run).expect_err("the mismatch fails the run");
    assert_internal(&error, "effect's own kind");

    // A receipt for an operation that the prompt's Lua asked for.
    let mut run = files_run(&[("Only", "return store.exists('x')")], &VfsRef::default());
    let (store, _) = only_effect(run.step());
    run.resume(store, EffectAnswer::VfsReceipt(Ok(VfsReceipt::Bool(false))));
    let error = finished(&mut run).expect_err("the mismatch fails the run");
    assert_internal(&error, "effect's own kind");
}

#[test]
fn a_file_request_on_a_run_that_has_returned_done_does_nothing() {
    let mut run = files_run(&[("Only", "return 'x'")], &VfsRef::default());
    assert_eq!(finished(&mut run).as_deref(), Ok("x"));
    run.request_file(EffectId(0), write("/note.md", "x"));
    let Step::Done { result, events, .. } = run.step() else {
        panic!("a finished run stays finished");
    };
    assert!(
        events.is_empty(),
        "the request reported nothing: {events:?}"
    );
    assert_internal(
        &result.expect_err("a finished run cannot be stepped"),
        "cannot be stepped again",
    );
}

#[test]
fn a_chains_abort_withdraws_a_tools_file_effect_with_its_call_and_done_waits_for_both() {
    let mut run = worker_run(
        "tasks.cancel(t)\n\
         tasks.join_any({ t })\n\
         return 'main done'",
        &VfsRef::default(),
    );
    let (effects, ..) = stepped(&mut run);
    let (call, ..) = tool_call(&effects);
    let store = store_call(&effects);
    run.request_file(call, write("/note.md", "x"));
    let (file, _) = only_effect(run.step());

    // The main chain cancels the worker while the worker's call and the
    // call's file effect are both out, and then returns.
    run.resume(store, absent());
    let (effects, events, withdrawn) = stepped(&mut run);
    assert!(effects.is_empty(), "{effects:?}");
    assert!(ended(&events), "the main chain returned: {events:?}");
    assert_eq!(
        withdrawn,
        [call, file],
        "the worker's abort withdraws its call and the call's file effect"
    );
    run.resume(call, EffectAnswer::Dropped);
    assert_still_waiting(&mut run);
    run.resume(file, EffectAnswer::Dropped);
    assert_eq!(finished(&mut run).as_deref(), Ok("main done"));
}

#[test]
fn a_call_answered_before_its_file_effect_ends_its_access_and_leaves_the_effect_owed_an_answer() {
    let mut run = one_call_run(&VfsRef::default());
    let (effects, ..) = stepped(&mut run);
    let (call, ..) = tool_call(&effects);
    let op = write("/late.md", "x");
    run.request_file(call, op.clone());
    let (file, effect) = only_effect(run.step());
    let Effect::Vfs { access, .. } = effect else {
        panic!("the request is a Vfs effect: {effect:?}");
    };

    // The call's answer ends the call's identity, so the operation is
    // refused from then on.
    run.resume(call, echoed("echoed"));
    assert!(
        matches!(access.perform(&op), Err(VfsError::ScopeEnded { .. })),
        "the call's access refuses the late operation"
    );
    let (effects, events, withdrawn) = stepped(&mut run);
    assert!(effects.is_empty(), "{effects:?}");
    assert!(ended(&events), "the chain returned: {events:?}");
    assert_eq!(withdrawn, [file], "the run's end withdraws the file effect");
    run.resume(file, EffectAnswer::Dropped);
    assert_eq!(finished(&mut run).as_deref(), Ok("echoed"));
}
