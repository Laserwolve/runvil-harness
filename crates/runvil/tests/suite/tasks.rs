//! The golden tests of Lua's `tasks` table: each test runs one prompt of
//! `tests/golden/tasks/` through a Harness and requires the log stored
//! beside it. The plan `plan/2026-11-18-1-tasks-golden-tests.plan.md`
//! lists what each prompt does and what its log must show.

use crate::golden::check;

#[tokio::test(start_paused = true)]
async fn a_spawned_task_is_joined() {
    check("01-spawn-and-join").await;
}
