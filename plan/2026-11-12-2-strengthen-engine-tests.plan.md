---
name: Strengthen Engine tests
overview: Make existing Engine tests fail when the code they run is broken. Mutation testing found 89 changes to production code, outside `runvil-vfs`, that an existing test runs and every test still passes. At `a449a1bcc`, 84 of them are still in the code and 78 still pass every test. Checks and input cases added to 41 existing tests now catch 55 of the 78. No test is added and no production code changes.
todos:
  - id: strengthen-tests
    content: "Strengthen the existing tests in `runvil-lua`, `runvil-runtime`, `runvil-prompt`, `runvil-types`, `runvil-plugin`, `runvil-model-client`, and `runvil-engine`, in one commit with this plan"
    status: completed
isProject: false
---

# Strengthen Engine tests

<product-contract>

## Product Requirements

A test that runs a line does not always fail when the line is wrong. Mutation testing finds those cases: it makes one small change to the production code at a time and reports each change that every test still passes. Per-test coverage then names the tests that run the changed line and pass anyway. This plan strengthens those tests, and changes nothing else.

- Problem and users:
  - The Engine was measured in `cppalliance/runvil`, where it lived until 2026-11-12. The first two commits named below are that repository's. This repository at `cc51d454` held the same Engine crates, byte for byte, as `cppalliance/runvil` at `2d40bafa2`.
  - On 2026-11-11 at `fa9936f64`, `cargo-mutants` 27.1.0 made 4,178 single changes to the Engine's production code, each judged by every Engine test. Outside `runvil-vfs`, 145 passed every test.
  - On 2026-11-12 at `2d40bafa2` the 145 were tested again. Two fail a test there. Of the other 143, 89 are on a line that at least one of the 2,638 Engine tests runs, and 54 are on a line no test runs.
  - The 89 are tests that pass with the code they run broken: 42 in `runvil-lua`, 23 in `runvil-runtime`, 14 in `runvil-prompt`, 7 in `runvil-types`, 2 in `runvil-plugin`, and 1 in `runvil-model-client`.
  - On 2026-11-16 the 89 were matched to this repository at `a449a1bcc`, 28 commits after `cc51d454`. Five are gone with the code they changed: `route_store_to_shims` in `runvil-lua`, `default_matches`, `yaml_type`, and the H1 filter of `Prompt::parse_body` in `runvil-prompt`, and the `is_h1` guard of `Scheduler::apply_settled` in `runvil-runtime`. The other 84 are on the same source line as before, 44 of them at a new line number.
  - Tested at `a449a1bcc` without this change, 6 of the 84 fail a test and 78 pass every test. The 6 fail tests that were added or reworked after `cc51d454`.
  - The users are the maintainers and reviewers, who rely on these tests to catch a regression.
- Goals:
  - Each of the 78 that alters behavior fails an existing test.
- Non-goals:
  - New tests.
  - Any change to production code.
  - `runvil-vfs`.
  - The 54 changes on lines no test runs.
  - Mutation testing in CI.
- Success criteria:
  - Testing the 84 again on the changed tree leaves only the ones the Decision Record lists.
  - The Engine has the same 2,958 tests before and after.
- Constraints:
  - Only existing test functions change: a check is added on a value the test already produces, or an input case is added to the test.
  - A test's name stays true to what it checks.
- Open questions: None

## Functional Specification

No product behavior changes. Each added check states a behavior the code already has.

- Acceptance criteria:

  | Crate | Still in the code | Fail a test without this change | Pass every test | Now fail a test | Alter no behavior | No fitting test |
  |---|---|---|---|---|---|---|
  | `runvil-lua` | 41 | 1 | 40 | 24 | 14 | 2 |
  | `runvil-runtime` | 22 | 4 | 18 | 15 | 2 | 1 |
  | `runvil-prompt` | 11 | 1 | 10 | 10 | 0 | 0 |
  | `runvil-types` | 7 | 0 | 7 | 4 | 1 | 2 |
  | `runvil-plugin` | 2 | 0 | 2 | 1 | 1 | 0 |
  | `runvil-model-client` | 1 | 0 | 1 | 1 | 0 | 0 |
  | Total | 84 | 6 | 78 | 55 | 18 | 5 |

  - The last three columns add up to the fourth: of the 78 that pass every test at `a449a1bcc`, 55 now fail a test, 18 alter no behavior, and 5 have no fitting test.
  - 41 existing tests gain checks or input cases: 19 in `runvil-lua`, 9 in `runvil-runtime`, 6 in `runvil-prompt`, 4 in `runvil-types`, and 1 each in `runvil-plugin`, `runvil-model-client`, and `runvil-engine`.
  - The Decision Record names the 18 that alter no behavior and the 5 with no fitting test.

</product-contract>
<implementation-contract>

## Technical Design

Test code only: 35 files in seven crates, 345 lines added and 33 removed. No public item, production line, or dependency changes.

- File and public API changes:
  - `crates/runvil-internal/lua/src`: 18 files, among them the test modules of `coro.rs` and `program.rs`.
  - `crates/runvil-internal/runtime/src`: 6 files under `execute/`, and `subst-tests.rs`.
  - `crates/runvil-internal/prompt/src`: `tests/section_fences.rs`, `contract/tests.rs`, and the test module of `body_text.rs`.
  - `crates/runvil-internal/types/src`: `error-tests.rs`, `names-tests.rs`, and the test modules of `models.rs` and `untrusted_inventory.rs`.
  - `crates/runvil-plugin/src/service.rs`, `crates/runvil-internal/model-client/src/client-tests.rs`, and `crates/runvil-engine/tests/suite/vfs.rs`: one test each.

</implementation-contract>
<verification-contract>

## Testing Plan

The measurement that found the 89 is also the check that they are closed. Each one still in the code was applied to the tree before and after this change and judged by every Engine test.

- Regression, security, and performance:

  | Tree | Tested | Fail a test | Pass every test |
  |---|---|---|---|
  | `cppalliance/runvil` at `2d40bafa2` | 89 | 0 | 89 |
  | This repository at `a449a1bcc` | 84 | 6 | 78 |
  | This repository at `a449a1bcc` with this change | 84 | 61 | 23 |

  The 61 are the 6 that fail without this change and the 55 this change adds.

  How the tests were found: every Engine test ran once under coverage with its own profile, which gives the tests that run each line. For each change, the test chosen is one whose subject is the changed code, in that code's own test module where there is one.

- Exit criteria:
  - `cargo nextest run --locked --workspace --all-features`: 3,931 tests, of which 2,958 are the Engine's, the counts without this change.
  - `CARGO_BUILD_WARNINGS=deny cargo clippy --locked --workspace --all-targets --all-features`
  - `cargo fmt --all --check`
  - `cargo test -p build-xtask`
  - `node --test tools/docs-claims.mjs`
  - The diff against `a449a1bcc` adds no file but this plan, adds or removes no `#[test]`, and touches no line outside test code.

</verification-contract>
<decision-record>

## Decision Record

- Decisions:
  - Only existing tests change. Rationale: the purpose is to repair tests that pass when the code they run is broken. A change on a line no test runs is missing coverage, which is a different problem with a different fix.
  - The test that gets the check is the one whose subject is the changed code, not every test that passes through the line. Rationale: one line is run by a median of 25 tests and by as many as 1,550, nearly all on their way to checking something else.
  - A new input case in an existing test counts as strengthening it. Rationale: several tests ran the line but never with an input on which the change makes a difference, such as two `lua` fences with no line between them.
  - One change for seven crates. Rationale: it is one kind of edit throughout and 35 files.
  - The check on the line `first_chunk_line` gives an origin is in `an_operation_watcher_sees_a_tool_calls_write_then_read_under_the_calls_origin`, in the `runvil-engine` suite. Rationale: since `ceb5da405` a run's store is no part of its handle, so the handle's watcher sees only a tool call's operations. That test is the one that reads a call's origin, and it checked the label and the file but not the line.
  - No check for a change that already fails a test at `a449a1bcc`. Rationale: five of the 6 had a check in the first version of this change, the four deleted arms of `vfs_observations` and `+` as `*` in `split_section_blocks` (`fence.rs:359`). Tests added after `cc51d454`, in `observations-store.rs` and `preamble_fences.rs`, now fail for them, so a second check would catch nothing more.
  - No test for the 18 changes that alter no behavior:
    - `runvil-lua`, 14: the seven changes to the library flags of the temporary VM in `compiler` (`program.rs:63` and `:64`), which only parses and dumps a chunk; `dispatch.rs:207`, whose guard is true for every error that can reach it; `json/encode.rs:458`, which zero never reaches; `models_loop/machine.rs:384` and `:387`, where the guard is already implied; `projection.rs:86`, where an empty batch accepts and rejects what no batch does; and two of the three changes at `tools/offer.rs:212`, which differ only for a negative length, where every form gives 0.
    - `runvil-runtime`, 2: `Admission::clear` (`admission.rs:285`), which runs only as a run ends; and the guard at `chain_record.rs:539`, which its one caller has already made true.
    - `runvil-types`, 1: `seed.rs:106`, `|` as `^` on two values with no bit in common.
    - `runvil-plugin`, 1: `registry.rs:276`, `Vec::new()` as `vec![]`.
  - No check for the 5 changes with no fitting test. Rationale: the tests that reach these lines are about something else, so a check there would make the test's name untrue.
    - `runvil-lua`, 2: `clear_jump` (`vm/run.rs:36`) and the guard at `vm/run.rs:285`, which differ only when a lock is poisoned.
    - `runvil-runtime`, 1: the guard in `Scheduler::finish` that ends the run when a chain ends with a determinism failure (`scheduler/chain.rs:201`). Every chain end runs the line, and at `a449a1bcc` no test in the workspace ends a chain with that failure: a `panic!` in the guarded arm fails none of the 3,931 tests. The one test that did, `a_prelude_write_across_two_arms_ends_the_run_with_determinism_caught_or_not`, was removed in `56a2e680e` with the store write of a prelude that reached the guard.
    - `runvil-types`, 2: `attribute_len` (`untrusted_inventory.rs:419`), which the tests reach without ever passing an attribute opener; and `Debug for ToolCatalog` (`tools/registry.rs:32`), reached only by a `runvil-runtime` test of another type's `Debug`.
- Rejected alternatives:
  - New tests for the 54 changes on lines no test runs and for the 5 with no fitting test. Reason: they are missing coverage, not weak tests. Revisit: as separate work on coverage.
  - A check in every test that runs a changed line. Reason: it would put unrelated assertions in hundreds of tests. Revisit: never.
- Assumptions, risks, and notes:
  - The measurement makes simple changes only: a swapped operator, a deleted `match` arm or negation, a function body replaced by a default value. A clean result does not show that the tests catch a subtler fault.
  - The list of changes comes from `fa9936f64`. Code added between it and `2d40bafa2` was measured only in `runvil-prompt`. Code added between `cc51d454` and `a449a1bcc` was not measured, and neither was `runvil-markdown`, which is newer than the list.
  - The measurement ran on Linux only.
  - The first version of this change, made against `cc51d454`, strengthened 45 tests. Four of them are not strengthened here. Two no longer exist at `a449a1bcc`: `an_arg_default_must_match_the_declared_type`, removed with `default_matches` and `yaml_type`, and the prelude test named above. Two had checks only for changes that already fail a test there: `the_records_span_the_complete_runtime_lifecycle`, and `malformed_section_phases_report_locations` with its `middle` case. A fifth, which read an origin through the watcher of a store, was rewritten in `ceb5da405`, and its check is now the one in the `runvil-engine` suite.
  - One of the 78, the guard at `execute/scheduler/effects.rs:290` in `runvil-runtime`, is counted as passing every test although it fails `a_message_to_one_task_does_not_withdraw_another_chains_request` in some runs of unchanged `a449a1bcc`: 15 of 40. `awaited_round` takes the first match in a `HashMap`, whose order differs from run to run. With this change it failed in 40 of 40.
  - One of the 6, `call_optional_line` in `runvil-lua` (`protocol/parse/store.rs:17`), had no fitting test at `cc51d454`. The reworked tests in `tests/store.rs` now fail for it.

### Deferred and Out of Scope

- Deferred: `runvil-vfs`, which held 56 such changes at `fa9936f64`, until the maintainers say whether more tests there are wanted.
- Deferred: the 54 changes on lines no test runs, and the 5 with no fitting test, as coverage work.
- Deferred: `runvil-markdown`, until it is measured.
- Out of scope: mutation testing in CI.

</decision-record>
<project-survey>

## Project Survey

None

</project-survey>
<execution-plan>

## Execution Instructions

<step-1>

### Step 1: Strengthen the tests [completed]

- Component: `strengthen-tests`
- Tests:
  - The exit criteria in the Testing Plan

Build: for each of the 84 changes still in the code, add a check or an input case to the existing test whose subject is the changed code. Leave the 23 the Decision Record lists. Test the 84 again and record the rows in the Testing Plan.

Commit: one commit. Suggested subject: `Make existing Engine tests fail when the code they run is broken`.

</step-1>

</execution-plan>
