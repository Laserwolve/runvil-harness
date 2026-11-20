---
name: Run every shipped prompt in a test
overview: The repository ships five sample prompts in `prompts/`. A test runs one of them. No test reads the other four, so a change that breaks one of them passes every test. This plan adds five tests that run those four prompts on a scripted model. It adds no test file and changes no production code and no prompt.
todos:
  - id: plan-copy
    content: "Copy this plan to plan/2026-11-20-4-run-every-shipped-prompt.plan.md on a new branch and commit it alone"
    status: completed
  - id: tests
    content: "Add the three helpers and the five tests to crates/runvil-engine/tests/suite/shipped.rs, and read the research prompt through the new helper"
    status: completed
  - id: breaks
    content: "Apply each of the five breaks, confirm a test now fails, and revert it"
    status: completed
  - id: verify
    content: "Run every verification command and vibelight"
    status: completed
  - id: pr
    content: "Push the branch to the fork and open one PR against cppalliance/runvil-harness"
    status: pending
---

# Run every shipped prompt in a test

<product-contract>

## Product Requirements

The repository ships five sample prompts in `prompts/`. A test runs one of them, `research-person.md`. No test reads `echo.md`, `greet.md`, `hello.md`, or `analyst-example.md`, so each of them can be broken and all 3,979 existing tests still pass. The four prompts are correct today, so this is missing protection, not a bug. This plan adds the missing tests.

- Problem and users:
  - People read and copy the samples to learn the language. A sample that no longer runs teaches them something false.
  - The samples are edited by hand when the language changes. Two commits of 2026-11-13 edited them that way: `3d87c936b` (a prompt's arguments became one JSON object) and `7147e709f` (every heading became a section). Nothing checked the result.
  - The users are the engineers who change the language, and who rely on the tests to notice a sample they broke.
- Goals:
  - Each of the five breaks listed in the Functional Specification makes at least one test fail.
  - Every prompt in `prompts/` is run by a test in one file, `crates/runvil-engine/tests/suite/shipped.rs`.
- Non-goals:
  - Any change to production code.
  - Any change to a prompt in `prompts/`.
  - A run through the Harness, or against a real model.
- Success criteria:
  - With each break applied in turn, `cargo nextest run -p runvil-engine` fails, and it passes again once the break is reverted.
  - The workspace has exactly five more tests than before: 3,984 run, up from 3,979.
  - Every verification command in the Testing Plan passes.
- Constraints:
  - The two existing tests in `shipped.rs` keep every assertion they make today.
  - The new tests use the test support the file already uses. They add no dependency.
- Open questions: None

## Functional Specification

Five tests run four prompts. Each test reads the prompt's file from `prompts/`, so the test runs the text a reader of the repository sees. Nothing a user of Runvil sees changes.

- Actors and workflows:
  - A developer or CI runs the test suite. No other actor is involved.
- Inputs and outputs:
  - `echo.md`, with the arguments `{"text":"ping"}`: the run returns `ping`, and it makes no model round.
  - `greet.md`, with the arguments `{"name":"Ada"}` and one scripted reply: the run makes one round under the `default` role. The round's only message is `Repeat exactly, with no extra words: Hello, Ada!`. The run returns the scripted reply.
  - `hello.md`, with one scripted reply: the run makes one round under the `default` role. The round's only message is `Say "Hello, world!"`. The run returns the scripted reply.
  - `analyst-example.md`, with the arguments `{"text":"the input"}`, one scripted reply, an 8,192-token model for `default`, and a 40,000-token model for `strong`: the run makes one round under the `strong` role. The round's only message is the prompt's instruction, a blank line, and `the input`. The run returns the scripted reply.
  - `analyst-example.md`, with a 39,999-token model filling every role: the run is refused on its first step with `requirements_unmet`. It reports no event and issues no effect. The message names the role, the 40,000 tokens it requires, and the 39,999 the model provides.
- States and validation: None
- Errors and recovery:
  - A prompt file that is missing fails its test with a message that names the path.
- Security and privacy behavior: None
- Acceptance criteria:
  - Break 1: in `echo.md`, change `return args.text` to `return args.txt`. The echo test fails.
  - Break 2: in `greet.md`, change `"Hello, "` to `"Hi, "`. The greet test fails.
  - Break 3: in `hello.md`, change `return models.infer(prose)` to `models.infer(prose)`. The hello test fails.
  - Break 4: in `analyst-example.md`, change the Lua line `models.use("strong")` to `models.use("fast")`. The analyst run test fails.
  - Break 5: in `analyst-example.md`, change `min_context: 40000` to `min_context: 4000`. The analyst refusal test fails.
  - With all five breaks applied, the five new tests are the only tests in the workspace that fail.

</product-contract>
<implementation-contract>

## Technical Design

The change is test code in one existing file. No module, interface, or stored format changes.

- Architecture:
  - The tests run on the Engine with scripted model replies, as the two existing tests of `research-person.md` do.
- Modules and interfaces: None
- File and public API changes:
  - `crates/runvil-engine/tests/suite/shipped.rs` gains three helpers and five tests. One helper reads a shipped prompt by its file name. One lists a run's model rounds as the role and the messages of each. One checks that a run made exactly one round and returned a given reply. The existing helper that runs `research-person.md` reads its file through the first.
  - The refusal test calls `refusal` in `crates/runvil-engine/tests/suite/prepare.rs`, which already checks the error code, the absence of events, and the absence of effects.
  - `plan/2026-11-20-4-run-every-shipped-prompt.plan.md` is this plan.
  - No public API changes, so `crates/runvil-engine/public-api.txt` and `crates/runvil/public-api.txt` stay as they are.
- Data, persistence, failure, security, and privacy constraints: None

</implementation-contract>
<verification-contract>

## Testing Plan

The change is itself tests, so its own check is that each break fails a test. The repository's full verification list runs once at the end.

- Unit: None
- Integration and end-to-end:
  - `cargo nextest run --locked -p runvil-engine -E 'test(shipped)'` runs the seven tests of the file: two existing and five new.
- Regression, security, and performance:
  - Apply each of the five breaks in turn, run the seven tests, see the named test fail, and revert the break.
  - Apply all five breaks at once and run the whole workspace. The five new tests fail and every other test passes.
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
  - The tests run on the Engine with a scripted model, not through the Harness. The existing tests of the fifth shipped prompt are written that way, in the same file, and the Engine's test support already records each model round. The Harness's test brokers do not record what a round sent.
  - The tests read the prompt files from `prompts/`. A copy of a prompt's text inside a test would pass while the shipped file broke.
  - Each message is compared whole, not searched for a phrase. A sample is short, and any edit to its prose is a deliberate change that should update its test.
  - The refusal test uses a 39,999-token model, one token short. With the passing run on a 40,000-token model, the pair pins the sample's `min_context` to exactly 40,000.
  - The gap was found by search before any test was written. Under `crates/`, `tools/`, and `.github/`, the only code that reads a file from `prompts/` is the helper in `shipped.rs` that reads `research-person.md`. Tests elsewhere use the name `greet.md` for a prompt written inside the test, which is a different text. The user's rule, given on 2026-11-18: "If existing tests already cover these behaviors, we shouldn't be raising a PR that introduces duplicate coverage."
- Rejected alternatives:
  - A test that lists the `prompts/` folder and fails on a file with no test. It reminds, but it cannot tell whether a listed prompt is run. Revisit if a sixth prompt ships without a test.
  - Running the samples against a real Gateway and model. No repository has a rig for that yet. Revisit when one exists.
- Assumptions, risks, and notes:
  - Existing tests already check each behavior the samples use, with prompts written inside the tests: `a_prompt_with_no_args_key_runs_on_the_empty_object_and_on_one_string_named_text`, `finite_pipeline_runs_the_unified_surface_end_to_end`, `a_switch_in_one_section_is_still_in_effect_in_the_next`, and `a_context_minimum_above_the_filling_models_is_refused`. What is new here is that the shipped files are run.
  - The tests fail whenever a sample's text changes. That is intended: the person who changes a sample updates its test in the same change.
  - A model round's record names the role it ran under as `alias` (`crates/runvil-internal/runtime/src/execute/run/effect.rs`). The tests read the role from there.
  - Result of the breaks: each of the five fails exactly the test the Functional Specification names. With all five applied, the workspace runs 3,984 tests, the five new ones fail, and the other 3,979 pass.
  - Result of the exit criteria: every command passed except the custom lints, which were not run locally because `cargo-dylint` is not installed on the author's machine. CI's `dylint` job runs them on the pull request.
  - `vibelight` outlines each crate's library code and not its `tests/` folder, so it was also run on `shipped.rs` alone. Its first run flagged a role passed as a string and two near-identical test bodies. The role is now a `ModelRole`, and the three one-round tests share one checking helper. Two leads remain: the bodies of the greet and hello tests are still alike, because the two samples do the same thing, and the literal `web/search` repeats in the three functions that were already there.

### Deferred and Out of Scope

- Deferred: a run of the shipped prompts through the Harness and the Gateway client. Revisit with the test that joins those two against a stub Gateway.
- Out of scope: every other test of the QA plan of attack.

</decision-record>
<project-survey>

## Project Survey

None

</project-survey>
<execution-plan>

## Execution Instructions

- Work items:
  - Commit this plan alone on a new branch made from `upstream/master`.
  - Add the three helpers and the five tests to `crates/runvil-engine/tests/suite/shipped.rs`.
  - Apply and revert each break, recording which test fails.
  - Run the exit criteria.
  - Commit the tests. Push the branch to the fork and open one PR only when the user asks.
- Dependencies and verification:
  - The tests depend on nothing outside the repository.
  - The breaks are never committed. `git status` shows no change under `prompts/` before each commit.

</execution-plan>
