---
name: Check that a round's options and tool schemas reach the broker
overview: The Harness answers each model round by calling the broker its Host supplies. It hands the broker the round's messages, its options, and the schemas of its tools. Tests check the messages. Every test broker ignores the options and the tool schemas, so the Harness can drop either and every test still passes. This plan adds one test, in the file that already checks what the broker receives. It changes no production code.
todos:
  - id: plan-copy
    content: "Copy this plan to plan/2026-11-20-6-round-options-and-tools-reach-the-broker.plan.md on the branch qa-plan-tests, which holds every step of this effort, and commit it alone"
    status: completed
  - id: test
    content: "Make the recording broker in crates/harness-internal/runner/tests/it/effect_loop-broker.rs keep each round's options and tool names, and add the test"
    status: pending
  - id: breaks
    content: "Apply each of the two breaks, confirm the test now fails, and revert it"
    status: pending
  - id: verify
    content: "Run every verification command and vibelight"
    status: pending
  - id: pr
    content: "Push the branch qa-plan-tests to the fork and open one PR for the whole effort against cppalliance/runvil-harness"
    status: pending
---

# Check that a round's options and tool schemas reach the broker

<product-contract>

## Product Requirements

A prompt sets how a model round is made: its role can turn thinking off, `models.use` can set a temperature and a token cap, and a section can offer tools. The Engine puts all of that in the round, and the Harness hands it to the broker in `perform`, in `crates/harness-internal/runner/src/effect_loop.rs`. Tests check each end: Engine tests check what the round holds, and the Gateway client's tests check what it sends for the options it is given. No test checks the hand-off between them, so it can be broken while all 3,986 existing tests pass. The code is correct today, so this is missing protection, not a bug.

- Problem and users:
  - With the options replaced by a set that keeps only the model's name, the temperature, the token cap, and the thinking switch never reach the broker. Every test passes.
  - With the tool schemas replaced by an empty list, the model is never told a tool exists. Every test passes.
  - The users are prompt authors, whose settings must reach the model, and the engineers who change the Harness and rely on its tests.
- Goals:
  - Each of the two breaks listed in the Functional Specification makes at least one test fail.
- Non-goals:
  - Any change to production code.
  - A test of the Engine's side or of the Gateway client's side. Both are covered today.
  - A test over HTTP. The hand-off is one call, and a broker that records what it is given pins it.
- Success criteria:
  - With each break applied in turn, `cargo nextest run -p harness-runner` fails, and it passes again once the break is reverted.
  - The workspace has exactly one more test than before: 3,987 run, up from 3,986.
  - Every verification command in the Testing Plan passes.
- Constraints:
  - The two existing tests in `effect_loop-broker.rs` keep every assertion they make today.
- Open questions: None

## Functional Specification

One test runs a prompt of one model round through the effect loop, with a broker that keeps what each round hands it. Nothing a user of Runvil sees changes.

- Actors and workflows:
  - A developer or CI runs the test suite. No other actor is involved.
- Inputs and outputs:
  - The prompt declares its `default` role with `thinking: false`. Its one section selects `default` with a temperature of 0.25 and a token cap of 64, defines an adhoc tool named `grab`, offers it, and makes one round.
  - The broker is handed one round. Its options turn thinking off and hold the temperature 0.25 and the token cap 64. Its tools are the one schema named `grab`.
- States and validation: None
- Errors and recovery: None
- Security and privacy behavior: None
- Acceptance criteria:
  - Break 1: in `perform`, change `binding.completion_options()` to `CompletionOptions::new(binding.completion_options().model())`. The test fails.
  - Break 2: in `perform`, pass `Vec::new()` to `broker.chat` in place of `tools`. The test fails.
  - With either break applied, the new test is the only test in the workspace that fails.

</product-contract>
<implementation-contract>

## Technical Design

The change is test code in one existing file. No module, interface, or stored format changes.

- Architecture:
  - The test drives a run through `drive_run`, as the two existing tests of the file do.
- Modules and interfaces: None
- File and public API changes:
  - In `crates/harness-internal/runner/tests/it/effect_loop-broker.rs`, the recording broker keeps each round's options and the names of its tools, beside the round and last message it keeps today. The file gains one prompt, one helper that builds a run over a given prompt, and one test. The existing helper builds its run through the new one.
  - `plan/2026-11-20-6-round-options-and-tools-reach-the-broker.plan.md` is this plan.
  - No public API changes, so `crates/runvil-engine/public-api.txt` and `crates/runvil/public-api.txt` stay as they are.
- Data, persistence, failure, security, and privacy constraints: None

</implementation-contract>
<verification-contract>

## Testing Plan

The change is itself a test, so its own check is that each break fails it. The repository's full verification list runs once at the end.

- Unit: None
- Integration and end-to-end:
  - `cargo nextest run --locked -p harness-runner -E 'test(effect_loop::broker)'` runs the three tests of the file: two existing and one new.
- Regression, security, and performance:
  - Apply each of the two breaks in turn, run the whole workspace, see the new test fail and every other test pass, and revert the break.
- Exit criteria:
  - `cargo nextest run --locked --workspace --all-features`
  - `cargo clippy --workspace --all-targets --all-features`, with `CARGO_BUILD_WARNINGS=deny`
  - `cargo fmt --all --check`
  - `cargo doc --workspace --no-deps --all-features` and `cargo doc -p runvil-engine --no-deps`, both with `RUSTDOCFLAGS="-D warnings"`
  - `cargo +<nightly> xtask api --check`, with the nightly named in `crates/build-xtask/src/api/toolchain.rs`
  - `cargo dylint --all -- --locked --workspace --all-targets --all-features`
  - `cargo test -p build-xtask`
  - `node --test tools/docs-claims.mjs`
  - `vibelight`, run in the repository root

</verification-contract>
<decision-record>

## Decision Record

- Decisions:
  - One new test function, in the file whose subject is what the broker receives. Its two existing tests run a prompt that sets no option and offers no tool, so neither can show the hand-off.
  - The existing recording broker gains the two things it ignored, in place of a second broker. One broker then records everything a round hands over.
  - The tool is an adhoc tool, defined in the prompt's Lua. It needs no Pack, so the test needs no tool performer.
  - Both breaks were shown to pass every test before the test was written. The user's rule, given on 2026-11-18: "If existing tests already cover these behaviors, we shouldn't be raising a PR that introduces duplicate coverage."
- Rejected alternatives:
  - A test that joins the Harness and the Gateway client against a stub HTTP server. It would cover this hand-off and repeat what the client's own tests check. Revisit if a break is found that only the joined test would notice.
- Assumptions, risks, and notes:
  - The Engine's side is checked by `models_use_forwards_binding_completion_options_to_the_gateway`, and the client's side by `complete_sends_completion_options_and_stream_flags_on_the_wire`.
  - Dropping a round's messages already fails at least eleven tests, so the messages need no new check.
  - The model's name in a round's options is checked by the tests of `plan/2026-11-20-5-model-the-broker-is-asked-for.plan.md`, so this test leaves it out.

### Deferred and Out of Scope

- Out of scope: every other test of the QA plan of attack.

</decision-record>
<project-survey>

## Project Survey

None

</project-survey>
<execution-plan>

## Execution Instructions

- Work items:
  - Commit this plan alone on the branch `qa-plan-tests`, after the commits of the plan before it, `plan/2026-11-20-5-model-the-broker-is-asked-for.plan.md`. One branch holds every step of this effort.
  - Change the recording broker and add the prompt, the helper, and the test in `crates/harness-internal/runner/tests/it/effect_loop-broker.rs`.
  - Apply and revert each break, recording which test fails.
  - Run the exit criteria.
  - Commit the test. Push the branch to the fork and open the PR only when the user asks.
- Dependencies and verification:
  - The test depends on nothing outside the repository.
  - The breaks are never committed. `git status` shows no change to `effect_loop.rs` before each commit.

</execution-plan>
