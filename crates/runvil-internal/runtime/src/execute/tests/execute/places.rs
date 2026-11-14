//! Where an `execute` may be written, and how the executed prompt counts
//! as part of the run that executes it: the preamble, a library, and a
//! task each execute a prompt; a caller waiting on one reports itself
//! blocked on a call; the executed prompt's tasks run within its caller's
//! share of the admission limit; and a cancel of the run, or of the task
//! that executes it, ends the executed prompt too.

use super::*;
use crate::execute::scheduler::test_hooks::TaskState;
use crate::execute::tests::context::task;
use crate::execute::tests::scheduler::asks;
use crate::execute::tests::tasks::TaskRecorder;
use crate::test_support::rig::Rig;

#[test]
fn the_preamble_executes_a_prompt() {
    let main = listing("t", &["child.md"])
        .preamble("var.early = execute('child.md', 'from the preamble')")
        .section("Main", "return var.early")
        .build();
    let trace = Scenario::new(main)
        .prompt("child.md", child("child", "return 'got ' .. args.text"))
        .run();
    assert_eq!(trace.text(), "got from the preamble");
}

#[test]
fn a_library_executes_a_prompt_its_requiring_prompt_lists() {
    let main = listing("t", &["child.md"])
        .section(
            "Main",
            "require '/runner'
             return run_child('from the library')",
        )
        .section(
            "runner",
            "function run_child(text)
               return execute('child.md', text)
             end",
        )
        .build();
    let trace = Scenario::new(main)
        .prompt("child.md", child("child", "return 'got ' .. args.text"))
        .run();
    assert_eq!(trace.text(), "got from the library");
}

#[test]
fn a_library_executes_a_prompt_while_it_loads() {
    let main = listing("t", &["child.md"])
        .section("Main", "require '/runner'\nreturn at_load")
        .section("runner", "at_load = execute('child.md', 'at load')")
        .build();
    let trace = Scenario::new(main)
        .prompt("child.md", child("child", "return 'got ' .. args.text"))
        .run();
    assert_eq!(trace.text(), "got at load");
}

#[test]
fn a_library_of_the_executed_prompt_executes_from_that_prompts_list() {
    // The library is the executed prompt's, so the name it executes is
    // looked up in that prompt's list, which the run's prompt does not
    // share.
    let main = main_listing(&["b.md"], "return execute('b.md')");
    let b = listing("b", &["c.md"])
        .section(
            "Main",
            "require '/runner'
             return 'b ran ' .. run_c()",
        )
        .section("runner", "function run_c() return execute('c.md') end")
        .build();
    let trace = Scenario::new(main)
        .prompt("b.md", b)
        .prompt("c.md", child("c", "return 'c'"))
        .run();
    assert_eq!(trace.text(), "b ran c");
}

#[test]
fn a_task_executes_a_prompt() {
    let main = listing("t", &["child.md"])
        .section(
            "Main",
            "local workers = tasks.join({
               tasks.spawn('../Worker', { args = 'one' }),
               tasks.spawn('../Worker', { args = 'two' }),
             })
             return workers[1].result .. ' ' .. workers[2].result",
        )
        .section("Worker", "return execute('child.md', args.text)")
        .build();
    let trace = Scenario::new(main)
        .prompt(
            "child.md",
            child("child", "return args.text .. '@' .. sys.taskid"),
        )
        .run();
    assert_eq!(
        trace.text(),
        "one@0.0 two@0.1",
        "each executed chain runs in the task that executes it"
    );
}

/// A prompt whose `Main` spawns `Worker`, which executes `child.md`,
/// whose one section asks the model `slow please`, a round the test
/// holds. `main` is the Lua of `Main` after the spawn of the task `t`.
fn worker_executing_a_parked_child(main: &str) -> Scenario {
    let source = listing("t", &["child.md"])
        .section(
            "Main",
            &format!("local t = tasks.spawn('../Worker')\n{main}"),
        )
        .section("Worker", "return execute('child.md')")
        .build();
    Scenario::new(source)
        .bound()
        .prompt(
            "child.md",
            child("child", "return models.infer('slow please')"),
        )
        .hold(asks("slow please"))
}

#[test]
fn a_task_waiting_on_the_prompt_it_executes_reports_blocked_on_a_call() {
    let gateway = ScriptedChat::new(vec![text("slow answer")]);
    let scenario = worker_executing_a_parked_child(
        "store.write('park', 'x')
         local s = tasks.status(t)
         log('blocked=' .. tostring(s.blocked) .. ' section=' .. tostring(s.section))
         local _, ok, result = tasks.join_any({ t })
         return tostring(ok) .. ' ' .. result",
    );
    let mut rig = Rig::start(scenario.chat(&gateway));
    rig.settle();
    rig.release(asks("slow please"));
    let rig = rig.finish();
    assert_eq!(rig.trace().text(), "true slow answer");
    assert_eq!(
        TaskRecorder::replayed(rig.trace()).logs("Main"),
        vec!["blocked=call section=Worker".to_owned()],
        "the task's own chain waits on the executed chain as on a `call` child"
    );
}

#[test]
fn a_cancel_of_the_run_while_the_executed_prompt_waits_ends_the_run_canceled() {
    let mut rig = Rig::start(
        Scenario::new(main_listing(&["child.md"], "return execute('child.md')"))
            .bound()
            .prompt(
                "child.md",
                child("child", "return models.infer('slow please')"),
            )
            .hold(asks("slow please")),
    );
    rig.settle();
    rig.cancel();
    let rig = rig.finish();
    assert!(
        rig.trace().error().is_canceled(),
        "got {:?}",
        rig.trace().result()
    );
    assert_eq!(
        started(rig.trace()),
        ["Main", "Only"],
        "the executed prompt's section had started when the cancel came"
    );
}

#[test]
fn a_cancel_of_the_task_that_executes_a_prompt_ends_the_executed_chain_with_it() {
    let scenario = worker_executing_a_parked_child(
        "store.write('park', 'x')
         tasks.cancel(t)
         local _, ok, result = tasks.join_any({ t })
         return tostring(ok) .. ' ' .. result.kind",
    );
    let mut rig = Rig::run(scenario);
    assert_eq!(
        rig.trace().text(),
        "false canceled",
        "the held round is withdrawn with the executed chain, so the run ends without it"
    );
    assert_eq!(
        rig.task_state_for_test(&task("0.0")),
        Some(TaskState::Canceled)
    );
}

#[test]
fn the_executed_prompts_tasks_run_within_its_callers_share_of_the_admission_limit() {
    let main = main_listing(
        &["child.md"],
        "tasks.concurrency(2)
         return execute('child.md') .. ' then ' .. tasks.concurrency()",
    );
    let child = child(
        "child",
        "local share = tasks.concurrency()
         tasks.concurrency(1)
         return share .. ' narrowed to ' .. tasks.concurrency()",
    );
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(
        trace.text(),
        "2 narrowed to 1 then 2",
        "the executed chain starts with its caller's limit, and narrowing its own leaves the \
         caller's alone"
    );
}
