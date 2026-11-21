---
name: Stock Tokio Timer
overview: Add `harness_gateway_client::TokioTimer`, replace every hand-written tokio timer in runvil-harness (three) and runvil-workshop (one type, four call sites), and clean up each call site that the swap touches. The plan is copied into the repo's plan folder, each testable step is its own commit, both repos get a pull request from the fork, and the merges go Harness then Workshop once every check on both is green. The classroom stays as it is.
todos:
  - id: plan-copy
    content: "Check that runvil-harness and runvil-workshop are clean and equal to origin and upstream, then Harness commit 0: copy the approved plan alone to runvil-harness/plan/2026-11-20-3-stock-tokio-timer.plan.md and commit it on master"
    status: completed
  - id: harness-add
    content: "Harness commit 1: add `TokioTimer` to harness-gateway-client (timer.rs, the wiring, the doc comments, `test-util` for the tests), its three tests on a paused clock, the three edits of harness-and-host.md, the clause in testing-and-debugging.md, and the two READMEs; run verify.ps1 -Tag t1a (3,979 tests expected, both listings unchanged); commit"
    status: completed
  - id: harness-use-plugin
    content: "Harness commit 2: add the Gateway client as a workspace dependency and as a dev-dependency of harness-test-plugin, switch the copy in test-plugin run.rs and clean up its call site (the stock finder in host_offering), refresh Cargo.lock (exactly one added line), run verify.ps1 -Tag t1b, commit"
    status: completed
  - id: harness-use-facade
    content: "Harness commit 3: add the Gateway client as a dev-dependency of runvil, switch the two copies (run-prompt.rs and the facade suite's support.rs) and clean up the suite's call site (the stock finder in launch), refresh Cargo.lock (exactly one added line), run verify.ps1 -Tag t1c and the example, commit"
    status: completed
  - id: workshop-dry-run
    content: "Workshop dry run under a temporary [patch] on the local Harness: remove `TokioTimer` from workshop-agents (file to cabinet/_trash), switch the four call sites and clean them up (the stock finder and the chained setup in run_for, the Host built inside its Arc in AgentSessions::new), edit the agents manifest, run clippy and nextest for workshop-agents and workshop-server, then restore the root Cargo.toml and Cargo.lock"
    status: completed
  - id: record
    content: "Harness commit 4: add 'What the build found' for both repos and the todo statuses to the repo copy of the plan, and commit it alone"
    status: completed
  - id: harness-pr
    content: Push Harness master to origin and open the pull request from vinniefalco:master
    status: completed
  - id: workshop-commit
    content: "Workshop commit: pin the three rev values to the Harness head, cargo fetch, run verify-workshop.ps1 -Tag t1w (706 and 353 tests expected), commit, push to origin, open the pull request"
    status: completed
  - id: merge
    content: "When every check on both pull requests is green (CI on both, and the Bundle workflow on Workshop) and neither upstream master moved: git push upstream master in the Harness and confirm, then the same in Workshop, then confirm that each fork equals its upstream"
    status: pending
isProject: false
---

# Put a stock `TokioTimer` in `harness-gateway-client` and use it in every repo

One new public type, `harness_gateway_client::TokioTimer`, replaces four hand-written tokio timers: three in `runvil-harness` and one in `runvil-workshop`. A Host still names its timer when it builds its `Host`, so a Host that leaves the timer out still fails to compile. The classroom stays as it is.

## How the work is delivered

1. The approved plan is copied into `runvil-harness/plan/`, as a commit of its own.
2. Each testable step is one commit on `master`: three in the Harness and one in Workshop. Each passes its repo's whole verification list before it is committed. The plan commit and the record commit hold text only.
3. Each repo is pushed to the fork (`origin`), and a pull request is opened from `vinniefalco:master`.
4. When every check on both pull requests is green, the merges go upstream in order: the Harness first, then Workshop, because Workshop pins a Harness commit that upstream `master` has to hold.

## Cleanup at each call site

A call site here is a function in which this plan changes the timer of a `Host`. There are seven: three in the Harness and four in Workshop. At each one, the same commit that swaps the timer also leaves the `Host`, and a `RunSetup` built in that function, in the shortest form that today's API allows. Every test has to see the same thing before and after.

- **What the old timer leaves behind goes.** That is the struct, its impl, the imports that only it used, and the doc comments that name it.
- **A hand-written finder for one prompt becomes `FindTextPrompt`.** Three sites hold an eleven-line closure that returns one source for one name and fails every other name as `NotFound`: `launch` in the facade suite, `host_offering` in the test Plugin's tests, and `run_for` in the tests of `workshop-agents`. The stock finder does that in one line. Each of these Hosts is asked for its one name only, so the failing branch runs in no test.
- **A `RunSetup` in struct update syntax becomes the chain.** One site: `run_for` writes `RunSetup { has_user: true, ..RunSetup::new(..) }`, which becomes `RunSetup::new(..).has_user(true)`.
- **A `Host` is built inside its `Arc`.** One site: `AgentSessions::new` builds a bare `Host` and wraps it further down, where it fills `Inner`.

Three of the seven need the swap alone:

- `main` of the example already uses the stock finder, the stock broker, and both chains.
- `run_prompt_over` in Workshop's server tests has a one-line finder that gives its source under every name, as its doc says.
- The test `an_agent_executes_an_agent_of_the_folder_that_its_prompts_list_names` builds its `Host` over `agent_finder`, the code under test.

## Where this starts

- `runvil-harness` `master` is `b5e2c935`, and `master` of `cppalliance/runvil-harness` is the same commit (read from the repo's log and from GitHub at 17:50 PST). The workspace ran 3,976 tests at `3b1ee6a5`, all passing, 25 skipped, and the two commits after it changed text only.
- `runvil-workshop` `master` is `9fb795ef`, the same as `cppalliance/runvil-workshop`. It pins the Harness at `c581ba33`. Its two test groups ran 706 and 353 tests.
- The shell is switched off in plan mode, so whether both trees are clean is still to be checked. That check is the first step.
- A search of every checkout in the workspace found a timer that only wraps `tokio::time::sleep` in three repos:
  - Harness, three copies named `Clock`: [runvil-harness/crates/runvil/examples/run-prompt.rs](runvil-harness/crates/runvil/examples/run-prompt.rs), [runvil-harness/crates/runvil/tests/suite/support.rs](runvil-harness/crates/runvil/tests/suite/support.rs), and [runvil-harness/crates/harness-internal/test-plugin/tests/it/run.rs](runvil-harness/crates/harness-internal/test-plugin/tests/it/run.rs).
  - Workshop, one type: `TokioTimer` in [runvil-workshop/crates/workshop/agents/src/timer.rs](runvil-workshop/crates/workshop/agents/src/timer.rs), used at four call sites in three files.
  - The classroom, four student programs, left alone (see Not in this plan).
- The Harness runner's tests hold eight other `Timer` types. Each is a test double (held, pending, pinned, instant, unused, or one that raises a flag when dropped), so they stay.
- The guide and the tutorials show the hand-written timer nowhere.

## The order of work

```mermaid
flowchart LR
    Add[Add the timer] --> TestPlugin[Test Plugin site]
    TestPlugin --> Facade[Facade sites]
    Facade --> DryRun[Workshop dry run]
    DryRun --> Record[Record commit]
    Record --> HarnessPR[Harness PR]
    HarnessPR --> WorkshopPR[Workshop PR]
    WorkshopPR --> Green[All checks green]
    Green --> MergeH[Merge Harness]
    MergeH --> MergeW[Merge Workshop]
```

- Harness commit 0: this plan alone, copied to `runvil-harness/plan/2026-11-20-3-stock-tokio-timer.plan.md`.
- Harness commit 1: add `TokioTimer`, its tests, its page entry.
- Harness commit 2: use it in the tests of the test Plugin.
- Harness commit 3: use it in the facade's example and suite.
- Workshop dry run: the switch, built against the local Harness through a temporary `[patch]`.
- Harness commit 4: the record of what the build found in both repos.
- Harness: push to the fork, open the pull request.
- Workshop commit: pin the Harness head and use the stock timer. Push to the fork, open the pull request.
- Every check on both pull requests green: merge the Harness, then Workshop.

Every commit goes on `master`. Each code commit passes its repo's whole verification list by itself.

## Harness commit 1: add `TokioTimer`

### The new type

New file [runvil-harness/crates/harness-gateway-client/src/timer.rs](runvil-harness/crates/harness-gateway-client/src/timer.rs). The body is Workshop's copy:

```rust
//! The stock tokio timer: each sleep is one sleep on tokio's timer.

/// The timer of a Host that runs on tokio.
///
/// A Host passes it as the timer of its `Host`. Each sleep is one
/// `tokio::time::sleep`. When a stop or a cancel ends a wait, the Harness
/// drops the future, and the drop removes the sleep from tokio's timer.
/// `now` is the trait's default, which reads the system clock.
///
/// A sleep needs a tokio runtime with its timer, from the call of `sleep`
/// to the end of the future. A Host that uses this timer must therefore
/// await `Harness::run` inside such a runtime.
#[derive(Debug)]
pub struct TokioTimer;

impl Timer for TokioTimer {
    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(duration))
    }
}
```

The crate already depends on tokio with the `time` feature, for its per-receive timeout, and `tokio::time::sleep` needs only that. So the crate gains no dependency.

### Wiring

- [runvil-harness/crates/harness-gateway-client/src/lib.rs](runvil-harness/crates/harness-gateway-client/src/lib.rs): add `mod timer;` and `pub use timer::TokioTimer;`, and one paragraph to the crate doc after the paragraph on `provide_gateway_resources`. It says what the type is and why it is in this crate: `runvil` excludes tokio, and this crate depends on it. That doc has `## Invariants`, so the wording check reads it.
- [runvil-harness/crates/harness-gateway-client/Cargo.toml](runvil-harness/crates/harness-gateway-client/Cargo.toml): the comment on `tokio` names the timer's sleeps beside the per-receive timeout. The dev-dependency on `tokio` gains the feature `test-util`, which the new tests need to pause the clock.
- [runvil-harness/crates/harness-gateway-client/README.md](runvil-harness/crates/harness-gateway-client/README.md) and the row of this crate in [runvil-harness/README.md](runvil-harness/README.md): one clause each that names the stock timer.

### Tests

Three tests at the bottom of `timer.rs`, each `#[tokio::test(start_paused = true)]`. A paused clock jumps to the next timer when the runtime is idle, so a one-hour sleep ends at once in real time, and only a sleep on tokio's timer does that.

- `a_sleep_ends_once_tokios_clock_has_advanced_by_its_duration`: awaits a sleep of one hour and asserts that tokio's clock moved at least one hour and less than one second more.
- `a_sleep_is_still_pending_one_second_before_its_duration_ends`: `tokio::time::timeout` of 59 minutes and 59 seconds over a sleep of one hour reports that the time ran out.
- `a_prompts_timed_wait_ends_on_the_stock_timer_of_its_host`: a real run through `Harness::run`, on a `Host` whose timer is `TokioTimer`, with `OfflineBroker` and `MemoryRecorder`. The prompt has two sections:

```lua
-- ## Main
local t = tasks.spawn('../Child')
local first = tasks.join_any({ t }, { timeout = 3600 })
tasks.send(t, 'go')
local _task, ok, result = tasks.join_any({ t })
return tostring(first == nil) .. '|' .. tostring(ok) .. '|' .. result

-- ## Child
return tasks.receive()
```

  It asserts that the result is `true|true|go`, that the record holds one `Timer` effect of 3600 seconds whose answer is `Timer`, and that tokio's clock moved at least one hour.

Expected count: 3,979 tests, which is 3,976 plus these three.

### Pages

In [runvil-harness/decisions/harness-and-host.md](runvil-harness/decisions/harness-and-host.md), three edits.

Section 1, entry "`harness-gateway-client` is the Harness family's second public crate": the first paragraph gains "It also has `TokioTimer`, the stock timer of a Host that runs on tokio (see Inference, the clock, and what a Host hands its Plugins)."

Section 1, entry "The Harness's async code uses `futures-util`, and any executor can poll a run": the last sentence becomes "A Host's own parts may still need a runtime: the docs of `GatewayBroker` and of `TokioTimer` state that their futures need tokio."

Section 4, a new entry right after "The Host's `Timer` performs each sleep and stamps each run's start":

```markdown
### `TokioTimer` is the stock timer of a Host that runs on tokio

`harness-gateway-client` has a stock timer, `TokioTimer`. Its `sleep` returns one `tokio::time::sleep`, so the future needs a tokio runtime with its timer, and its `now` is the trait's default. A Host on tokio passes `Arc::new(TokioTimer)` as the `timer` of its `Host`, so the code that builds the `Host` still names the timer.

The timer is in `harness-gateway-client` because `runvil` excludes tokio, and the Gateway client is the public Harness crate that depends on tokio. Each Host on tokio would otherwise write the same eight lines. A Host on another runtime implements `Timer` over that runtime's sleep.
```

In [runvil-harness/decisions/testing-and-debugging.md](runvil-harness/decisions/testing-and-debugging.md), the entry on paused clocks: "in the Harness runner, in Workshop, and in the Gateway" becomes "in the Harness runner and the Gateway client, in Workshop, and in the Gateway". The crate's tests on real sockets keep real time, as that entry says.

### Listing and lock file

- The API listing check covers the two facades, `runvil-engine` and `runvil`. The new item is in the Gateway client, so both `public-api.txt` files stay as they are, and `xtask api --check` must pass unchanged.
- `Cargo.lock` stays as it is, because a feature of a dependency is outside the lock file.

Then `cargo fmt --all` and `verify.ps1 -Tag t1a`.

## Harness commit 2: use it in the tests of the test Plugin

The test Plugin sits beside the Gateway client in the dependency graph, so this dev-dependency points one way. It is the plain half of the switch.

### Manifests

- [runvil-harness/Cargo.toml](runvil-harness/Cargo.toml): `[workspace.dependencies]` gains `harness-gateway-client = { path = "crates/harness-gateway-client" }`. Today the Gateway client is a member that no other member names.
- [runvil-harness/crates/harness-internal/test-plugin/Cargo.toml](runvil-harness/crates/harness-internal/test-plugin/Cargo.toml): `[dev-dependencies]` gains `harness-gateway-client.workspace = true`, and the comment above the table names it.
- `Cargo.lock`: refreshed with `cargo fetch`. Its diff must be exactly one added line, `"harness-gateway-client",` in the dependency list of `harness-test-plugin`.

### Site

- [runvil-harness/crates/harness-internal/test-plugin/tests/it/run.rs](runvil-harness/crates/harness-internal/test-plugin/tests/it/run.rs): remove `Clock`. `host_offering` uses `Arc::new(TokioTimer)`, and its eleven-line finder closure becomes `Arc::new(FindTextPrompt::new(PROMPT, source))`. `Timer`, `BoxFuture`, `Error`, and `ErrorCode` leave the imports where the compiler agrees, and `Duration` stays for `WAKE_LIMIT`.

### Checks

- The count stays at 3,979, because this commit only switches the test Plugin's existing tests to the stock timer.
- Both listings stay as they are.
- `verify.ps1 -Tag t1b`.

## Harness commit 3: use it in the facade's example and suite

This half has the unusual edge, so it gets a commit of its own. If a check refuses the edge, commits 1 and 2 stand.

### Manifests

- [runvil-harness/crates/runvil/Cargo.toml](runvil-harness/crates/runvil/Cargo.toml): `[dev-dependencies]` gains `harness-gateway-client.workspace = true`, with a comment that the suite's Host and the example's Host sleep on its stock timer.
- `Cargo.lock`: refreshed with `cargo fetch`. Its diff must be exactly one added line, `"harness-gateway-client",` in the dependency list of `runvil`.

The Gateway client depends on `runvil`, so this dev-dependency points back at a crate that depends on `runvil`. Cargo allows that for dev-dependencies. The example and the suite are separate targets that link the same build of `runvil` as the Gateway client, so `TokioTimer` implements the `Timer` they see. The structural checks I read allow the edge: the product matrix in `build-xtask` lets a Harness crate name the Gateway client, which sits outside the private container, and the tokio ban reads `[dependencies]` only.

### Sites

- [runvil-harness/crates/runvil/examples/run-prompt.rs](runvil-harness/crates/runvil/examples/run-prompt.rs): remove `Clock` and its impl. The Host's timer is `Arc::new(TokioTimer)`. `Duration`, `BoxFuture`, and `Timer` leave the imports, and `use harness_gateway_client::TokioTimer;` joins them. The header comment says "the stock tokio timer". `main` already uses the stock finder, the stock broker, and both chains, so the swap is the whole change there.
- [runvil-harness/crates/runvil/tests/suite/support.rs](runvil-harness/crates/runvil/tests/suite/support.rs): remove `Clock`, the imports only it used, and the words about it in the module doc and in the doc of `launch`. `launch` uses `Arc::new(TokioTimer)`, and its finder closure becomes `Arc::new(FindTextPrompt::new(PROMPT, source))`. The line `let source = source.to_owned();` and the imports `Error` and `ErrorCode` go with the closure. No other file of the suite names `Clock`.

### Checks

- The count stays at 3,979, because this commit only switches the facade suite's existing tests to the stock timer.
- Both listings stay as they are.
- `verify.ps1 -Tag t1c`, then `cargo run -p runvil --example run-prompt`, which must print `Hello, desk.`.

## Workshop: one commit that pins the Harness and uses the stock timer

### The switch

- [runvil-workshop/crates/workshop/agents/src/timer.rs](runvil-workshop/crates/workshop/agents/src/timer.rs): the file goes. By the workspace rule it moves to `cabinet/_trash/`, and git sees a deletion.
- [runvil-workshop/crates/workshop/agents/src/lib.rs](runvil-workshop/crates/workshop/agents/src/lib.rs): remove `mod timer;` and `pub use timer::TokioTimer;`. The first paragraph of the crate doc loses "the Host's timer,". The tier bullet under `## Invariants` gains one sentence: the crate's tests take the stock tokio timer from `harness-gateway-client`, the Harness's second public crate.
- [runvil-workshop/crates/workshop/agents/Cargo.toml](runvil-workshop/crates/workshop/agents/Cargo.toml): `[dev-dependencies]` gains `harness-gateway-client.workspace = true`. In `[dependencies]`, `tokio` loses the feature `time` and the comment loses its clause, since the timer was the only code that used it. The dev-dependency on `tokio` gains `time`, because the conversation tests call `tokio::time::timeout`.
- [runvil-workshop/crates/workshop/agents/tests/it/support.rs](runvil-workshop/crates/workshop/agents/tests/it/support.rs), [runvil-workshop/crates/workshop/server/src/agents.rs](runvil-workshop/crates/workshop/server/src/agents.rs), and [runvil-workshop/crates/workshop/server/src/agents/tests.rs](runvil-workshop/crates/workshop/server/src/agents/tests.rs): `TokioTimer` leaves the `workshop_agents` import and comes from `harness_gateway_client`. The four `Arc::new(TokioTimer)` read the same as before. `workshop-server` already depends on the Gateway client. The cleanup at each call site:
  - `run_for` in `support.rs`: the finder closure becomes `Arc::new(FindTextPrompt::new(AGENT, source))`, the setup becomes `RunSetup::new(..).has_user(true)`, and `Error` and `ErrorCode` leave the imports.
  - `AgentSessions::new` in `agents.rs`: the `Host` is built as `Arc::new(Host { .. })`, and `Inner` takes it as `host`.
  - `run_prompt_over` and the test of `execute` in `tests.rs`: the swap is the whole change.
- A search for `TokioTimer` and for "timer" in the two crates finds any doc comment that still says the agents crate holds the timer.

### The pin

- [runvil-workshop/Cargo.toml](runvil-workshop/Cargo.toml), lines 28 to 30: the three `rev` values become the Harness head, which is Harness commit 4. Then `cargo fetch`. `Cargo.lock` must change its 14 source lines for `runvil-harness`, gain `"harness-gateway-client",` in the dependency list of `workshop-agents`, and nothing else.

### Dry run, before the Harness is pushed

- Add the temporary `[patch."https://github.com/cppalliance/runvil-harness"]` section of the earlier walks to Workshop's root `Cargo.toml`, with path entries for `runvil`, `harness-gateway-client`, and `plugin-mcp`. It never goes into a commit.
- Make the edits of the switch.
- Run clippy with warnings denied and nextest for the two crates that change: `-p workshop-agents -p workshop-server`, without `--locked`, because the patch changes the lock file.
- Restore the root `Cargo.toml` and `Cargo.lock` with `git restore`. The edits of the switch stay in the working tree for the real commit.

### The real commit, after the Harness pull request is open

- Set the pin, run `cargo fetch`, then `verify-workshop.ps1 -Tag t1w`, the whole list of Workshop's `AGENTS.md`. Expected counts: 706 and 353 tests, the same as before.
- Commit, `git push origin master`, and open the pull request.

## Verification and commit messages

- Harness: `cabinet/_scratch/remove-ui-build/verify.ps1 -Tag <tag>` runs the whole list of the Harness's `AGENTS.md`. Every line must read `exit=0`.
- Workshop: `verify-workshop.ps1 -Tag <tag>` beside it does the same for Workshop, structural checks included.
- Each commit message is written from the staged diff by `.cursor/rules/commit.mdc`, saved in the scratch folder, and checked with `check-message.ps1 -Message <file> -Repo <path>`.

## Record

Harness commit 4 adds a section "What the build found" to the repo copy of the plan and sets the todo statuses. It covers the three Harness code commits and the Workshop dry run. The approved Cursor plan file stays as it is. The pull request numbers, the CI results, and the merges come after this commit, so I report them in chat.

## Publishing, in order

1. Harness: `git push origin master`, then `gh pr create --repo cppalliance/runvil-harness --base master --head vinniefalco:master`.
2. Workshop: the real commit above, then `git push origin master` and `gh pr create --repo cppalliance/runvil-workshop --base master --head vinniefalco:master`. Its body names the Harness pull request and says that it merges second.
3. Wait until every check on both pull requests has passed. On the Harness that is the CI workflow, which ends with `ci-green`. On Workshop it is the CI workflow with `ci-green` and the Bundle workflow, whose five builds take about 20 minutes. While they run I read the automatic reviews, and a finding that is right gets a fix as a new commit on the fork.
4. Merge the Harness: `git fetch upstream`, confirm that `upstream/master` is still `b5e2c935`, then `git push upstream master`, and confirm the new head with `gh`.
5. Merge Workshop: `git fetch upstream`, confirm that `upstream/master` is still `9fb795ef`, then `git push upstream master`, and confirm the same way.
6. Confirm that each fork equals its upstream, and report the CI run that each push starts on upstream `master`.

Workshop can pin the Harness head before the Harness merges. The last walk showed it: `cargo fetch` got a commit of `cppalliance/runvil-harness` that only an open pull request held, and the fast-forward push keeps the commit ids.

When to stop or change course:

- A check fails on a pull request. I fix it with a new commit on the fork, with no rewrite. If the new Harness commit changes code, Workshop moves its pin to it.
- Upstream `master` moved in either repo. I stop and tell you, because a rebase rewrites commits that the fork already holds.
- A check refuses the dev-dependency from `runvil` to the Gateway client. Commits 1 and 2 stand, and I stop and tell you before I change the approach to commit 3.

## What each step needs

- Harness commit 0 needs this plan's text, the file name above, and two clean trees.
- Harness commit 1 needs nothing from commit 0. Inside it, the type and its wiring come first, the tests and the pages are independent of each other after that, and the verification needs all of them.
- Harness commit 2 needs the type of commit 1.
- Harness commit 3 needs the type of commit 1 and the workspace dependency entry of commit 2. Its two sites are independent of each other.
- The Workshop dry run needs the type of commit 1 in the local Harness checkout. It needs nothing from commits 2 and 3.
- Harness commit 4 needs the results of commits 1 to 3 and of the dry run.
- The Harness pull request needs commit 4.
- The Workshop commit needs the id of Harness commit 4, an open Harness pull request so that the id can be fetched, and the edits that the dry run left in the working tree.
- The merges need every check green on both pull requests and two upstream branches that have not moved. Workshop's merge needs the Harness merge.

The Harness verification and the Workshop dry run are both full builds on one machine, so they run one after the other.

## Choices I settled

- The classroom stays as it is (confidence: medium - the offline broker plan made the same choice, and each student program pins an older Harness commit, so the stock timer there means moving each pin across API changes).
- `runvil` and the test Plugin take the Gateway client as a dev-dependency (confidence: medium - you asked for every copy, Cargo allows a dev-dependency on a crate that depends back, and the structural checks I read allow the edge, but it is the first such edge in this workspace and nothing has built it yet).
- `workshop-agents` takes the Gateway client as a dev-dependency for its one test fixture (confidence: medium - the other way is a test-only copy of the timer, which is what this plan removes).
- Three Harness code commits: add the type, then one commit for each crate that takes the new dev-dependency (confidence: medium - each is a step that its own tests check, and the last one isolates the dev-dependency that points back at `runvil`).
- "Green" means every check on both pull requests, the Bundle workflow of Workshop included (confidence: medium - it is the plain reading of "everything is green", and it adds about 20 minutes of waiting).
- "Clean up each call site" covers the `Host` and a `RunSetup` built in the same function, in the commit of the swap (confidence: medium - you said call sites, and the leftovers of the old timer were in the plan already).
- The cleanup stops at the function that the swap touches (confidence: medium - a wider sweep would turn a timer change into a rewrite of test helpers).
- The one-line finder of `run_prompt_over` stays a closure (confidence: medium - it is one line already, and the stock finder would need the name `browses` written a second time).
- One Workshop commit for the pin and the switch (confidence: medium - the pin moves only to get the type).
- The type is a unit struct that derives `Debug` alone, written `Arc::new(TokioTimer)` (confidence: medium - the workspace lint needs `Debug`, every site wraps it in an `Arc`, and `OfflineBroker` has the same shape).
- The tests pause tokio's clock (confidence: medium - a one-hour sleep that ends at once shows that the sleep is on tokio's timer, at the cost of `test-util` in one dev-dependency).
- One test runs a real prompt on the stock timer (confidence: medium - it shows a `Timer` effect of a run answered by the stock type, and it fits in the crate because the Gateway client depends on `runvil`).
- The plan commit and the record commit skip the full verification list (confidence: medium - they change only files under `plan/`, which the checks leave unread, and both earlier walks did the same).

## Not in this plan

- The classroom. Four student programs hold a copy: Robles's `checkprompt` and `host`, Kirill's `runner`, and Parsons's `verify-briefer.rs`. Robles's `host` pins the retired `cppalliance/runvil` repository, and the Briefer verifier is written for the old crate layout. Wheeler's `host` has a timer that blocks a thread with `std::thread::sleep`, which is a different defect.
- The other changes from the design chat: the timer moving from `Host` to `RunSetup`, the Host builder, a `Host` that lives only in an `Arc`, and the built-in Packs. Each one later moves the sites that this plan touches.
- The eight test timers of the Harness runner.
- Struct update syntax in functions that the swap leaves alone: `harness_recorded` in the test Plugin's tests, `run_agent_on` and `setup_for` in Workshop's server, and five places in the facade suite.
- The loose ends of the earlier walks: the two-model test broker, issue #29, PR #27, and the stale "Left open" bullets.

## Risks

- The dev-dependency from `runvil` back to the Gateway client is new here. Clippy, the doc builds, dylint, and `xtask api` all have to accept it, and commit 3's verification is the proof.
- The prompt of the run test is written from the guide and from the runner's own timer test, and nothing has run it. If the run reports another text, I find the reason before I change the expected text, and the record says what happened.
- I expect tokio's paused clock to land within one millisecond of the hour, so the test allows up to one second more. I have not run it.
- I read Workshop's tier table, which covers `workshop-*` crates only, and the list of its pin check, which allows the Gateway client. Neither has run with the new dev-dependency of `workshop-agents`, and that run happens on the real pin.
- The custom dylint rules are out of my sight, so that check is the one most likely to ask for a small change.
- If one of the open Harness pull requests (#56, #27, #26, #14) merges while this work is open, upstream `master` moves and I stop.
- PR #27 edits the example, so its conflict with `master` grows. That is expected.
- The stock finder fails another name a little differently from the closures it replaces: its prose is its own, it holds the name as a fact, and in `run_for` the message changes from `no agent named {name}` to `no prompt named {name}`. I searched each file: each of these Hosts is parsed under its one name, and the old message is in no assertion. The tests are the proof.
- After the cleanup, the facade suite and the test Plugin find their prompt through the stock finder. A closure as a finder stays covered by the runner's `host-tests.rs` and by Workshop's `agent_finder`.

# What the build found

The plan held in both repos. This record was written after the three Harness code commits and the Workshop dry run, and before anything was pushed. These points differ from the plan above, or add to it.

## Harness commit 0, `a346bdab`: the plan

- **The copy is the approved text, with every todo `pending`.** It was taken before the first todo changed its status.

## Harness commit 1, `6300d5a1`: the type

- **The run test passed as the plan wrote it.** The prompt returns `true|true|go`. The record holds one `Timer` effect whose payload is `{"Timer":{"seconds":3600.0}}`, its answer is `Timer`, and tokio's clock moved one hour during the run. That settles the second risk.
- **A sleep of one hour ends within the bound that the first test sets**, at least 3600 seconds and less than 3601 on tokio's paused clock. That settles the third risk. The second test puts a `tokio::time::timeout` of 3599 seconds over the same sleep, and the time runs out.
- **The custom lints passed the new code as the plan wrote it.**
- **Both listings and `Cargo.lock` stayed as they were**, as the plan expected.
- **Every command of the verification list passed.** The whole workspace ran 3,979 tests, all passing, with 25 skipped. The Gateway client has 160 tests, where it had 157.

## Harness commit 2, `a45bcf6b`: the test Plugin

- **The switch went as the plan wrote it.** `host_offering` takes `TokioTimer` and `FindTextPrompt`, and `Cargo.lock` gained its one line.
- **`Timer` and `ErrorCode` left the imports of `run.rs`, and `BoxFuture` and `Error` stay.** The recorder that the file defines returns `BoxFuture`, and two assertions name `Error::is_canceled`.
- **Every command of the verification list passed again**, and the count stayed at 3,979.

## Harness commit 3, `b4f347d0`: the facade

- **The dev-dependency that points back at `runvil` builds and passes every check**: clippy, both doc builds, `xtask api --check`, dylint, and the structural checks. That settles the first risk. `cargo doc -p runvil --no-deps` and `cargo deny check`, which CI runs beside that list, pass too.
- **`launch` lost its closure and its `to_owned` line**, and `support.rs` lost five imports: `Duration`, `Timer`, `BoxFuture`, `Error`, and `ErrorCode`. The example lost three: `Duration`, `Timer`, and `BoxFuture`.
- **The dev-dependency of `runvil` on `tokio` keeps its `time` feature.** The crate's tests and its example no longer call `tokio::time`, but the runtime that each of them builds must have tokio's timer, which a sleep of the stock timer needs.
- **`Cargo.lock` gained its one line**, the count stayed at 3,979, and `cargo run -p runvil --example run-prompt` printed `Hello, desk.`.

## The Workshop dry run

- **Workshop built against the local Harness checkout** through a temporary `[patch]` section, which was taken out again with `git restore` on `Cargo.toml` and `Cargo.lock`.
- **Clippy with warnings denied passed for `workshop-agents` and `workshop-server`.** Their tests passed too: 21 in `workshop-agents`, and 256 in `workshop-server` with 2 skipped, the same counts as before the switch.
- **Workshop's structural checks accept the new dev-dependency of `workshop-agents`.** `cargo test -p build-xtask` ran after the restore, on the old pin, and passed its 86 tests, and the wording check passed. That settles the fourth risk as far as a run before the real pin can.
- **The four call sites changed as the plan wrote them.** `run_for` uses `FindTextPrompt` and `RunSetup::new(..).has_user(true)`, `AgentSessions::new` builds its `Host` inside the `Arc`, and the two sites in the server's `tests.rs` changed their import alone.
- **One edit goes beyond the plan.** The module doc of `server/src/agents.rs` still said that a run's setup takes the granted roots of the workspace. The walk that removed the roots left that sentence behind. The Workshop commit corrects it, because the same module doc names the timer and changes in this commit.
- **Two doc comments of `server/src/agents.rs` say "the stock tokio timer"** where they said "the tokio timer": the module doc and the doc of the `host` field.
- **The old `TokioTimer` of Workshop derived `Clone`, `Copy`, `Debug`, and `Default`, and the stock one derives `Debug` alone.** Each of the four call sites writes `Arc::new(TokioTimer)`, which needs none of the other three.
- **The edits wait in Workshop's working tree** for the commit that pins the Harness.

## Still to come when this record was written

- The push of the Harness to the fork, and its pull request.
- The Workshop commit on the real pin, with Workshop's whole verification list, and its pull request.
- The two merges, the Harness first, once every check on both pull requests is green.

Each of those is reported in its pull request and in the chat, because a record of them would be one more commit after the merge.

## After the pull requests opened

A review of the Harness pull request led to one more commit, so this note was added with it, before either merge.

- **The pull requests are [#59](https://github.com/cppalliance/runvil-harness/pull/59) on the Harness and [#31](https://github.com/cppalliance/runvil-workshop/pull/31) on Workshop.**
- **`cargo fetch` got `f87bfbf5` from `cppalliance/runvil-harness` while only the open pull request held it**, so the planned order stood: both pull requests were open before either merge.
- **Workshop commit `4666b17f` passed Workshop's whole verification list on the real pin.** Its two test groups ran 706 and 353 tests, all passing, the same counts as before. `Cargo.lock` changed its 14 source lines for `runvil-harness` and gained one line for the new dev-dependency of `workshop-agents`.
- **The automatic review found a count in the new entry of the Harness page.** The entry said that each Host on tokio would otherwise write the same eight lines, and `AGENTS.md` keeps counts of things in the code out of an entry. Commit `33c90c64` says that each such Host would otherwise write that timer itself, so that sentence differs from the text this plan gave.
- **The automatic reviews raised no other point in either pull request.**
- **Workshop keeps its pin at `f87bfbf5`.** That commit holds all of the code, and `master` of `cppalliance/runvil-harness` holds it after the merge. The commits above it change text only.
