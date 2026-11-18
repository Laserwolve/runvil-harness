//! The store of an executed prompt: each `execute` gives the prompt it
//! runs a new, empty store, which its `call` chains and tasks share and
//! its caller never sees, and each `Vfs` record names its store by the id
//! of the chain the store was made for, so the records of one store
//! rebuild it. Every chain that shared the store lets go of it by the
//! time the executed prompt ends, finished or aborted.

use std::collections::BTreeMap;

use runvil_types::ids::ChainId;
use runvil_vfs::Store;

use super::*;
use crate::execute::run::{AnswerRecord, EffectRecord};
use crate::test_support::rig::Rig;
use crate::test_support::scenario::fresh_store_view;

#[test]
fn the_executed_prompt_starts_on_an_empty_store_and_its_caller_sees_none_of_its_files() {
    let main = main_listing(
        &["child.md"],
        "store.write('mine.md', 'caller')
         local seen = execute('child.md')
         return seen .. '|' .. tostring(store.exists('theirs.md')) .. '|' .. store.read('mine.md')
           .. '|' .. #store.glob('**')",
    );
    let child = child(
        "child",
        "local had = #store.glob('**')
         store.write('theirs.md', 'child')
         store.write('mine.md', 'the child wrote its own')
         return had .. ',' .. store.read('mine.md')",
    );
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(
        trace.text(),
        "0,the child wrote its own|false|caller|1",
        "one path is two files, one in each store"
    );
    assert_eq!(trace.read("mine.md").as_deref(), Ok("caller"));
    assert!(
        trace.read("theirs.md").is_err(),
        "the run's own store holds no file of the executed prompt"
    );
}

#[test]
fn a_prompt_executed_twice_starts_on_an_empty_store_both_times() {
    let main = main_listing(
        &["child.md"],
        "return execute('child.md') .. ' ' .. execute('child.md')",
    );
    let child = child(
        "child",
        "local had = store.exists('note.md')
         store.write('note.md', 'kept for this execution only')
         return tostring(had)",
    );
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(trace.text(), "false false");
}

#[test]
fn two_fanout_arms_that_execute_one_prompt_each_write_its_one_path_with_no_conflict() {
    // The arms are two tasks with no order between them. In one store
    // their writes of one path would be a determinism violation.
    let main = listing("t", &["child.md"])
        .section(
            "Main",
            "local arms = tasks.fanout('Arm', { 'a', 'b' })
             return arms[1].text .. '|' .. arms[2].text",
        )
        .section("### Arm", "return execute('child.md', item)")
        .build();
    let child = child(
        "child",
        "store.write('out.md', args.text)
         return store.read('out.md')",
    );
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(trace.text(), "a|b");
}

#[test]
fn a_call_and_a_task_of_the_executed_prompt_share_its_store() {
    let main = main_listing(
        &["child.md"],
        "store.write('shared.md', 'the caller has one too')
         return execute('child.md') .. ' / ' .. store.read('shared.md')",
    );
    let child = PromptSource::new()
        .name("child")
        .section(
            "Start",
            "store.write('shared.md', 'start')
             local from_call = call('../Reader')
             local t = tasks.spawn('../Appender')
             local _, ok, appended = tasks.join_any({ t })
             return from_call .. '|' .. tostring(ok) .. ' ' .. appended .. '|'
               .. store.read('shared.md')",
        )
        .section("Reader", "return 'read ' .. store.read('shared.md')")
        .section(
            "Appender",
            "store.append('shared.md', '+task')
             return 'appended'",
        )
        .build();
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(
        trace.text(),
        "read start|true appended|start+task / the caller has one too"
    );
}

#[test]
fn two_unordered_tasks_of_the_executed_prompt_still_conflict_on_one_path() {
    let main = main_listing(&["child.md"], "return execute('child.md')");
    let child = PromptSource::new()
        .name("child")
        .section(
            "Start",
            "local a = tasks.spawn('../Writer')
             local b = tasks.spawn('../Writer')
             tasks.join({ a, b })
             return 'unreachable'",
        )
        .section("Writer", "store.write('same.md', 'x')\nreturn 'wrote'")
        .build();
    let trace = Scenario::new(main).prompt("child.md", child).run();
    let error = trace.error();
    assert_eq!(
        error.code,
        ErrorCode::Determinism,
        "the executed prompt's store keeps its own claims: {error:?}"
    );
}

/// A prompt that executes `a.md`, which executes `b.md`: each writes
/// `f.md` in its own store and reads it back after the prompt it executed
/// returns.
fn three_deep() -> Scenario {
    let main = main_listing(
        &["a.md"],
        "store.write('f.md', 'main')
         local below = execute('a.md')
         return below .. ' main:' .. store.read('f.md')",
    );
    let a = listing("a", &["b.md"])
        .section(
            "Only",
            "local had = tostring(store.exists('f.md'))
             store.write('f.md', 'a')
             local below = execute('b.md')
             store.append('f.md', '+after')
             return had .. ' ' .. below .. ' a:' .. store.read('f.md')",
        )
        .build();
    let b = child(
        "b",
        "local had = tostring(store.exists('f.md'))
         store.write('f.md', 'b')
         store.write('only-b.md', 'b')
         return had .. ' b:' .. store.read('f.md')",
    );
    Scenario::new(main).prompt("a.md", a).prompt("b.md", b)
}

#[test]
fn a_prompt_that_executes_another_gives_it_a_third_store() {
    assert_eq!(
        three_deep().run().text(),
        "false false b:b a:a+after main:main"
    );
}

/// Every file `view` reaches beside its text, in path order.
fn files(view: &Access) -> Vec<(String, String)> {
    let paths = view.glob("**").expect("the rebuilt store lists its files");
    paths
        .into_iter()
        .map(|path| {
            let text = view
                .read_string(&path)
                .expect("the rebuilt store reads its file");
            (path, text)
        })
        .collect()
}

#[test]
fn the_vfs_records_of_each_store_id_rebuild_that_store() {
    let trace = three_deep().run();
    assert_eq!(trace.text(), "false false b:b a:a+after main:main");

    // Each operation the run performed, applied in record order to one
    // new store for each store id the records name.
    let mut rebuilt: BTreeMap<ChainId, Access> = BTreeMap::new();
    for (effect, answer) in trace.effects() {
        let (EffectRecord::Vfs { store, op }, AnswerRecord::Vfs(Ok(outcome))) = (effect, answer)
        else {
            continue;
        };
        let view = rebuilt.entry(store.clone()).or_insert_with(|| {
            fresh_store_view(&Store::new(), "a rebuilt store").expect("a new store gives a view")
        });
        assert_eq!(
            &view
                .perform(op)
                .expect("the recorded operation succeeds again"),
            outcome,
            "{op:?} on store {store} gives the outcome the run recorded"
        );
    }

    let one = |path: &str, text: &str| (path.to_owned(), text.to_owned());
    let root = ChainId::root();
    let expected = BTreeMap::from([
        (root.clone(), vec![one("f.md", "main")]),
        (root.child(0), vec![one("f.md", "a+after")]),
        (
            root.child(0).child(0),
            vec![one("f.md", "b"), one("only-b.md", "b")],
        ),
    ]);
    let rebuilt: BTreeMap<ChainId, Vec<(String, String)>> = rebuilt
        .iter()
        .map(|(store, view)| (store.clone(), files(view)))
        .collect();
    assert_eq!(
        rebuilt, expected,
        "the run's own store is `0`, and each executed prompt's is named by its chain"
    );
    assert_eq!(
        trace.read("f.md").as_deref(),
        Ok("main"),
        "the store the run starts with holds what the `0` records rebuild"
    );
}

/// Whether `effect` is the model round that asks `wait here`, which a
/// scenario holds to stop the run where a test reads its chains.
fn waits_here(effect: &Effect) -> bool {
    crate::execute::tests::scheduler::asks("wait here")(effect)
}

#[test]
fn every_chain_of_an_executed_prompt_lets_go_of_its_store_when_that_prompt_ends() {
    // The executed prompt, chain `0.0`, runs a `call` chain and a task
    // on its store and returns. The run's prompt then parks on a round
    // the test holds, with every chain of the executed prompt ended.
    let main = main_listing(
        &["child.md"],
        "store.write('mine.md', 'main')
         local done = execute('child.md')
         return done .. ' ' .. models.infer('wait here')",
    );
    let child = PromptSource::new()
        .name("child")
        .section(
            "Start",
            "store.write('a.md', 'start')
             call('../Called')
             tasks.join({ tasks.spawn('../Spawned') })
             return 'done'",
        )
        .section("Called", "store.write('b.md', 'call')\nreturn 'called'")
        .section("Spawned", "store.write('c.md', 'task')\nreturn 'spawned'")
        .build();
    let gateway = ScriptedChat::new(vec![text("released")]);
    let mut rig = Rig::start(
        Scenario::new(main)
            .bound()
            .prompt("child.md", child)
            .chat(&gateway)
            .hold(waits_here),
    );
    rig.settle();
    assert_eq!(
        rig.scheduler_for_test().held_stores_for_test(),
        ["0"],
        "only the root chain still holds a store, its own"
    );
    rig.release(waits_here);
    assert_eq!(rig.finish().trace().text(), "done released");
}

#[test]
fn an_aborted_executed_chain_lets_go_of_its_store() {
    // The task `0.0` executes a prompt that parks on a held round, and
    // the run's prompt cancels the task: the task's chain and the
    // executed chain under it are aborted, not finished.
    let main = listing("t", &["child.md"])
        .section(
            "Main",
            "local t = tasks.spawn('../Worker')
             store.write('park', 'x')
             tasks.cancel(t)
             tasks.join_any({ t })
             return models.infer('wait here')",
        )
        .section("Worker", "return execute('child.md')")
        .build();
    let child = child(
        "child",
        "store.write('kept.md', 'by the executed prompt')
         return models.infer('never answered')",
    );
    let gateway = ScriptedChat::new(vec![text("released")]);
    let never = crate::execute::tests::scheduler::asks("never answered");
    let mut rig = Rig::start(
        Scenario::new(main)
            .bound()
            .prompt("child.md", child)
            .chat(&gateway)
            .hold(move |effect| waits_here(effect) || never(effect)),
    );
    rig.settle();
    assert_eq!(
        rig.scheduler_for_test().held_stores_for_test(),
        ["0"],
        "the canceled task's chain and the executed chain hold no store"
    );
    rig.release(waits_here);
    assert_eq!(rig.finish().trace().text(), "released");
}

#[test]
fn a_store_made_for_an_execute_names_the_executed_chain_whichever_of_its_chains_writes() {
    // The executed prompt is chain `0.0`. Its `call` chain is `0.0.0` and
    // its task `0.0.1`, and every one of their operations is on the store
    // made for `0.0`.
    let main = main_listing(&["child.md"], "return execute('child.md')");
    let child = PromptSource::new()
        .name("child")
        .section(
            "Start",
            "store.write('a.md', 'start')
             call('../Called')
             tasks.join({ tasks.spawn('../Spawned') })
             return 'done'",
        )
        .section("Called", "store.write('b.md', 'call')\nreturn 'called'")
        .section("Spawned", "store.write('c.md', 'task')\nreturn 'spawned'")
        .build();
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(trace.text(), "done");
    let stores: Vec<String> = trace
        .effects()
        .iter()
        .filter_map(|(effect, _)| match effect {
            EffectRecord::Vfs { store, .. } => Some(store.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(stores, ["0.0", "0.0", "0.0"]);
}
