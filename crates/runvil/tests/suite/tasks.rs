//! The golden tests of Lua's `tasks` table: each test runs one prompt of
//! `tests/golden/tasks/` through a Harness and requires the log stored
//! beside it. The plan `plan/2026-11-18-1-tasks-golden-tests.plan.md`
//! lists what each prompt does and what its log must show.

use crate::golden::check;

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
