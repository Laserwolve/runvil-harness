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
