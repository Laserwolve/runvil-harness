---
name: Check four unchecked statements about tasks
overview: Four statements about Lua's `tasks` table have a part that no existing test checks. Each was shown by breaking the code and seeing every existing test still pass. This plan adds checks to three existing tests so that each break fails a test. It adds no test file and no test function, and changes no production code.
todos:
  - id: plan-copy
    content: "Copy this plan to plan/2026-11-18-1-tasks-unchecked-statements.plan.md on a new branch and commit it alone"
    status: completed
  - id: ownership
    content: "Add the non-owner `ready` and `status` checks to a_non_owner_is_refused_with_task_not_owned"
    status: completed
  - id: owner-ended
    content: "Give the abandoned task of a_returning_owner_abandons_its_running_task_as_owner_returned a started task of its own, and check how that task ends"
    status: completed
  - id: owner-canceled
    content: "Give the canceled task of cancel_ends_a_parked_task_idempotently_and_reports_task_canceled_once a started task of its own, and check how that task ends"
    status: completed
  - id: breaks
    content: "Apply each of the five breaks, confirm a test now fails, and revert it"
    status: completed
  - id: verify
    content: "Run every verification command and vibelight"
    status: completed
  - id: pr
    content: "Push the branch to the fork and open one PR against cppalliance/runvil-harness"
    status: in_progress
---

# Check four unchecked statements about tasks

<product-contract>

## Product Requirements

A list of 29 statements describes how Lua's `tasks` table behaves. Existing tests check 25 of them in full. The other four each have one part that nothing checks: the code can be broken there and all 4,052 existing tests still pass. The code is correct today, so this is missing protection, not a bug. This plan adds the missing checks to tests that already exist.

- Problem and users:
  - Item 18 says `tasks.cancel(task)` stops a task "and every task it owns". Nothing checks the second half.
  - Item 22 says only the owner may "check" a task. Nothing checks, through the Lua calls, that `tasks.ready` and `tasks.status` refuse a caller that is not the owner.
  - Item 25 says a task still live when its owner ends is abandoned "with every task under it". Nothing checks the second half.
  - Item 27 says each task that starts reports "exactly one terminal event". Nothing checks it for a task whose owner was stopped.
  - The users are the engineers who will build on tasks, and who rely on the tests to notice a regression.
- Goals:
  - Each of the five breaks listed in the Functional Specification makes at least one test fail.
  - The checks go into three existing tests. No test file and no test function is added.
- Non-goals:
  - Any change to production code.
  - New kinds of test, such as prompt-and-log pairs.
  - The unchecked detail that a second `tasks.note` replaces the first. It is outside the wording of the 29 statements.
- Success criteria:
  - With each break applied in turn, `cargo nextest run -p runvil-runtime` fails, and it passes again once the break is reverted.
  - The workspace has the same number of tests before and after.
  - Every verification command in the Testing Plan passes.
- Constraints:
  - Every assertion the three tests make today stays. One is narrowed from "no task" to "the canceled task", because the test gains a second task.
- Open questions: None

## Functional Specification

Three tests gain a task or two and the assertions about them. Nothing a user of Runvil sees changes.

- Actors and workflows:
  - A developer breaks one of the five spots below, runs the tests, and now sees a failure that names what broke.
- Inputs and outputs:
  - The five breaks, each a change to one function under `crates/runvil-internal/runtime/src/execute/scheduler/`:

    | Break | File and function | The change | Items |
    |---|---|---|---|
    | 1 | `chain.rs`, `abort_subtree` | The tasks a stopped chain owns are left running: the call to `abandon_owned_tasks` is removed. | 18, 25 |
    | 2 | `task_end.rs`, `abandon_owned_tasks` | A task abandoned as `OwnerAborted` reports no ending event. | 27 |
    | 3 | `chain.rs`, `abort_subtree` | The reason passed is `OwnerReturned` in place of `OwnerAborted`. | 25, 27 |
    | 4 | `waits.rs`, `dispatch_ready` | The owner check is replaced by one that admits any caller. | 22 |
    | 5 | `waits.rs`, `task_status` | The same, for `tasks.status`. | 22 |

  - What each test gains, in `crates/runvil-internal/runtime/src/execute/tests/`:

    | Test | What it gains | Breaks it catches |
    |---|---|---|
    | `a_non_owner_is_refused_with_task_not_owned` in `waits.rs` | A second task, the caller's sibling. The caller's `tasks.ready` on itself and on the sibling, and its `tasks.status` on the sibling, each raise `task_not_owned`. | 4, 5 |
    | `a_returning_owner_abandons_its_running_task_as_owner_returned` in `tasks-chain-end.rs` | The abandoned task owns a task that has started. That inner task ends `Abandoned`, reports one start and one ending, the ending names `OwnerAborted`, and it comes before its owner's ending. | 1, 2, 3 |
    | `cancel_ends_a_parked_task_idempotently_and_reports_task_canceled_once` in `waits.rs` | The canceled task owns a task that has started. The same four checks on that inner task. | 1, 2, 3 |

- States and validation: None
- Errors and recovery: None
- Security and privacy behavior: None
- Acceptance criteria:
  - Each break fails at least one of the three tests, as the last column above says.
  - The three tests pass on unbroken code.

</product-contract>
<implementation-contract>

## Technical Design

The change is confined to two test files and this plan. It reuses the helpers those files already use.

- Architecture: None
- Modules and interfaces:
  - The sibling in the ownership test is spawned second, so the caller stays task `0.0`, which the test already asserts, and the caller names its sibling by the id `0.1`.
  - In both stopped-owner tests the inner task parks the same way its owner does, on the same held effect, so it is running when the stop lands.
  - Both stopped-owner tests make their checks on the inner task through one helper, `TaskRecorder::assert_aborted_with_its_owner` in `crates/runvil-internal/runtime/src/execute/tests/tasks.rs`. It requires the task's observations to be exactly one start and one `TaskAbandoned` naming `OwnerAborted`, and that ending to come before its owner's.
- File and public API changes:
  - Changed: `waits.rs`, `tasks-chain-end.rs`, and `tasks.rs` in `crates/runvil-internal/runtime/src/execute/tests/`. The third gains the helper.
  - New: `plan/2026-11-18-1-tasks-unchecked-statements.plan.md`, this plan.
  - Public API: none.
- Data, persistence, failure, security, and privacy constraints: None

</implementation-contract>
<verification-contract>

## Testing Plan

The proof that the checks work is the five breaks: each must pass unnoticed before the change and fail a test after it. Before the change, all five passed 4,052 tests.

- Unit: None
- Integration and end-to-end:
  - For each break: apply it, run `cargo nextest run --locked -p runvil-runtime`, record the test that fails, and revert it.
- Regression, security, and performance:
  - `cargo nextest run --locked --workspace --all-features`
  - `cargo clippy --workspace --all-targets --all-features` with `CARGO_BUILD_WARNINGS=deny`
  - `cargo fmt --all --check`
  - `cargo doc --workspace --no-deps --all-features` and `cargo doc -p runvil-engine --no-deps`, both with `RUSTDOCFLAGS="-D warnings"`
  - `cargo +<nightly> xtask api --check`, with the nightly named in `crates/build-xtask/src/api/toolchain.rs`
  - `cargo dylint --all -- --locked --workspace --all-targets --all-features`
  - `cargo test -p build-xtask`
  - `node --test tools/docs-claims.mjs`
  - `vibelight` in the repository root
- Exit criteria:
  - All five breaks fail a test, and the working tree holds no break afterwards.
  - Every command above passes.
  - The count of tests in the workspace is unchanged.

</verification-contract>
<decision-record>

## Decision Record

- Decisions:
  - Add checks to existing tests, and add no test. Andrew, on a larger set of new tests for behavior that was already covered: "we shouldn't be raising a PR that introduces duplicate coverage". His request here: "raise a PR that adds coverage for those four items".
  - Cover both ways an owner is stopped, a cancel and the owner's own end. Items 18 and 25 each name one of them, and both go through `abort_subtree`.
  - Leave the second `tasks.note` out. Andrew: "we only care about those 29 in that list".
  - Change no production code. Every break was reverted, and the behavior the new checks pin is the behavior the code has today.
- Rejected alternatives:
  - Thirty prompt-and-log tests through the Harness. They were written and withdrawn: existing tests already checked 25 of the 29 statements in full. Revisit only if the readable form is wanted for its own sake.
  - New test functions for the stopped-owner cases. Revisit for the cancel case alone if giving the existing cancel test an inner task cannot keep its present assertions.
  - Changing `aborting_a_chain_abandons_the_tasks_it_owns`. Its inner task never starts, and making it start would drop the check that a task which never ran reports nothing.
- Assumptions, risks, and notes:
  - `aborting_a_chain_abandons_the_tasks_it_owns` in `tasks-chain-end.rs` passes even under break 1. The end of the run marks every leftover task abandoned, so the state it asserts is reached without the stop doing anything. The test is left as it is; the two stopped-owner tests now prove what its name says.
  - `OwnerAborted` is the one abandon reason of five that no test names today.
  - The cancel test steps its run by hand and holds one store write. The inner task needs a second held write, and it must be admitted before the cancel. If that cannot be arranged without weakening the test, the fallback above applies and is recorded here.
  - A wait, a send, and a cancel by a caller that is not the owner are already checked from Lua, so the ownership test gains only `ready` and `status`.
  - What the build found: the cancel test took an inner task without the fallback. Its last assertion, that no task reports another terminal, became the exact list of the canceled task's observations, one start and one `TaskCanceled`.
  - The helper exists because the cancel test, with the checks written out, was 107 lines against the limit of 100 on a function. Both tests now share it.
  - The five breaks after the change, each run with `cargo nextest run --locked -p runvil-runtime`: breaks 1, 2, and 3 each fail `a_returning_owner_abandons_its_running_task_as_owner_returned` and `cancel_ends_a_parked_task_idempotently_and_reports_task_canceled_once`, and breaks 4 and 5 each fail `a_non_owner_is_refused_with_task_not_owned`. Every break was reverted.
  - Verification on 2026-11-18: the workspace has 4,049 passing tests, the same count as before, and the lint, format, docs, facade surface, structural, and wording commands pass. `cargo dylint` did not run, because it is not installed on this machine.
  - `vibelight` outlines a crate's `src`, so its run over the workspace says nothing about test files. Run on `waits.rs` and `tasks-chain-end.rs` alone, it gave two leads for each, the same two as before the change, all on code that was already there.

### Deferred and Out of Scope

- Deferred: a check that a second `tasks.note` replaces the first, until someone asks for it.
- Out of scope: any test of tasks that the model launches.

</decision-record>
<project-survey>

## Project Survey

- The three tests and their helpers are in `crates/runvil-internal/runtime/src/execute/tests/waits.rs` (`tasks_prompt`) and `tasks-chain-end.rs` (`run_abandoning_a_running_task`, `CHILD_PARKED_ON_HELD_ROUND`).
- `TaskRecorder` in `crates/runvil-internal/runtime/src/execute/tests/tasks.rs` gives a run's task observations: `observations_of`, `records`, `logs`, and `position`.
- `Rig` in `crates/runvil-internal/runtime/src/test_support/rig.rs` steps a run, holds chosen effects, and reads a task's state with `task_state_for_test`.
- The stop of a chain's own tasks is `abort_subtree` in `crates/runvil-internal/runtime/src/execute/scheduler/chain.rs`, which calls `abandon_owned_tasks` in `task_end.rs`.
- The owner checks are `TaskArena::owned` and `TaskArena::visible` in `crates/runvil-internal/runtime/src/execute/scheduler/tasks.rs`, called from `dispatch_ready` and `task_status` in `waits.rs` beside it.

</project-survey>
<execution-plan>

## Execution Instructions

- Work items:
  1. Cut a branch from upstream `master`, copy this plan to `plan/2026-11-18-1-tasks-unchecked-statements.plan.md`, and commit it alone.
  2. Change the three tests as the Functional Specification says, in one commit.
  3. Apply each break in turn, run the crate's tests, record which test fails, and revert. Write the results into this plan.
  4. Run every command of the Testing Plan, then `vibelight`.
  5. Push the branch to the fork and open one pull request against `cppalliance/runvil-harness` `master`. Andrew tags the human reviewers and posts the link in the channel.
- Dependencies and verification:
  - Item 3 follows item 2, and the working tree is checked clean of breaks before item 4.
  - Commits use the GitHub no-reply address, and nothing is pushed to the cppalliance repository.

</execution-plan>
