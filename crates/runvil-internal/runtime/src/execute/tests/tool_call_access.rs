//! Tests for the access a tool call's file requests run on: the call's
//! own identity, forked from the calling chain when the call is issued
//! and ended when its answer is applied or the call is aborted, and
//! rooted at `/` of the handle the caller gave the run. The driver
//! reports the one operation each call asks for with
//! `Run::request_file`, and performs it through the access of the `Vfs`
//! effect the run issues for it. The run's handle checks no claims, so a
//! test of the order of identities has the driver note each call in a
//! store of the test's own first, through a view under the call's
//! identity: a store checks claims, and a note that nothing orders after
//! an earlier note of its file fails the call.
//! One call hands a file to a later call: the end joins the first call's
//! work into the chain before the chain's next step, and the fork orders
//! the chain's earlier work before the later call's. An executed prompt's
//! calls fork from its caller's identity, so a call of each hands a file
//! to a call of the other. A task's calls fork
//! from the task, the calls of two tasks are unordered, a dropped answer
//! and a canceled task's call are joined
//! like any other, and an access used after its call has ended refuses
//! every operation. A call's access reaches no store: a call sees no file
//! the chain wrote with `store.write`, and `store.read` sees no file a
//! call wrote.

use std::fmt;

use runvil_vfs::{ExecId, Origin, Store, VfsRef};

use super::models_loop::loop_models;
use super::*;
use crate::execute::run::{
    Effect, EffectAnswer, EffectId, IssuedEffect, Run, Step, VfsAsker, VfsReceipt,
};
use crate::test_support::perform_locally;
use crate::test_support::scenario::text_of;

/// The suite's one tool, called by its full id: it is the run's whole
/// catalog.
const FILES: &str = "test/tools/files";

/// The tool the run binds. The driver performs its calls itself, through
/// the access of each call's file request, so its own call never runs.
#[track_caller]
fn files_tool() -> FnTool {
    FnTool::new(FILES, json!({ "type": "object" }), |_| {
        Err(make_tool_error(
            ErrorCode::ToolOther,
            "the suite's driver performs this tool",
        ))
    })
    .description("reads, writes, or lists the run's files")
}

/// The one operation a files call asks for: `read` names a file whose
/// text is the output, `list` is a pattern whose matches are the output,
/// and `write` names a file that receives `text`.
fn asked_op(args: &Value) -> VfsOp {
    if let Some(path) = args["read"].as_str() {
        return VfsOp::Read {
            path: path.to_owned(),
            start: None,
            end: None,
        };
    }
    if let Some(pattern) = args["list"].as_str() {
        return VfsOp::Glob {
            pattern: pattern.to_owned(),
        };
    }
    let path = args["write"]
        .as_str()
        .expect("a files call names a file to read or write, or a pattern to list");
    VfsOp::Write {
        path: path.to_owned(),
        contents: args["text"]
            .as_str()
            .unwrap_or_default()
            .as_bytes()
            .to_vec(),
    }
}

/// One file request of a files call, as the run issued it.
struct Asked<'a> {
    /// The call's own access, which the run issued the operation with.
    access: &'a Arc<Access>,
    /// The operation the call asked for.
    op: &'a VfsOp,
}

/// What a driver made of one file request: the outcome of its operation,
/// which the run takes as a receipt, and the answer of the call that
/// asked.
type Performed = (std::result::Result<VfsOutcome, VfsError>, EffectAnswer);

/// The answer of a files call whose operation ended as `outcome`: the
/// text of a read, the matches of a listing joined with commas between
/// brackets, `written` for a write, or the failure as a tool error.
fn call_answer(outcome: &std::result::Result<VfsOutcome, VfsError>) -> EffectAnswer {
    EffectAnswer::ToolCall(match outcome {
        Ok(VfsOutcome::Text(text)) => Ok(ToolOutput::trusted(text.clone())),
        Ok(VfsOutcome::Paths(paths)) => Ok(ToolOutput::trusted(format!("[{}]", paths.join(",")))),
        Ok(_) => Ok(ToolOutput::trusted("written")),
        Err(error) => Err(make_tool_error(ErrorCode::ToolOther, error.to_string())),
    })
}

/// Performs the operation of `asked` through the call's access, and
/// answers the call with what it found.
fn use_access(asked: &Asked<'_>) -> Performed {
    let outcome = asked.access.perform(asked.op);
    let answer = call_answer(&outcome);
    (outcome, answer)
}

/// Notes one file request in `ledger`, a store the test keeps beside the
/// run: the call's write or read of its file, made through a view under
/// the call's own identity. A store checks claims, which the run's handle
/// does not, so the note fails with a conflict unless every earlier note
/// of the file is ordered before this call.
fn note_in(ledger: &Store, asked: &Asked<'_>) -> std::result::Result<(), VfsError> {
    let view = ledger.view(asked.access)?;
    let name = |path: &str| path.trim_start_matches('/').to_owned();
    match asked.op {
        VfsOp::Write { path, .. } => view.write(&name(path), b"noted"),
        VfsOp::Read { path, .. } => view.exists(&name(path)).map(drop),
        _ => Ok(()),
    }
}

/// [`use_access`] once the request's note in `ledger` is made. A note
/// that conflicts fails the call with the conflict and leaves the file
/// alone.
fn use_noted(ledger: &Store, asked: &Asked<'_>) -> Performed {
    match note_in(ledger, asked) {
        Ok(()) => use_access(asked),
        Err(conflict) => {
            let outcome = Err(conflict);
            let answer = call_answer(&outcome);
            (outcome, answer)
        }
    }
}

/// The receipt the run takes for an operation that ended as `outcome`.
fn receipt(outcome: std::result::Result<VfsOutcome, VfsError>) -> EffectAnswer {
    EffectAnswer::VfsReceipt(outcome.map(|outcome| VfsReceipt::of(&outcome)))
}

/// What a driven files run came to: its result, and the identity each
/// store effect's view held, in issue order. A view keeps its chain's
/// identity.
struct Driven {
    result: std::result::Result<String, runvil_types::error::Error>,
    stores: Vec<ExecId>,
}

/// Whether every tool call of `tools` ran under an identity of its own:
/// distinct from every other call's and from every chain's in `stores`.
fn each_call_is_its_own(tools: &[ExecId], stores: &[ExecId]) -> bool {
    tools
        .iter()
        .enumerate()
        .all(|(index, tool)| !stores.contains(tool) && !tools[index + 1..].contains(tool))
}

/// Drives `run` to its end as a caller whose tool asks for its files.
/// For each files call, the driver reports the operation the call's
/// arguments name with `Run::request_file`. The run issues it as a `Vfs`
/// effect that holds the call's own access, `perform` makes of it the
/// operation's outcome and the call's answer, and the driver answers the
/// effect with the receipt and then the call. Each store effect runs on
/// its view.
fn drive_files(mut run: Run, mut perform: impl FnMut(Asked<'_>) -> Performed) -> Driven {
    let mut stores = Vec::new();
    loop {
        let effects = match run.step() {
            Step::Done { result, .. } => return Driven { result, stores },
            Step::Pending { effects, .. } => effects,
        };
        assert!(
            !effects.is_empty(),
            "the driver answers every effect, so a pending step issues one"
        );
        for IssuedEffect { id, effect, .. } in effects {
            match &effect {
                Effect::ToolCall { args, .. } => run.request_file(id, asked_op(args)),
                Effect::Vfs {
                    asker: VfsAsker::Tool { call },
                    access,
                    op,
                } => {
                    let (outcome, answer) = perform(Asked { access, op });
                    run.resume(id, receipt(outcome));
                    run.resume(*call, answer);
                }
                other => {
                    if let Effect::Vfs { access, .. } = other {
                        stores.push(access.id());
                    }
                    run.resume(id, perform_store(other));
                }
            }
        }
    }
}

/// A run over `md` with the files tool as its whole catalog, nothing
/// offered, the loop models filled in, and `vfs` as the files its tool
/// calls work in, for the test to step.
#[track_caller]
fn files_run(md: &str, vfs: &VfsRef) -> Run {
    Scenario::new(md)
        .models(loop_models())
        .offering(Vec::new())
        .tool(files_tool())
        .vfs(vfs.clone())
        .into_run()
}

/// Every file `vfs` holds beside its text, in path order, read through an
/// access of the caller's own once the run over `vfs` has ended.
fn files_in(vfs: &VfsRef) -> std::result::Result<Vec<(String, String)>, VfsError> {
    let access = vfs.acquire(Origin::new("the caller's own access"))?;
    access
        .glob("/**")?
        .into_iter()
        .map(|path| {
            let text = access.read_string(&path)?;
            Ok((path, text))
        })
        .collect()
}

/// The one file `path` holding `text`, as [`files_in`] lists it.
fn one_file(path: &str, text: &str) -> Vec<(String, String)> {
    vec![(path.to_owned(), text.to_owned())]
}

/// Performs a store effect, or panics on the model round no files run
/// issues.
fn perform_store(effect: &Effect) -> EffectAnswer {
    perform_locally(effect, &mut |round| {
        panic!("no model round is issued: {round:?}")
    })
}

/// One operation of each kind tried through `access`, each beside its
/// name: a read of `file`, a write of `late.txt`, an existence check and
/// a removal of `file`, and a listing.
fn every_operation(
    access: &Access,
    file: &str,
) -> Vec<(&'static str, std::result::Result<(), VfsError>)> {
    vec![
        ("read", access.read(file).map(drop)),
        ("write", access.write("late.txt", b"after the call ended")),
        ("exists", access.exists(file).map(drop)),
        ("glob", access.glob("**").map(drop)),
        ("remove", access.remove(file, false).map(drop)),
    ]
}

/// Asserts that every result in `tried` is the refusal of an access
/// whose tool call has ended. The run and the tool call end an access as
/// the same variant; the callers hold the run open, so the call's end is
/// the one refused.
#[track_caller]
fn assert_all_refused_as_ended<T: fmt::Debug>(
    tried: Vec<(&'static str, std::result::Result<T, VfsError>)>,
) {
    assert!(!tried.is_empty(), "an operation was tried");
    for (operation, result) in tried {
        match result {
            Err(VfsError::ScopeEnded { .. }) => {}
            other => panic!("expected the ended-call refusal of the {operation}, got {other:?}"),
        }
    }
}

/// The performer of a files run for a test of the order of identities:
/// each file request is first noted in a store the performer keeps, and
/// a call whose note conflicts fails with the conflict. `tools` takes
/// the identity each request held.
fn ordered(tools: &mut Vec<ExecId>) -> impl FnMut(Asked<'_>) -> Performed + '_ {
    let ledger = Store::new();
    move |asked| {
        tools.push(asked.access.id());
        use_noted(&ledger, &asked)
    }
}

/// The performer of a files run with no note: each file request is
/// performed through its call's access. `tools` takes the identity each
/// request held.
fn unordered(tools: &mut Vec<ExecId>) -> impl FnMut(Asked<'_>) -> Performed + '_ {
    move |asked| {
        tools.push(asked.access.id());
        use_access(&asked)
    }
}

#[test]
fn one_tool_call_hands_a_file_to_a_later_tool_call() -> std::result::Result<(), VfsError> {
    // Both accesses are rooted at `/` of the run's handle: the first call
    // writes `handoff.txt` below its root, the second reads
    // `/handoff.txt`, and the caller's own access finds the file there
    // once the run has ended. The first call's end joins its write into
    // the chain, and the second call forks from the chain after that, so
    // the write is ordered before the read, and the read's note meets no
    // conflict.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        tools.call('test/tools/files', { write = 'handoff.txt', text = 'first call wrote this' })\n\
        return tools.call('test/tools/files', { read = '/handoff.txt' })\n\
        ```\n";
    let vfs = VfsRef::default();
    let driven = drive_files(files_run(md, &vfs), ordered(&mut Vec::new()));
    assert_eq!(text_of(driven.result), "first call wrote this");
    assert_eq!(
        files_in(&vfs)?,
        one_file("/handoff.txt", "first call wrote this")
    );
    Ok(())
}

#[test]
fn a_tool_call_of_an_executed_prompt_and_one_of_its_caller_hand_files_to_each_other()
-> std::result::Result<(), VfsError> {
    // The executed prompt's chain runs on its caller's access, as a
    // `call` chain does, so its tool calls fork from the caller's
    // identity: the caller's earlier call is ordered before the executed
    // prompt's read, and the executed prompt's write before the caller's
    // later read. The two prompts' stores are apart, but the run's files
    // are one.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\nprompts: [child.md]\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        tools.call('test/tools/files', { write = 'down.txt', text = 'the caller wrote this' })\n\
        local seen = execute('child.md')\n\
        return seen .. ' | ' .. tools.call('test/tools/files', { read = '/up.txt' })\n\
        ```\n";
    let child = "---\nname: child\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        local seen = tools.call('test/tools/files', { read = '/down.txt' })\n\
        tools.call('test/tools/files', { write = 'up.txt', text = 'the child wrote this' })\n\
        return seen\n\
        ```\n";
    let vfs = VfsRef::default();
    let run = Scenario::new(md)
        .prompt("child.md", child)
        .models(loop_models())
        .offering(Vec::new())
        .tool(files_tool())
        .vfs(vfs.clone())
        .into_run();
    let mut tools = Vec::new();
    let driven = drive_files(run, ordered(&mut tools));
    assert_eq!(
        text_of(driven.result),
        "the caller wrote this | the child wrote this"
    );
    assert_eq!(tools.len(), 4, "two calls of each prompt, none refused");
    assert!(
        each_call_is_its_own(&tools, &driven.stores),
        "each call ran under an identity of its own"
    );
    assert_eq!(
        files_in(&vfs)?,
        vec![
            ("/down.txt".to_owned(), "the caller wrote this".to_owned()),
            ("/up.txt".to_owned(), "the child wrote this".to_owned()),
        ]
    );
    Ok(())
}

#[test]
fn a_tool_calls_work_is_ordered_after_its_chains_earlier_work_and_before_its_chains_next_step() {
    // The notes of two writes of one file by two identities conflict
    // unless one is ordered before the other, and a conflict fails the
    // call. The first
    // call's end joins its write into the chain, so it is ordered before
    // the chain's next step; the second call forks from the chain at that
    // step, so the chain's earlier work, the first call's included, is
    // ordered before the second call's write; and the third call reads
    // what the second wrote the same way.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        tools.call('test/tools/files', { write = 'turn.txt', text = 'first call' })\n\
        tools.call('test/tools/files', { write = 'turn.txt', text = 'second call' })\n\
        return tools.call('test/tools/files', { read = 'turn.txt' })\n\
        ```\n";
    let driven = drive_files(files_run(md, &VfsRef::default()), ordered(&mut Vec::new()));
    assert_eq!(text_of(driven.result), "second call");
}

#[test]
fn the_tool_calls_of_two_tasks_are_unordered_and_both_reach_the_runs_files()
-> std::result::Result<(), VfsError> {
    // Nothing orders one task's call and the other's, so the note of the
    // second write of the file conflicts with the first, which fails that
    // call before it writes.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Main\n\n\
        ```lua\n\
        local left = tasks.spawn('../Left')\n\
        local right = tasks.spawn('../Right')\n\
        local done = tasks.join({ left, right })\n\
        return done[1].result .. '|' .. done[2].result\n\
        ```\n\n\
        ## Left\n\n\
        ```lua\n\
        return tostring(pcall(tools.call, 'test/tools/files', { write = 'same.txt', text = 'left' }))\n\
        ```\n\n\
        ## Right\n\n\
        ```lua\n\
        return tostring(pcall(tools.call, 'test/tools/files', { write = 'same.txt', text = 'right' }))\n\
        ```\n";
    let vfs = VfsRef::default();
    let driven = drive_files(files_run(md, &vfs), ordered(&mut Vec::new()));
    assert_eq!(text_of(driven.result), "true|false");
    assert_eq!(files_in(&vfs)?, one_file("/same.txt", "left"));

    // With no note to conflict, both calls write the file through the
    // run's handle, which checks no claims, and the later write stays.
    let vfs = VfsRef::default();
    let driven = drive_files(files_run(md, &vfs), unordered(&mut Vec::new()));
    assert_eq!(text_of(driven.result), "true|true");
    assert_eq!(files_in(&vfs)?, one_file("/same.txt", "right"));
    Ok(())
}

#[test]
fn each_tool_call_has_an_identity_of_its_own() {
    // The store effects' views keep the chain's identity, which is how
    // the driver learns it: one identity for the chain before, between,
    // and after its calls, and another for each call.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        store.exists('before.txt')\n\
        tools.call('test/tools/files', { write = 'a.txt', text = 'first call' })\n\
        store.exists('between.txt')\n\
        tools.call('test/tools/files', { read = 'a.txt' })\n\
        store.exists('after.txt')\n\
        return 'done'\n\
        ```\n";
    let mut tools = Vec::new();
    let driven = drive_files(files_run(md, &VfsRef::default()), unordered(&mut tools));
    assert_eq!(text_of(driven.result), "done");
    assert_eq!(tools.len(), 2, "two calls were issued");
    assert_eq!(
        driven.stores, [driven.stores[0]; 3],
        "the chain keeps one identity across its calls"
    );
    assert!(
        each_call_is_its_own(&tools, &driven.stores),
        "each call ran under an identity forked from the chain, not the chain's own"
    );
}

#[test]
fn a_tasks_tool_calls_fork_from_the_task_and_the_owners_call_reads_their_file_after_the_join() {
    // The task's second call reads the file its first call wrote, which
    // only the task's identity orders before it; the owner's call reads
    // the file the task's last call wrote once the task is joined, which
    // joins the task's calls with it. A note out of that order would
    // conflict and fail its call.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Main\n\n\
        ```lua\n\
        local t = tasks.spawn('../Worker')\n\
        tasks.join({ t })\n\
        return tools.call('test/tools/files', { read = 'out.txt' })\n\
        ```\n\n\
        ## Worker\n\n\
        ```lua\n\
        tools.call('test/tools/files', { write = 'in.txt', text = 'task call wrote this' })\n\
        local seen = tools.call('test/tools/files', { read = 'in.txt' })\n\
        tools.call('test/tools/files', { write = 'out.txt', text = 'later call saw: ' .. seen })\n\
        return 'done'\n\
        ```\n";
    let mut tools = Vec::new();
    let driven = drive_files(files_run(md, &VfsRef::default()), ordered(&mut tools));
    assert_eq!(
        text_of(driven.result),
        "later call saw: task call wrote this"
    );
    assert_eq!(
        tools.len(),
        4,
        "the task issued three calls and its owner one"
    );
    assert!(
        each_call_is_its_own(&tools, &driven.stores),
        "each call ran under its own identity"
    );
}

#[test]
fn a_tool_call_answered_dropped_is_still_joined_and_its_access_refuses_every_operation()
-> std::result::Result<(), VfsError> {
    // The driver performs the first call's request, answers the call
    // `Dropped`, and keeps its access. The second call reads what the
    // first wrote, and its note meets no conflict, so the dropped call
    // was joined into the chain; while the driver performs the second
    // call's request, with the run's scope still open, it tries the
    // first call's access.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        local ok, err = pcall(tools.call, 'test/tools/files', { write = 'dropped.txt', text = 'tool wrote this' })\n\
        return tostring(ok) .. ':' .. err.kind .. '|' .. tools.call('test/tools/files', { read = 'dropped.txt' })\n\
        ```\n";
    let vfs = VfsRef::default();
    let ledger = Store::new();
    let mut dropped: Option<Arc<Access>> = None;
    let mut late = None;
    let driven = drive_files(files_run(md, &vfs), |asked| {
        let (outcome, answer) = use_noted(&ledger, &asked);
        if let Some(earlier) = &dropped {
            late = Some(every_operation(earlier, "dropped.txt"));
            (outcome, answer)
        } else {
            dropped = Some(Arc::clone(asked.access));
            (outcome, EffectAnswer::Dropped)
        }
    });
    assert_eq!(text_of(driven.result), "false:canceled|tool wrote this");
    assert_all_refused_as_ended(late.expect("the second call tried the dropped call's access"));
    assert_eq!(
        files_in(&vfs)?,
        one_file("/dropped.txt", "tool wrote this"),
        "the refused operations left the files as they were"
    );
    Ok(())
}

#[test]
fn an_access_kept_past_its_calls_answer_refuses_every_operation_and_changes_nothing()
-> std::result::Result<(), VfsError> {
    // The driver keeps the access of the first call's request and tries
    // it while performing the second call's request, with the run's
    // scope still open.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        tools.call('test/tools/files', { write = 'first.txt', text = 'first call' })\n\
        return tools.call('test/tools/files', { read = 'first.txt' })\n\
        ```\n";
    let vfs = VfsRef::default();
    let mut kept: Option<Arc<Access>> = None;
    let mut late = None;
    let driven = drive_files(files_run(md, &vfs), |asked| {
        if let Some(earlier) = &kept {
            late = Some(every_operation(earlier, "first.txt"));
        }
        kept = Some(Arc::clone(asked.access));
        use_access(&asked)
    });
    assert_eq!(text_of(driven.result), "first call");
    assert_all_refused_as_ended(late.expect("the second call tried the first call's access"));
    assert_eq!(
        files_in(&vfs)?,
        one_file("/first.txt", "first call"),
        "the refused operations left the files as they were"
    );
    Ok(())
}

#[test]
fn a_canceled_tasks_tool_call_is_joined_and_its_access_refuses_every_later_operation()
-> std::result::Result<(), VfsError> {
    // The driver performs the request of the task's call at once but
    // holds the call's answer, so the cancel finds the task parked on
    // it. The owner's call reads what the task's call wrote before the
    // cancel, and its note meets no conflict, so the canceled call was
    // joined; what the driver tries through the access of the task's
    // call once the owner has returned, while the run waits only on the
    // held answer, is refused.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Main\n\n\
        ```lua\n\
        local a = tasks.spawn('../Task')\n\
        store.exists('p1')\n\
        store.exists('p2')\n\
        tasks.cancel(a)\n\
        tasks.join_any({ a })\n\
        return tools.call('test/tools/files', { read = 'tool.txt' })\n\
        ```\n\n\
        ## Task\n\n\
        ```lua\n\
        return tools.call('test/tools/files', { write = 'tool.txt', text = 'tool wrote this' })\n\
        ```\n";
    let vfs = VfsRef::default();
    let ledger = Store::new();
    let mut run = files_run(md, &vfs);
    let mut held: Option<(EffectId, Arc<Access>, EffectAnswer)> = None;
    let mut late = None;
    let result = loop {
        let (effects, withdrawn) = match run.step() {
            Step::Done { result, .. } => break result,
            Step::Pending {
                effects, withdrawn, ..
            } => (effects, withdrawn),
        };
        if effects.is_empty() {
            let (id, access, answer) = held
                .take()
                .expect("the decided run waits on the held call's answer");
            assert_eq!(
                withdrawn,
                [id],
                "only the step that decides the run waits on nothing new, and it withdraws the held call"
            );
            late = Some(every_operation(&access, "tool.txt"));
            run.resume(id, answer);
            continue;
        }
        for IssuedEffect { id, effect, .. } in effects {
            match &effect {
                Effect::ToolCall { args, .. } => run.request_file(id, asked_op(args)),
                Effect::Vfs {
                    asker: VfsAsker::Tool { call },
                    access,
                    op,
                } => {
                    let (outcome, answer) = use_noted(&ledger, &Asked { access, op });
                    run.resume(id, receipt(outcome));
                    if matches!(op, VfsOp::Write { .. }) {
                        held = Some((*call, Arc::clone(access), answer));
                    } else {
                        run.resume(*call, answer);
                    }
                }
                _ => run.resume(id, perform_store(&effect)),
            }
        }
    };
    assert_eq!(text_of(result), "tool wrote this");
    assert_all_refused_as_ended(late.expect("the driver tried the canceled call's access"));
    assert_eq!(
        files_in(&vfs)?,
        one_file("/tool.txt", "tool wrote this"),
        "the refused operations left the files as they were"
    );
    Ok(())
}

#[test]
fn a_tool_call_sees_no_file_the_chain_wrote_with_store_write() -> std::result::Result<(), VfsError>
{
    // One call lists every file its access reaches, and another reads
    // the name the chain wrote: the listing is empty, and the read finds
    // no file at that name below the root of the run's handle.
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        store.write('note.txt', 'the chain wrote this')\n\
        local listed = tools.call('test/tools/files', { list = '**' })\n\
        local ok, err = pcall(tools.call, 'test/tools/files', { read = 'note.txt' })\n\
        return listed .. '|' .. tostring(ok) .. '|' .. tostring(err) .. '|' .. store.read('note.txt')\n\
        ```\n";
    let vfs = VfsRef::default();
    let driven = drive_files(files_run(md, &vfs), unordered(&mut Vec::new()));
    assert_eq!(
        text_of(driven.result),
        "[]|false|prompt:12: not found: /note.txt|the chain wrote this"
    );
    assert_eq!(
        files_in(&vfs)?,
        Vec::new(),
        "the run's handle holds no file the chain wrote to its store"
    );
    Ok(())
}

#[test]
fn store_read_sees_no_file_a_tool_call_wrote() -> std::result::Result<(), VfsError> {
    let md = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n\
        ```lua\n\
        tools.call('test/tools/files', { write = 'tool.txt', text = 'a tool call wrote this' })\n\
        local ok, err = pcall(store.read, 'tool.txt')\n\
        return tostring(ok) .. '|' .. err.reason .. '|' .. tostring(store.exists('tool.txt')) .. '|' .. #store.glob('**')\n\
        ```\n";
    let vfs = VfsRef::default();
    let driven = drive_files(files_run(md, &vfs), unordered(&mut Vec::new()));
    assert_eq!(text_of(driven.result), "false|not_found|false|0");
    assert_eq!(
        files_in(&vfs)?,
        one_file("/tool.txt", "a tool call wrote this"),
        "the call's file is in the run's handle"
    );
    Ok(())
}
