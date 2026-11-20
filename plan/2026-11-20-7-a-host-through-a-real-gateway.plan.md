---
name: Run the shipped hello prompt through a real Gateway
overview: Every test of the Gateway client talks to a stand-in server, and every test that runs a prompt uses a broker written for the test. No test runs a prompt through the Harness and this crate's broker against a real Gateway and a real model. This plan adds one such test. It is ignored unless asked for, because it needs a Gateway that is running. It changes no production code.
todos:
  - id: plan-copy
    content: "Copy this plan to plan/2026-11-20-7-a-host-through-a-real-gateway.plan.md on the branch qa-plan-tests, which holds every step of this effort, and commit it alone"
    status: completed
  - id: test
    content: "Add crates/harness-gateway-client/tests/live.rs with the one ignored test"
    status: pending
  - id: runs
    content: "Run the test against a real Gateway with a model, with and without a named model, and against a stopped Gateway"
    status: pending
  - id: verify
    content: "Run the verification commands and vibelight"
    status: pending
  - id: pr
    content: "Push the branch qa-plan-tests to the fork and open one PR for the whole effort against cppalliance/runvil-harness"
    status: pending
---

# Run the shipped hello prompt through a real Gateway

<product-contract>

## Product Requirements

Workshop is one Host. Any program that runs a prompt through the Harness is another, and such a Host reaches its models through `GatewayBroker` in `crates/harness-gateway-client`. The pieces are tested apart: the broker against stand-in servers, and the Harness against brokers written for its tests. Nothing runs the whole path against the real product, so a fault that only the real Gateway shows goes unseen. One such fact was found while writing this test and is recorded below.

- Problem and users:
  - No test sends a prompt from a Host through the Harness and `GatewayBroker` to a real Gateway that serves a real model.
  - The users are the people who write a Host other than Workshop, and the engineers who change the Harness or the Gateway client.
- Goals:
  - One test runs the shipped `prompts/hello.md` that way and checks that the run ends with text.
  - A normal test run skips it. A person asks for it by name and names the Gateway in the environment.
- Non-goals:
  - Any change to production code.
  - Starting or stopping a Gateway, or fetching a model. The person who asks for the test has a Gateway running.
  - A run in CI. No workflow has a Gateway with a chat model.
  - A check of what the model says. A real model's words vary.
- Success criteria:
  - With a Gateway running that serves a chat model, the test passes with no model named and with that model named.
  - With no Gateway at the named address, the test fails and the failure names the address.
  - `cargo nextest run --locked --workspace --all-features` passes and counts the test as skipped.
- Constraints:
  - The test adds no dependency to the crate.
- Open questions: None

## Functional Specification

The test builds a Host as a small command-line program would, and reads its settings from three environment variables.

- Actors and workflows:
  - A person starts a Gateway that serves a chat model, sets the variables, and runs `cargo nextest run -p harness-gateway-client --test live --run-ignored ignored-only`.
- Inputs and outputs:
  - `RUNVIL_E2E_GATEWAY` is the Gateway's API root, such as `http://127.0.0.1:8189/v1`.
  - `RUNVIL_E2E_KEY` is the Gateway's key.
  - `RUNVIL_E2E_MODEL`, when set, is the model the Host selects. Without it the run takes the first model the Gateway lists.
  - The Host holds no Pack but the built-ins and finds one prompt, the text of `prompts/hello.md`.
  - The run ends with text that is not empty.
- States and validation:
  - A missing `RUNVIL_E2E_GATEWAY` or `RUNVIL_E2E_KEY` fails the test with a message that names the variable.
- Errors and recovery:
  - A run that ends with an error fails the test and prints the error whole.
- Security and privacy behavior:
  - The key comes from the environment and is held in the client's `SecretString`, which is redacted wherever it is formatted.
- Acceptance criteria:
  - The three runs listed under Success criteria give the results stated there.

</product-contract>
<implementation-contract>

## Technical Design

The change is one new test file. No module, interface, or stored format changes.

- Architecture:
  - The test is an integration test of `harness-gateway-client`, the crate that holds the broker. That crate already depends on `runvil`, the one crate a Host depends on, so the test builds its Host from public items only.
- Modules and interfaces: None
- File and public API changes:
  - `crates/harness-gateway-client/tests/live.rs` is new. It holds one test marked `#[ignore]` with the reason.
  - `plan/2026-11-20-7-a-host-through-a-real-gateway.plan.md` is this plan.
  - No public API changes, so `crates/runvil-engine/public-api.txt` and `crates/runvil/public-api.txt` stay as they are.
- Data, persistence, failure, security, and privacy constraints: None

</implementation-contract>
<verification-contract>

## Testing Plan

The change is itself a test. It is checked by running it against a real Gateway, and against none.

- Unit: None
- Integration and end-to-end:
  - Run the test with a Gateway that serves a small local chat model, once with no model named and once with the model named.
- Regression, security, and performance:
  - Run the test with the Gateway stopped, and see it fail with the address in the message.
  - Run the crate's tests without asking for ignored tests, and see the new test skipped.
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
  - The test is ignored by default and reads the Gateway's address from the environment. The user's direction, given on 2026-11-20: "All these tests should be runable locally. We aren't dealing with running tests in CI yet."
  - The test sends the shipped `hello.md`, because the QA plan's test names it: "Send `hello.md` from a command-line Host through a real Gateway. The run ends with text."
  - The key is a required setting with no default. The broker always sends a key, and a real Gateway answers a wrong key with status 401 even from a loopback caller it would trust with no key at all. A default key would fail that way, which was seen on the first run of this test.
  - The test asserts only that the text is not empty. What a real model writes for "Say Hello, world!" is not fixed.
- Rejected alternatives:
  - A new command-line program in `examples/` in place of a test. A test can be asked for by name and reports pass or fail, and `crates/runvil/examples/run-prompt.rs` already shows a Host's steps.
  - A test that starts its own Gateway and fetches a model. It would tie this repository to the Gateway's settings format and to one model. Revisit when several tests need the same Gateway and a shared way to start it.
- Assumptions, risks, and notes:
  - The runs recorded here used a Gateway built from `cppalliance/runvil-gateway` at commit `52fd92d`, serving Qwen3 0.6B from a local file.
  - `cargo nextest run -p harness-gateway-client --test live` with no other test selected ends with "no tests to run", since the only test of that file is ignored. A run of the crate or the workspace reports it as skipped.

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
  - Commit this plan alone on the branch `qa-plan-tests`, after the commits of the plan before it, `plan/2026-11-20-6-round-options-and-tools-reach-the-broker.plan.md`.
  - Add `crates/harness-gateway-client/tests/live.rs`.
  - Make the three runs and record their results.
  - Run the exit criteria.
  - Commit the test. Push the branch to the fork and open the PR only when the user asks.
- Dependencies and verification:
  - The runs need a Gateway that serves a chat model. Nothing else outside the repository is needed.

</execution-plan>
