//! The golden tests of Lua's `tasks` table: each test runs one prompt of
//! `tests/golden/tasks/` through a Harness and requires the log stored
//! beside it. The plan `plan/2026-11-18-1-tasks-golden-tests.plan.md`
//! lists what each prompt does and what its log must show.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::golden::{blessing, check, folder};

#[tokio::test(start_paused = true)]
async fn a_spawned_task_is_joined() {
    check("01-spawn-and-join").await;
}

#[tokio::test(start_paused = true)]
async fn a_task_survives_its_owners_move_to_the_next_section() {
    check("02-survives-fall-through").await;
}

#[tokio::test(start_paused = true)]
async fn a_task_stays_queued_until_its_owner_waits() {
    check("03-queued-until-owner-waits").await;
}

#[tokio::test(start_paused = true)]
async fn a_task_sees_its_inputs_as_they_were_at_the_spawn() {
    check("04-inputs-copied-at-spawn").await;
}

#[tokio::test(start_paused = true)]
async fn a_limit_of_one_starts_the_second_task_after_the_first_ends() {
    check("05-limit-of-one").await;
}

#[tokio::test(start_paused = true)]
async fn the_default_limit_starts_eight_of_nine_tasks() {
    check("06-default-limit-of-eight").await;
}

#[tokio::test(start_paused = true)]
async fn join_returns_two_tool_loops_results_in_the_order_listed() {
    check("07-join-two-tool-loops").await;
}

#[tokio::test(start_paused = true)]
async fn join_any_returns_the_first_task_to_end() {
    check("08-join-any-first-to-end").await;
}

#[tokio::test(start_paused = true)]
async fn a_wait_returns_a_tasks_text_and_a_failed_tasks_error_as_data() {
    check("09-result-and-error").await;
}

#[tokio::test(start_paused = true)]
async fn a_second_wait_on_a_delivered_task_is_refused_as_consumed() {
    check("10-wait-twice").await;
}

#[tokio::test(start_paused = true)]
async fn an_owner_reads_a_tasks_store_write_after_joining_it() {
    check("11-store-after-join").await;
}

#[tokio::test(start_paused = true)]
async fn a_timed_join_ends_and_a_later_join_collects_the_task() {
    check("12-timed-join").await;
}

#[tokio::test(start_paused = true)]
async fn pending_lists_the_tasks_still_live() {
    check("13-pending").await;
}

#[tokio::test(start_paused = true)]
async fn ready_reads_false_before_a_task_ends_and_true_after() {
    check("14-ready-after-end").await;
}

#[tokio::test(start_paused = true)]
async fn ready_reads_true_while_a_reply_waits_and_false_once_it_is_taken() {
    check("15-ready-with-reply").await;
}

#[tokio::test(start_paused = true)]
async fn status_reads_queued_then_chat_then_done_around_a_model_call() {
    check("16-status-through-a-model-call").await;
}

#[tokio::test(start_paused = true)]
async fn status_shows_the_latest_note_a_task_set() {
    check("17-note").await;
}

#[tokio::test(start_paused = true)]
async fn cancel_stops_a_queued_task_a_waiting_task_and_a_task_that_owns_a_task() {
    check("18-cancel").await;
}

#[tokio::test(start_paused = true)]
async fn an_owner_steers_a_worker_between_its_replies() {
    check("19-steer-with-replies").await;
}

#[tokio::test(start_paused = true)]
async fn receive_returns_messages_oldest_first_and_waits_for_the_next() {
    check("20-receive-oldest-first").await;
}

#[tokio::test(start_paused = true)]
async fn a_task_that_acts_on_its_sibling_is_refused_as_not_the_owner() {
    check("21-sibling-not-owned").await;
}

#[tokio::test(start_paused = true)]
async fn a_task_that_cancels_itself_or_a_made_up_id_is_refused_as_not_the_owner() {
    check("22-self-and-unknown-not-owned").await;
}

#[tokio::test(start_paused = true)]
async fn a_task_reads_its_own_status() {
    check("23-own-status").await;
}

#[tokio::test(start_paused = true)]
async fn a_chain_that_returns_with_a_live_task_fails_and_the_task_is_abandoned() {
    check("24-return-with-live-task").await;
}

#[tokio::test(start_paused = true)]
async fn a_chain_that_fails_with_a_live_task_keeps_its_own_error() {
    check("25-error-with-live-task").await;
}

#[tokio::test(start_paused = true)]
async fn a_chain_that_returns_with_a_task_waiting_after_a_reply_ends_normally() {
    check("26-return-with-waiting-task").await;
}

#[tokio::test(start_paused = true)]
async fn a_tasks_task_is_numbered_under_it() {
    check("27-nested-ids").await;
}

#[tokio::test(start_paused = true)]
async fn a_call_an_execute_and_a_timed_wait_each_take_a_child_number() {
    check("28-ids-skip-other-children").await;
}

#[tokio::test(start_paused = true)]
async fn two_runs_of_one_prompt_give_the_same_log() {
    let first = check("29-same-log-twice").await;
    let second = check("29-same-log-twice").await;
    assert_eq!(first, second, "the second run's log differs from the first");
}

/// The task events of `log`, in order, each without its provenance: the
/// starts and endings of tasks, apart from where their chains stood.
fn task_events(log: &str) -> Vec<Value> {
    log.lines()
        .filter_map(|line| line.splitn(5, ' ').nth(4))
        .filter_map(|payload| serde_json::from_str::<Value>(payload).ok())
        .filter(|payload| {
            payload["kind"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("task_"))
        })
        .map(|mut payload| {
            if let Some(fields) = payload.as_object_mut() {
                fields.remove("provenance");
            }
            payload
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn a_fanout_reports_the_task_events_of_the_same_spawns_made_by_hand() {
    let fanout = task_events(&check("30a-fanout").await);
    let by_hand = task_events(&check("30b-spawns-by-hand").await);
    assert_eq!(
        fanout.len(),
        6,
        "three starts and three endings: {fanout:?}"
    );
    assert_eq!(fanout, by_hand);
}

/// The stems of the files in the golden folder with the extension
/// `extension`.
fn stems(extension: &str) -> BTreeSet<String> {
    let entries = match std::fs::read_dir(folder()) {
        Ok(entries) => entries,
        Err(error) => panic!("the golden folder is listed: {error}"),
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|found| found == extension))
        .filter_map(|path| Some(path.file_stem()?.to_str()?.to_owned()))
        .collect()
}

#[test]
fn every_golden_prompt_has_a_stored_log_and_a_test_that_names_it() {
    let prompts = stems("md");
    // While the tests beside this one are storing their logs, the folder
    // holds only the logs stored so far.
    let logs = if blessing() {
        prompts.clone()
    } else {
        stems("log")
    };
    assert_eq!(
        prompts, logs,
        "each prompt has one stored log, and each stored log one prompt"
    );
    let tests = include_str!("tasks.rs");
    for stem in &prompts {
        assert!(
            tests.contains(&format!("check(\"{stem}\")")),
            "no test in tasks.rs runs {stem}.md"
        );
    }
}
