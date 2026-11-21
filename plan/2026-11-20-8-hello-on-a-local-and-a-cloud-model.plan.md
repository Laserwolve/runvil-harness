---
name: Run the shipped hello prompt on a local model and on a cloud model
overview: A Gateway answers a request from a model it runs on its own machine or from a cloud provider. The live tests so far run on a local model only, and the Gateway's own tests of its cloud route use stand-in servers. No test runs a prompt on a real cloud provider's model. This plan adds one test that runs the shipped hello prompt on a local model and then on a cloud model of the same Gateway. It is ignored unless asked for. It changes no production code.
todos:
  - id: plan-copy
    content: "Copy this plan to plan/2026-11-20-8-hello-on-a-local-and-a-cloud-model.plan.md on the branch qa-plan-tests and commit it alone"
    status: completed
  - id: test
    content: "Add the test to crates/harness-gateway-client/tests/live.rs, and have both tests of that file share one run of hello.md"
    status: completed
  - id: runs
    content: "Run the test against a Gateway that serves a local chat model and a cloud chat model, and against one that serves no cloud model"
    status: completed
  - id: verify
    content: "Run the verification commands and vibelight"
    status: pending
  - id: pr
    content: "Push the branch qa-plan-tests to the fork and open one PR for the whole effort against cppalliance/runvil-harness"
    status: pending
---

# Run the shipped hello prompt on a local model and on a cloud model

<product-contract>

## Product Requirements

A Gateway serves two kinds of chat model. A `[[local_model]]` entry of its settings is a model file that the Gateway runs on its own machine. A `[[model]]` entry is a model at an `[[endpoint]]`, which for a cloud provider is an address on the internet and a key. A request for either kind arrives the same way, and the Gateway answers it by a different route.

`a_host_sends_the_shipped_hello_prompt_through_a_real_gateway` runs a prompt through a real Gateway, on whichever model the Gateway lists first. The runs recorded for it used a local model. The Gateway's own tests of the cloud route send to stand-in servers. So nothing runs a prompt on a real cloud provider's model.

- Problem and users:
  - No test runs a prompt through the Harness and a real Gateway on a cloud provider's model.
  - The users are the people who point a Gateway at a cloud provider, and the engineers who change the Harness, the Gateway client, or the Gateway's cloud route.
- Goals:
  - One test runs the shipped `prompts/hello.md` on a local model and then on a cloud model of the same Gateway, and checks that each run ends with text.
  - The test itself tells the local model from the cloud model, so it cannot pass on two local models.
  - A normal test run skips it.
- Non-goals:
  - Any change to production code.
  - Starting a Gateway, fetching a model, or holding a provider's key. The person who asks for the test has a Gateway running with both kinds of model.
  - A run in CI.
  - A check of what either model says. A real model's words vary.
- Success criteria:
  - With a Gateway that serves a local chat model and a cloud chat model, the test passes.
  - With a Gateway that serves no cloud model, the test fails before any run and says which kind is missing.
  - `cargo nextest run --locked -p harness-gateway-client --all-features` passes and counts the test as skipped.
- Constraints:
  - The test adds no dependency to the crate.
- Open questions: None

## Functional Specification

The test reads the Gateway's settings to find the two models, then makes two runs.

- Actors and workflows:
  - A person starts a Gateway that serves a local chat model and a cloud chat model, sets two variables, and runs `cargo nextest run -p harness-gateway-client --test live -E 'test(local_model_and_on_a_cloud_model)' --run-ignored ignored-only`.
- Inputs and outputs:
  - `RUNVIL_E2E_GATEWAY` is the Gateway's API root, such as `http://127.0.0.1:8189/v1`. `RUNVIL_E2E_KEY` is the Gateway's key.
  - The test asks the Gateway for its settings at `/admin/config`, with the key.
  - The local model is the first `[[local_model]]` entry of kind `chat`.
  - The cloud model is the first `[[model]]` entry of kind `chat` whose first endpoint has a `base_url` that is not a loopback address.
  - The test runs `hello.md` with the local model selected, and then with the cloud model selected. Each run ends with text that is not empty.
- States and validation:
  - Settings with no local chat model, or with no cloud chat model, fail the test before any run. The message says which is missing.
- Errors and recovery:
  - A run that ends with an error fails the test. The message says whether it was the local or the cloud run, names the model, and prints the error.
- Security and privacy behavior:
  - The provider's key belongs to the Gateway and never reaches the test. The test sends only the Gateway's key.
  - The cloud run sends the text of `hello.md` to the provider, and the provider charges for it.
- Acceptance criteria:
  - The two runs listed under Success criteria give the results stated there.

</product-contract>
<implementation-contract>

## Technical Design

The change is in one test file. No module, interface, or stored format changes.

- Architecture:
  - The test sits beside the first live test in `crates/harness-gateway-client/tests/live.rs` and builds its Host the same way.
- Modules and interfaces: None
- File and public API changes:
  - `crates/harness-gateway-client/tests/live.rs`: a `Gateway` type holds the API root and the key the environment names. Its `hello` method is the run of `hello.md` that the first test made in its own body, now shared by both tests. Its `a_local_and_a_cloud_model` method reads the settings and returns the two names in a `LocalAndCloud` value. The new test is marked `#[ignore]` with the reason.
  - `plan/2026-11-20-8-hello-on-a-local-and-a-cloud-model.plan.md` is this plan.
  - No public API changes, so `crates/runvil-engine/public-api.txt` and `crates/runvil/public-api.txt` stay as they are.
- Data, persistence, failure, security, and privacy constraints: None

</implementation-contract>
<verification-contract>

## Testing Plan

The change is itself a test. It is checked by running it against real Gateways.

- Unit: None
- Integration and end-to-end:
  - Run the test with a Gateway that serves a small local chat model and one model of a cloud provider.
- Regression, security, and performance:
  - Run the test with a Gateway that serves no cloud model, and see it fail with the message that names the missing kind.
  - Run the first live test again, and see it pass as before.
  - Run the crate's tests without asking for ignored tests, and see both live tests skipped.
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
  - The test finds the two models in the Gateway's settings. With two variables that name them, a person could name two local models and the test would pass without a request ever leaving the machine. The cost is that the test knows eight names of the settings: `local_model`, `model`, `endpoint`, `kind`, `name`, `endpoints`, `id`, and `base_url`. A rename there fails the test with the message that says what it did not find.
  - A cloud model is one whose endpoint is not at a loopback address. The test asks `GatewayEndpoint::is_loopback`, the rule this crate already applies to the Gateway's own address. By this rule a model served from another machine on the same network counts as a cloud model.
  - One test makes both runs, the local one first, as the QA plan words it: "Run one prompt on a local model, then on a cloud provider. Both runs end with text." When the cloud run fails after the local run passed, the fault is on the cloud route.
  - The test asserts only that each text is not empty.
- Rejected alternatives:
  - Two variables that name the models. See the first decision.
  - A test in the Gateway's repository. Its tests send requests to stand-in servers and run no prompt.
  - A test through Workshop. It adds Workshop's server and its agents to the path, and the choice of a model in Workshop has a test of its own there.
- Assumptions, risks, and notes:
  - The plan before this one, `plan/2026-11-20-7-a-host-through-a-real-gateway.plan.md`, turned down a test that starts its own Gateway, because it would tie this repository to the Gateway's settings format. This test reads eight names of that format and writes none.
  - This serves the test "Run one prompt on a local model, then on a cloud provider. Both runs end with text." of the QA plan of attack.
  - The runs recorded here used a Gateway built from `cppalliance/runvil-gateway` at commit `52fd92d`. It served Qwen3 0.6B from a local file and Claude Haiku 5.5 at Anthropic's OpenAI-compatible address, `https://api.anthropic.com/v1`. The cloud entries of its settings are the ones the Gateway's settings page writes from its model sheet.
  - Result of the first runs, made with a provider key that was not scoped to one workspace: with both models served, the test found both, the local run ended with text, and the cloud run failed. The provider refused the request with status 400, because of the key and not because of the request: "This API key is not scoped to a workspace, so this request must include the anthropic-workspace-id header with the ID of the workspace to use." With no cloud model served, the test failed before any run with "the Gateway's settings name no chat model whose endpoint is off its machine", and the first live test passed as before.
  - Found in the first runs: a Gateway cannot use a provider key that is not scoped to one workspace. An `[[endpoint]]` entry has an id, a protocol, an address, a key, and a dominion, and no setting for a request header. The test needs a key that is scoped to one workspace.
  - A trial the same day, which does not count as a run of this test: a relay on the Gateway's machine added the header and passed each request on to the provider. Through it, the first live test passed three times of three with `RUNVIL_E2E_MODEL=claude-haiku-5-5`, in about 0.65 seconds a run. So the header was the only thing between this test and a pass. The trial does not count, because the Gateway's endpoint was then an address on its own machine, which this test rightly does not take for a cloud model.
  - Result of the runs with a provider key scoped to one workspace: the test passed three times in a row, in 0.65 to 0.82 seconds a run. The Gateway sent straight to the provider, and its log shows no refused request.
  - Found in the first runs: the Gateway's caller is told only "upstream returned 400". The provider's reason is in the Gateway's log and nowhere in the run's error.
  - Found in the first runs: the Gateway lists the cloud model before the local one. The first live test selects no model, so it runs on the cloud model whenever the Gateway serves one.
  - Result of the exit criteria, in part. Passed: `cargo fmt --all --check`, `cargo clippy -p harness-gateway-client --all-targets --all-features`, the crate's tests with 157 passed and the 2 live tests skipped, `cargo test -p build-xtask`, and the wording check. `vibelight`, run on `live.rs` alone, reported no lead. Not run locally: the workspace-wide test and clippy runs, the docs builds, the facade check, and the custom lints.

### Deferred and Out of Scope

- Out of scope: tool calls, streaming, and reasoning on a cloud model. This test makes one round with no tools.

</decision-record>
<project-survey>

## Project Survey

None

</project-survey>
<execution-plan>

## Execution Instructions

- Work items:
  - Commit this plan alone on the branch `qa-plan-tests`, after the commits of the plan before it, `plan/2026-11-20-7-a-host-through-a-real-gateway.plan.md`.
  - Change `crates/harness-gateway-client/tests/live.rs`.
  - Make the runs and record their results.
  - Run the exit criteria.
  - Commit the test. Push the branch to the fork and open the PR only when the user asks.
- Dependencies and verification:
  - The runs need a Gateway that serves a local chat model and a cloud chat model, which needs a key the provider accepts from the Gateway.

</execution-plan>
