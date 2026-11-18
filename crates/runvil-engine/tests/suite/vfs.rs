//! Guarding and watching the files a run's tool calls work in, and the
//! run's own store, which shares no file with them. The loop here plays
//! each tool call itself: it reports the file operations the tool asks
//! for with `Run::request_file`, and performs each through the access of
//! the `Vfs` effect the run issues for it.

use std::sync::{Arc, Mutex};

use runvil_engine::effect::{Effect, EffectAnswer, EffectId, IssuedEffect, VfsAsker, VfsReceipt};
use runvil_engine::vfs::{
    Access, MemoryBackend, Mode, ModePolicy, Op, OpEvent, Origin, VfsError, VfsOp, VfsOutcome,
    VfsRef, VfsRefBuilder, perform_vfs_op,
};
use runvil_engine::{Run, Step, ToolCatalog, ToolOutput};
use serde_json::{Value, json};

use super::support::{Built, Scenario, TestResult, greeter, note, tool_info};

/// The id of the one tool a run here may call, which [`drive`] plays.
const KEEP: &str = "example/note/keep";

/// What a test has [`KEEP`] do in one call.
struct Keep {
    /// The file operations the tool asks for, in order, given the call's
    /// arguments.
    asks: fn(&Value) -> TestResult<Vec<VfsOp>>,
    /// The text the tool answers with, given the outcomes of those
    /// operations, in order.
    says: fn(&[VfsOutcome]) -> TestResult<String>,
}

/// A call of [`KEEP`] that waits for the outcomes of its operations.
struct Open {
    /// The id of the call's effect.
    call: EffectId,
    /// How many of the call's operations the run has still to issue.
    asked: usize,
    /// The outcome of each operation performed so far, in order.
    outcomes: Vec<VfsOutcome>,
}

/// A run this module's own loop stepped to its end.
struct Ran {
    /// The run's final text.
    text: String,
    /// The store view of each `Vfs` effect a `store` call asked for, in
    /// issue order, kept once the loop had performed the effect through
    /// it.
    views: Vec<Arc<Access>>,
    /// The run itself, at `Done` and not yet dropped.
    run: Run,
}

/// Runs `source` over `vfs` with [`KEEP`] as its one catalog tool, as a
/// caller's loop does. It performs each store effect through that
/// effect's view. It plays each tool call as `tool` says: it reports
/// each operation the tool asks for, performs it through the access of
/// the effect the run issues for it, answers that effect with the
/// receipt, and answers the call once every outcome is in.
fn drive(vfs: &VfsRef, source: &str, tool: &Keep) -> TestResult<Ran> {
    let catalog = ToolCatalog::new([tool_info(KEEP, json!({ "type": "object" }))?])?;
    let vfs = vfs.clone();
    let scenario = Scenario::default()
        .params(move |ctx| ctx.with_vfs(vfs.clone()).with_tools(catalog.clone()));
    let Built { prompt, ctx } = scenario.build(source)?;
    let mut run = Run::new(Arc::new(prompt), "{}", ctx);
    let mut views = Vec::new();
    let mut open: Option<Open> = None;
    loop {
        // Each prompt here runs one chain to its end, so no step
        // withdraws an effect, and one tool call is open at a time.
        let effects = match run.step() {
            Step::Pending { effects, .. } => effects,
            Step::Done { result, .. } => {
                return Ok(Ran {
                    text: result?,
                    views,
                    run,
                });
            }
        };
        for IssuedEffect { id, effect, .. } in effects {
            match effect {
                Effect::ToolCall { args, .. } => {
                    let ops = (tool.asks)(&args)?;
                    let asked = ops.len();
                    for op in ops {
                        run.request_file(id, op);
                    }
                    open = Some(Open {
                        call: id,
                        asked,
                        outcomes: Vec::new(),
                    });
                }
                Effect::Vfs {
                    asker: VfsAsker::Tool { .. },
                    access,
                    op,
                } => {
                    // The tool gets the whole outcome, and the run its
                    // receipt.
                    let outcome = perform_vfs_op(&access, &op)?;
                    run.resume(id, EffectAnswer::VfsReceipt(Ok(VfsReceipt::of(&outcome))));
                    let call = open.as_mut().ok_or("a file effect names an open call")?;
                    call.outcomes.push(outcome);
                    call.asked -= 1;
                }
                Effect::Vfs { access, op, .. } => {
                    let outcome = perform_vfs_op(&access, &op);
                    views.push(access);
                    run.resume(id, EffectAnswer::Vfs(outcome));
                }
                other => return Err(format!("no prompt here asks for {other:?}").into()),
            }
        }
        // A call whose every operation has its outcome ends with what
        // the tool says of them.
        if let Some(call) = open.take_if(|call| call.asked == 0) {
            let said = (tool.says)(&call.outcomes)?;
            run.resume(
                call.call,
                EffectAnswer::ToolCall(Ok(ToolOutput::trusted(said))),
            );
        }
    }
}

/// A prompt that has [`KEEP`] keep `hello` and returns the tool's answer.
fn keeps_hello() -> String {
    greeter("return tools.call('example/note/keep', { text = 'hello' })")
}

/// [`KEEP`] as a tool that keeps a note: it writes the call's `text` to
/// `/note.md` and answers with the file read back.
const KEEP_NOTE: Keep = Keep {
    asks: |args| {
        let text = args["text"].as_str().ok_or("the call passes its `text`")?;
        Ok(vec![
            VfsOp::Write {
                path: "/note.md".to_owned(),
                contents: text.as_bytes().to_vec(),
            },
            VfsOp::Read {
                path: "/note.md".to_owned(),
                start: None,
                end: None,
            },
        ])
    },
    says: |outcomes| match outcomes {
        [_, VfsOutcome::Text(text)] => Ok(text.clone()),
        other => Err(format!("the note was written and read back, got {other:?}").into()),
    },
};

/// [`KEEP`] under a prompt that calls no tool: a call fails the test.
const NO_TOOL: Keep = Keep {
    asks: |_| Err("the prompt calls no tool".into()),
    says: |_| Err("the prompt calls no tool".into()),
};

/// Each watched operation's kind, path, and origin.
type Watched = Arc<Mutex<Vec<(Op, String, Origin)>>>;

/// A builder holding one empty memory mount at `/` and a watcher, beside
/// the operations the watcher saw.
fn watched() -> (VfsRefBuilder, Watched) {
    let log = Watched::default();
    let sink_log = Arc::clone(&log);
    let builder =
        VfsRef::builder()
            .mount("/", MemoryBackend::new())
            .on_op(move |event: OpEvent<'_>| {
                if let Ok(mut entries) = sink_log.lock() {
                    entries.push((event.op(), event.path().to_string(), event.origin().clone()));
                }
            });
    (builder, log)
}

#[test]
fn agent_mode_lets_a_tool_call_write_and_ask_mode_refuses_a_later_write() -> TestResult {
    let policy = ModePolicy::new(Mode::Agent);
    let mode = policy.handle();
    let vfs = VfsRef::builder()
        .mount("/", MemoryBackend::new())
        .policy(policy)
        .build();

    assert_eq!(drive(&vfs, &keeps_hello(), &KEEP_NOTE)?.text, "hello");

    mode.set(Mode::Ask);
    let access = vfs.acquire(Origin::new("caller"))?;
    let refused = access.write("/note.md", b"later");
    assert!(
        matches!(&refused, Err(VfsError::ApprovalRequired { path, .. }) if path == "/note.md"),
        "{refused:?}"
    );
    assert_eq!(access.read_string("/note.md")?, "hello");
    Ok(())
}

#[test]
fn an_operation_watcher_sees_a_tool_calls_write_then_read_under_the_calls_origin() -> TestResult {
    let (builder, log) = watched();

    assert_eq!(
        drive(&builder.build(), &keeps_hello(), &KEEP_NOTE)?.text,
        "hello"
    );

    let entries = log.lock().map_err(|_| "the watcher panicked")?;
    let [(Op::Write, written, first), (Op::Read, read, second)] = entries.as_slice() else {
        return Err(format!("the watcher saw {entries:?}").into());
    };
    assert_eq!((written.as_str(), read.as_str()), ("/note.md", "/note.md"));
    assert_eq!(first, second, "one call's operations share its origin");
    // `greeter` puts the section's one chunk on line 10 of the prompt.
    assert_eq!(
        (first.label.as_str(), first.file.as_str(), first.line),
        (KEEP, "greeter", 10),
        "a tool call's origin holds the tool's id, the prompt's frontmatter `name`, and the \
         line the calling section's first chunk starts on"
    );
    Ok(())
}

#[test]
fn a_mode_policy_and_an_operation_watcher_never_see_a_store_operation() -> TestResult {
    let (builder, log) = watched();
    let vfs = builder.policy(ModePolicy::new(Mode::Ask)).build();

    assert_eq!(
        drive(&vfs, &note("'hello'"), &NO_TOOL)?.text,
        "hello",
        "the Ask mode, which refuses every change to the caller's files, lets the store write"
    );

    let entries = log.lock().map_err(|_| "the watcher panicked")?;
    assert!(entries.is_empty(), "the watcher saw {entries:?}");
    Ok(())
}

#[test]
fn each_run_starts_with_an_empty_store_that_shares_no_file_with_the_callers_filesystem()
-> TestResult {
    let vfs = VfsRef::default();
    vfs.acquire(Origin::new("caller"))?
        .write("/notes.md", b"from the caller")?;
    let source = greeter(
        "
        local found = store.exists('notes.md')
        local held = #store.glob('**')
        store.write('notes.md', 'from the run')
        store.write('summary.md', 'from the run')
        return tostring(found) .. ' ' .. held",
    );

    // The second run finds neither the caller's file nor the first run's.
    for _ in 0..2 {
        assert_eq!(drive(&vfs, &source, &NO_TOOL)?.text, "false 0");
    }

    let access = vfs.acquire(Origin::new("caller"))?;
    assert_eq!(access.read_string("/notes.md")?, "from the caller");
    let names: Vec<String> = access
        .list("/")?
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["notes.md"], "no run's store write is a file here");
    Ok(())
}

#[test]
fn a_tool_call_and_the_runs_store_share_no_file() -> TestResult {
    let source = greeter(
        "
        store.write('note.md', 'from the store')
        local found = tools.call('example/note/keep')
        return found .. ' ' .. store.read('note.md')",
    );
    // The tool looks for the store's note, then writes one of its own.
    let looks_then_writes = Keep {
        asks: |_| {
            Ok(vec![
                VfsOp::Exists {
                    path: "/note.md".to_owned(),
                },
                VfsOp::Write {
                    path: "/note.md".to_owned(),
                    contents: b"from the tool".to_vec(),
                },
            ])
        },
        says: |outcomes| match outcomes {
            [VfsOutcome::Bool(found), _] => Ok(found.to_string()),
            other => Err(format!("the tool looked and then wrote, got {other:?}").into()),
        },
    };
    let ran = drive(&VfsRef::default(), &source, &looks_then_writes)?;
    assert_eq!(ran.text, "false from the store");
    Ok(())
}

#[test]
fn a_store_view_is_the_callers_one_way_to_the_runs_store_and_refuses_every_operation_once_the_run_is_done()
-> TestResult {
    let vfs = VfsRef::default();
    let ran = drive(&vfs, &note("'hello'"), &NO_TOOL)?;
    assert_eq!(
        ran.text, "hello",
        "the read through its view returns what the write through its view stored"
    );
    assert!(
        vfs.acquire(Origin::new("caller"))?.list("/")?.is_empty(),
        "the filesystem the caller gave the run holds none of the store"
    );

    let path = || "note.md".to_owned();
    let ops = [
        VfsOp::Write {
            path: path(),
            contents: b"late".to_vec(),
        },
        VfsOp::Append {
            path: path(),
            contents: b"late".to_vec(),
        },
        VfsOp::Read {
            path: path(),
            start: None,
            end: None,
        },
        VfsOp::ReadNumbered {
            path: path(),
            start: None,
            end: None,
        },
        VfsOp::StrReplace {
            path: path(),
            old: "hello".to_owned(),
            new: "late".to_owned(),
        },
        VfsOp::Delete { path: path() },
        VfsOp::Glob {
            pattern: "*.md".to_owned(),
        },
        VfsOp::Exists { path: path() },
    ];
    assert_eq!(ran.views.len(), 2, "the write's view and the read's");
    for view in &ran.views {
        for op in &ops {
            let refused = perform_vfs_op(view, op);
            assert!(
                matches!(refused, Err(VfsError::ScopeEnded { .. })),
                "{op:?}: {refused:?}"
            );
        }
    }
    // The run outlived every refusal above, so `Done` ended its scope,
    // not its drop.
    drop(ran.run);
    Ok(())
}
