---
name: Check which model the broker is asked for
overview: A Host hands each run a broker and, if it wants, the name of a model. Every round then asks the broker for the model the Host selected, or for the first model the broker lists when the Host selected none. No test checks which model a round asks for, so the rule can be broken three ways and every test still passes. This plan adds two tests in which a Host sends the shipped `hello.md` to a broker that lists two models. It changes no production code.
todos:
  - id: plan-copy
    content: "Copy this plan to plan/2026-11-20-5-model-the-broker-is-asked-for.plan.md on the branch qa-plan-tests, which holds every step of this effort, and commit it alone"
    status: completed
  - id: tests
    content: "Make the scripted broker in crates/runvil/tests/suite/broker.rs keep the model each round asks for, and add two helpers and the two tests"
    status: completed
  - id: breaks
    content: "Apply each of the three breaks, confirm a test now fails, and revert it"
    status: completed
  - id: verify
    content: "Run every verification command and vibelight"
    status: completed
  - id: pr
    content: "Push the branch qa-plan-tests to the fork and open one PR for the whole effort against cppalliance/runvil-harness"
    status: pending
---

# Check which model the broker is asked for

<product-contract>

## Product Requirements

Two pieces of the Harness decide which model a round asks the broker for. `current_model` in `crates/harness-internal/runner/src/environment.rs` picks the model a run binds, and `perform` in `crates/harness-internal/runner/src/effect_loop.rs` hands each round to the broker with that model's name. Every test broker ignores the name it is given, and every test that lists models lists exactly one. So both pieces can be broken while all 3,984 existing tests pass. The code is correct today, so this is missing protection, not a bug.

- Problem and users:
  - A Host that selects a model expects its runs to ask for that model. With `current_model` changed to ignore a selection the list holds and bind the first listed model, every test passes.
  - A Host that selects no model gets the first model its broker lists, as the doc comment on `current_model` says. With `current_model` changed to bind the last listed model, every test passes.
  - With `perform` changed to hand the broker another model's name, every round asks for a model the run never bound. Every test passes.
  - The users are the engineers who write a Host, and the engineers who change the Harness and rely on its tests.
- Goals:
  - Each of the three breaks listed in the Functional Specification makes at least one test fail.
  - The tests read the result where a Host can see it: the model its broker is asked for.
- Non-goals:
  - Any change to production code.
  - A test over HTTP. The Gateway client's own tests check what it sends for the model it is given.
- Success criteria:
  - With each break applied in turn, `cargo nextest run -p runvil` fails, and it passes again once the break is reverted.
  - The workspace has exactly two more tests than before: 3,986 run, up from 3,984.
  - Every verification command in the Testing Plan passes.
- Constraints:
  - The existing test in `crates/runvil/tests/suite/broker.rs` keeps every assertion it makes today.
- Open questions: None

## Functional Specification

Two tests run the shipped prompt `prompts/hello.md` through the Harness, as a Host does, with a broker that lists two models, named `first` and `second`, in that order. The broker keeps the model each round asks it for. Nothing a user of Runvil sees changes.

- Actors and workflows:
  - A developer or CI runs the test suite. No other actor is involved.
- Inputs and outputs:
  - The Host selects no model: the run's one round asks the broker for `first`.
  - The Host selects `second`: the round asks for `second`.
  - The Host selects `first`: the round asks for `first`.
  - In each case the run ends with the broker's reply.
- States and validation: None
- Errors and recovery:
  - A missing `prompts/hello.md` fails the test with a message that names the path.
- Security and privacy behavior: None
- Acceptance criteria:
  - Break A: in `current_model`, change `catalog.models().first()` to `catalog.models().last()`. The no-selection test fails.
  - Break B: in `current_model`, change `catalog.get(&id)` to `catalog.get(&id).and(catalog.models().first())`, so that a selection the list holds binds the first listed model. The selection test fails.
  - Break C: in `perform`, change `binding.completion_options()` to `binding.completion_options().with_model("other")`. Both tests fail.
  - With any one break applied, the tests named above are the only tests in the workspace that fail.

</product-contract>
<implementation-contract>

## Technical Design

The change is test code in one existing file. No module, interface, or stored format changes.

- Architecture:
  - The tests are in the suite of the `runvil` crate, the one crate a Host depends on. Each builds a Host, hands the run a broker, and runs the prompt to its end, as the existing test of the file does.
- Modules and interfaces: None
- File and public API changes:
  - In `crates/runvil/tests/suite/broker.rs`, the scripted broker keeps the model each round asks for, beside the rounds it keeps today. The file gains one helper that builds a scripted broker from a list of model names, one helper that sends `hello.md` with a given selection and returns what the broker was asked for, and two tests. The existing test builds its broker through the first helper.
  - `plan/2026-11-20-5-model-the-broker-is-asked-for.plan.md` is this plan.
  - No public API changes, so `crates/runvil-engine/public-api.txt` and `crates/runvil/public-api.txt` stay as they are.
- Data, persistence, failure, security, and privacy constraints: None

</implementation-contract>
<verification-contract>

## Testing Plan

The change is itself tests, so its own check is that each break fails a test. The repository's full verification list runs once at the end.

- Unit: None
- Integration and end-to-end:
  - `cargo nextest run --locked -p runvil -E 'test(broker)'` runs the tests of the file: one existing and two new.
- Regression, security, and performance:
  - Apply each of the three breaks in turn, run the whole workspace, see the named tests fail and every other test pass, and revert the break.
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
  - The tests observe the model the broker is asked for, through a whole run. The test this plan serves, in the QA plan of attack, reads: "Select no model in that Host and send `hello.md`. The broker is asked for the first model." A reviewer of that plan asked how such a result is captured, since no screen shows it. The broker is where a Host sees it.
  - The tests send the shipped `hello.md`, read from `prompts/`, because the QA plan's test names that prompt.
  - The selection test selects `second` and then `first`. Selecting only `first` would pass under Break B, and selecting only `second` would pass if the last listed model were always bound.
  - Breaks A and B were shown before these tests were written, by applying each and running the whole workspace: all 3,984 tests passed under each. Break C is shown by the run that proves these tests: with it applied, every test but these two passes. The user's rule, given on 2026-11-18: "If existing tests already cover these behaviors, we shouldn't be raising a PR that introduces duplicate coverage."
- Rejected alternatives:
  - Unit tests of `current_model` alone. They were written first, and they pin the choice of model with less code. They read the result inside the Harness, not where a Host sees it, and they miss Break C. They were replaced by these tests, not kept beside them, since the two would check the same rules.
- Assumptions, risks, and notes:
  - The Workshop repository has a test that selects a second model and checks the next request names it. That test should notice Break B, but only after the Harness change reached Workshop through its pinned commit. It was not run against the break.
  - The tests fail if `prompts/hello.md` changes its prose or stops making exactly one model round. The person who changes that sample updates these tests in the same change.
  - Result of the breaks: the workspace runs 3,986 tests. Break A fails the no-selection test, Break B fails the selection test, and Break C fails both. Every other test passes under each.
  - Result of the exit criteria: every command passed except the custom lints, which were not run locally because `cargo-dylint` is not installed on the author's machine. CI's `dylint` job runs them on the pull request.
  - `vibelight` outlines each crate's library code and not its `tests/` folder, so it was also run on `broker.rs` alone. Its first run flagged a message repeated in three tests. The two new tests now return their error in place of that message, and the second run reported no lead.

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
  - Commit this plan alone on the branch `qa-plan-tests`, after the commits of the plan before it, `plan/2026-11-20-4-run-every-shipped-prompt.plan.md`. One branch holds every step of this effort.
  - Change the scripted broker and add the two helpers and the two tests in `crates/runvil/tests/suite/broker.rs`.
  - Apply and revert each break, recording which tests fail.
  - Run the exit criteria.
  - Commit the tests. Push the branch to the fork and open the PR only when the user asks.
- Dependencies and verification:
  - The tests depend on nothing outside the repository.
  - The breaks are never committed. `git status` shows no change to `environment.rs` or `effect_loop.rs` before each commit.

</execution-plan>
