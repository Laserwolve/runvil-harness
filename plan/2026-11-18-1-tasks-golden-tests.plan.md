---
name: Golden tests for tasks
overview: Add 30 tests of Lua's `tasks` table to the `runvil` crate's suite. Each test is a small prompt file beside the log that running it through the Harness must produce. One new runner runs a prompt on a Host with a mock model and a paused clock, renders the run's whole record as text, and compares it with the stored log.
todos:
  - id: plan-copy
    content: "Copy this plan to plan/2026-11-18-1-tasks-golden-tests.plan.md on branch tasks-golden-tests and commit it alone"
    status: completed
  - id: runner
    content: "Add the runner, the mock model, and test 1, and show that a changed log line fails the test"
    status: completed
  - id: cases
    content: "Add tests 2 to 30, one commit per area, checking each generated log against its row in the table"
    status: completed
  - id: overlap
    content: "List which existing tests already cover each of the 30, for the PR description"
    status: completed
  - id: gist
    content: "Reword Test #29 in the gist to drop \"the same seed\""
    status: completed
  - id: verify
    content: "Run every verification command and vibelight"
    status: completed
  - id: pr
    content: "Push the branch to the fork and open one PR against cppalliance/runvil-harness"
    status: in_progress
---

# Golden tests for tasks

<product-contract>

## Product Requirements

Lua's `tasks` table lets a prompt launch, watch, steer, and stop tasks. It must be airtight before the model is allowed to launch tasks, and other people must be able to read how it behaves. This plan adds 30 tests that each pair the smallest prompt for one behavior with the log that the run must produce. The tests run through the Harness's public API, as a Host runs a prompt. All 30 arrive in one pull request with this plan.

- Problem and users:
  - The roughly 175 existing tests of tasks are Rust tests that step the Engine. A reader has to know the test support code to see what a prompt did and what came out.
  - No test of tasks runs through the Harness with a model that a Host supplied.
  - The users are the engineers who will build on tasks, and reviewers who need to see each behavior as a prompt and its outcome.
- Goals:
  - One test per row of the table in the Functional Specification, 30 in total, covering all 29 behaviors listed there.
  - A test is two files: a prompt and its expected log. Adding a test needs no new Rust beyond one line that names it.
  - A failing test says which line of the log differed, with the expected and the actual line.
- Non-goals:
  - Changing any production code, or any existing test.
  - Moving the tasks code into a module or a crate of its own.
  - Testing a fixed seed. The Harness draws a new seed for every run.
- Success criteria:
  - All 30 tests pass on two consecutive runs with no change to a stored log.
  - Changing one line of a stored log makes that test, and only that test, fail.
  - Every verification command in the Testing Plan passes.
- Constraints:
  - Tests reach the Harness only through the `runvil` crate, the one crate a Host depends on.
  - No real model, no network, and no real sleeping. The whole suite runs in seconds.
  - A stored log is never changed to match a run when the run contradicts the table. The disagreement is reported first.
- Open questions: None

## Functional Specification

A developer writes a prompt file, runs the suite once in bless mode to write its log, reads the log against what the prompt should do, and commits both. From then on the suite runs the prompt and fails if the log differs. The prompt itself tells the mock model what to answer, so nothing else describes a test.

- Actors and workflows:
  - Add a test: write `NN-name.md`, add one line naming it to the suite, run with `RUNVIL_BLESS=1`, review `NN-name.log`, commit.
  - Run the tests: the workspace test command runs them with every other test.
  - Change behavior on purpose: run with `RUNVIL_BLESS=1` and review the diff of the logs in the commit.
- Inputs and outputs:
  - Input: a prompt file, an ordinary Runvil prompt. A test whose prompt calls `execute` keeps the second prompt in a `helpers/` folder beside the tests.
  - Output: the expected log. One line per record that the recorder received, in the order received: the task id, the record's position in its task, `effect`, `answer`, or `event`, the effect's number or `-`, and the payload as JSON with sorted keys. One last line gives the run's result: `result ok` and the text, or `result error`, the error code, and the message.
  - The mock model reads its instruction from the newest user message of the request:
    - `reply <text>` answers `<text>` at once.
    - `after <seconds> reply <text>` answers after that many seconds on the paused clock.
    - `call <tool> <json>` asks for one tool call. Steps joined by ` then ` are taken one per round, so `call echo {} then reply done` is a two-round tool loop.
    - Any other text fails the round with an error that quotes the text.
- States and validation:
  - A guard test fails when a prompt file has no log, a log has no prompt, or a prompt file is not named in the suite. While a run is storing logs, the guard checks only that each prompt is named.
  - A missing log outside bless mode fails the test and names the bless command.
- Errors and recovery:
  - A mismatch reports the file, the first differing line number, the expected line, and the actual line.
  - A prompt that fails to parse fails its test with the parser's error.
- Security and privacy behavior: None
- Acceptance criteria:
  - Each test in the table below exists as a prompt and a log, and its log shows the stated outcome.
  - Each of the 29 behaviors is covered by at least one test, as the last column records.

### The 29 behaviors under test

1. **Chain.** A chain is one walk through a prompt's sections.
2. **Task.** A task is a section that `tasks.spawn` starts as its own chain. The chain that spawned it owns it.
3. **Lifetime.** A task lives as long as its owner chain.
4. **Turns.** Chains take turns on one thread, so only one chain's Lua runs at a time.
5. **Launching.** `tasks.spawn(target, opts)` returns a handle, `{ task = id }`, before the task runs.
6. **Inputs.** `opts` can set the task's `args`, `item`, and `index`. `var` and the model are copied at the spawn.
7. **Start.** A task first runs once its owner parks and a slot is free.
8. **Limit.** At most 8 tasks run at once by default. `tasks.concurrency(n)` changes a chain's limit.
9. **Result.** A task's result is the text its section returns. An error fails only that task.
10. **Join any.** `tasks.join_any(set)` waits for one task to end or reply. It returns the handle, `ok`, and the result.
11. **Join.** `tasks.join(set)` waits for every task and returns one entry per task, in the order listed.
12. **Errors.** A wait returns a failed task's error as data and never raises it.
13. **Once.** Each result goes to one wait only. A second wait for it raises `task_consumed`.
14. **Store.** Once a wait delivers a task, the task's store writes are visible to its owner.
15. **Timeout.** `opts.timeout` ends a wait after that many seconds and leaves the tasks running.
16. **Checking.** `tasks.pending()` lists the caller's live tasks. `tasks.ready(task)` says if one has ended or holds a reply no wait has taken.
17. **Status.** `tasks.status(task)` gives a task's state and what it waits on. `tasks.note(text)` sets a progress note.
18. **Canceling.** `tasks.cancel(task)` stops a task and every task it owns. A wait on it returns kind `canceled`.
19. **Steering.** Every task has an inbox of text messages. Only its owner fills it, with `tasks.send`.
20. **Receive.** `tasks.receive()` returns the oldest message, waiting if there is none.
21. **Reply.** `tasks.reply(value)` hands the owner a result, then waits for the next message, so the task stays open.
22. **Ownership.** Only the owner may wait on, check, send to, or cancel a task, though a task may read its own status. Others get `task_not_owned`.
23. **Correctness.** Ownership protects program correctness, not security: a task id is a guessable string.
24. **Endings.** A chain that ends normally while its task is still running fails with `tasks_live`. A chain that fails keeps its own error, and a task waiting after a reply does not count.
25. **Abandoned.** A task still live when its owner ends is `abandoned`, with every task under it.
26. **Ids.** The main walk is task `0`, and a task's id adds a dot and its owner's next child number: `0.0`, then `0.0.0` under it. A `call`, an `execute`, or a timed wait takes a number too.
27. **Events.** Each task that starts reports one `TaskStarted` event and exactly one terminal event.
28. **Repeatability.** The same seed, start time, and answers in the same order give the same run.
29. **Fanout.** `tasks.fanout` is Lua over `spawn`, `join_any`, and `cancel`. Each arm is an ordinary task.

### The 30 tests

| # | File stem | The prompt | The log must show | Behaviors |
|---|---|---|---|---|
| 1 | `01-spawn-and-join` | Spawns one task and joins it. | One start, one success, and the result. | 2, 5 |
| 2 | `02-survives-fall-through` | Spawns a task, falls through to the next section, joins it there. | The task succeeds after the section change. | 1, 3 |
| 3 | `03-queued-until-owner-waits` | Spawns a task, then polls `tasks.status` in a loop that never waits. | Every read is `queued`. | 4, 5, 7 |
| 4 | `04-inputs-copied-at-spawn` | Spawns with `args`, `item`, and `index`, then changes `var` and the model. | The task sees the values from the spawn. | 6 |
| 5 | `05-limit-of-one` | Sets the limit to 1 and spawns two tasks. | The second starts after the first succeeds. | 7, 8 |
| 6 | `06-default-limit-of-eight` | Spawns nine tasks that each wait on the mock model. | Eight start, and the ninth starts when one ends. | 8 |
| 7 | `07-join-two-tool-loops` | Launches two tasks that each block in a tool loop, and joins both. | Results in the order listed. | 11 |
| 8 | `08-join-any-first-to-end` | One task waits on the mock model and one returns at once. | `join_any` returns the second's handle, `ok`, and result. | 10 |
| 9 | `09-result-and-error` | One task returns text and one raises. | The owner gets the text, and `ok = false` with the error. | 9, 12 |
| 10 | `10-wait-twice` | Waits twice on one task. | The second wait raises `task_consumed`. | 13 |
| 11 | `11-store-after-join` | A task writes a store file. | Its owner reads it after the join. | 14 |
| 12 | `12-timed-join` | Time-limits a `join` while one task waits on the mock model. | It times out, and a second `join` collects the task. | 15 |
| 13 | `13-pending` | Spawns three tasks and joins the second. | `tasks.pending()` lists the first and third. | 16 |
| 14 | `14-ready-after-end` | Reads `tasks.ready` before and after a task ends. | `false`, then `true`. | 16 |
| 15 | `15-ready-with-reply` | A task replies and stays open. | `tasks.ready` reads `true` until a wait takes the reply, then `false`. | 16 |
| 16 | `16-status-through-a-model-call` | Reads `tasks.status` before, during, and after a task's model call. | `queued`, `chat`, then `done`. | 17 |
| 17 | `17-note` | A task sets two notes. | Its owner's `tasks.status` shows the second. | 17 |
| 18 | `18-cancel` | Cancels a queued task, one waiting on the mock model, and one that owns a task. | Each wait returns kind `canceled`. | 18 |
| 19 | `19-steer-with-replies` | A worker replies, is sent "shorten it", replies again, is sent "done". | The worker returns. | 19, 21 |
| 20 | `20-receive-oldest-first` | Sends a task two messages before it runs. | `tasks.receive()` returns them oldest first, and a third call waits. | 19, 20 |
| 21 | `21-sibling-not-owned` | A task tries to wait on, check, send to, and cancel its sibling. | Each raises `task_not_owned`. | 22 |
| 22 | `22-self-and-unknown-not-owned` | A task tries to cancel itself and the made-up id `9.9`. | Each raises `task_not_owned`. | 22, 23 |
| 23 | `23-own-status` | A task reads `tasks.status(sys.taskid)`. | Its own status, with state `running`. | 22 |
| 24 | `24-return-with-live-task` | Returns while a task that owns a task runs. | The run fails with `tasks_live` and both end `abandoned`. | 24, 25 |
| 25 | `25-error-with-live-task` | Makes a call that is refused, uncaught, while a task runs. | The run fails with that call's error, `task_not_owned`, and the task ends `abandoned`. | 24 |
| 26 | `26-return-with-waiting-task` | Returns while a task waits after a reply. | The run ends normally and the task ends `abandoned`. | 24 |
| 27 | `27-nested-ids` | A task spawns a task. | Ids `0.0` and `0.0.0`, each with one start and one terminal event. | 26, 27 |
| 28 | `28-ids-skip-other-children` | Makes a `call` and an `execute`, spawns a task, waits on it with a time limit, spawns another. | The tasks are `0.2` and `0.4`. | 26 |
| 29 | `29-same-log-twice` | Runs one prompt twice. | The two logs are identical. | 28 |
| 30 | `30a-fanout`, `30b-spawns-by-hand` | Fans out over three members, and spawns three tasks by hand. | The task events of the two logs match. | 29 |

</product-contract>
<implementation-contract>

## Technical Design

The change is test code and test data in the `runvil` crate, plus one dev-dependency feature. No production code and no facade listing changes. The runner builds one Host for every test, so a test differs from another only in its prompt.

- Architecture:
  - The runner is a module of the crate's existing `suite` test target, beside the modules already there (`crates/runvil/tests/suite/main.rs`).
  - Each run goes `Host::parse`, `Harness::new`, `Harness::run`, as the suite's `launch` does today (`crates/runvil/tests/suite/support.rs`), with a `MemoryRecorder` whose records become the log.
  - The clock is tokio's, paused. A sleep completes in the order of its deadline with no real waiting, which fixes the order in which timers and delayed model replies answer.
- Modules and interfaces:
  - `golden`: builds the Host and the `RunSetup`, runs a prompt by file stem, renders the record, and compares or blesses. Its Host finds prompts by file name in the test folder, sleeps on the paused clock, and stamps every run's start as the Unix epoch.
  - `mock_model`: an `InferenceBroker` that lists one model, `mock-model`, and answers by the instruction grammar in the Functional Specification.
  - `tasks`: one short test per prompt naming its file stem, the guard test, and the two tests that compare logs with each other (29 and 30).
  - One fixture Pack with a single `echo` tool, for the tool loops of test 7, registered as `crates/runvil/tests/suite/vfs.rs` registers its `acme/desk` Pack.
- File and public API changes:
  - New: `crates/runvil/tests/suite/golden.rs`, `mock_model.rs`, `tasks.rs`, and three `mod` lines in `main.rs`.
  - New: `crates/runvil/tests/golden/tasks/`, holding 31 prompts, 31 logs, and `helpers/` with the prompt that test 28 executes.
  - Changed: `crates/runvil/Cargo.toml`, where the tokio dev-dependency gains the `test-util` feature for the paused clock.
  - New: `plan/2026-11-18-1-tasks-golden-tests.plan.md`, this plan.
  - Public API: none. `crates/runvil/public-api.txt` and `crates/runvil-engine/public-api.txt` stay as they are.
- Data, persistence, failure, security, and privacy constraints:
  - A stored log is read from the source tree at test time, and written only when `RUNVIL_BLESS=1` is set.
  - A prompt in this suite uses no randomness, because the seed differs on every run.

</implementation-contract>
<verification-contract>

## Testing Plan

The 30 tests are the deliverable, so the plan for testing is about trusting the runner and the stored logs. Each log is generated once and then read line by line against its row in the table before it is committed.

- Unit:
  - One test of the comparison: two texts that differ on one line produce a message with that line number, the expected line, and the actual line.
- Integration and end-to-end:
  - The 30 tests of the table, through the Harness's public API.
  - The guard test over the test folder.
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
  - Every command above passes.
  - The suite passes twice in a row with no stored log changed.
  - A hand edit of one stored log fails exactly that test, and the edit is then reverted.
  - Every stored log has been read against its row of the table, and every disagreement is written into the Decision Record.

</verification-contract>
<decision-record>

## Decision Record

- Decisions:
  - One pull request holds the plan, the runner, and all 30 tests. The standing rule is "Keep PRs small... Split big work into smaller steps, each with its own plan". Asked which governs here, Andrew chose "One PR with all 30".
  - The tests go through the Harness, not the Engine alone. Andrew's note from the session of 2026-11-17: "test the tasks through the harness". Vinnie, in the huddle of 2026-11-18, described the mock model as an inference broker passed to the Harness that returns canned results.
  - A log holds the whole record: every effect, answer, and event. Andrew chose "Whole record". It shows timers and what the mock model was sent, and it is the same record that replay will compare.
  - Test 29 runs one prompt twice and compares the logs. The Harness draws the seed with `rand::random()` (`crates/harness-internal/runner/src/prepare.rs`), and `RunSetup` has no seed field (`crates/harness-internal/runner/src/setup.rs`), so a test here cannot hold the seed fixed. Andrew chose "Run twice, compare logs". Its wording in the shared list of tests drops "the same seed".
  - The mock model's instructions sit in the prompt's own messages. A test stays two files, and a reader sees the model's answer where the prompt asks for it.
  - The clock is paused, not real. Tests of timeouts and of "while a task waits on the model" need a fixed order of answers, and real sleeps would make the order depend on the machine.
  - The tests join the existing `suite` target. A new test target or crate would add a link step and nothing else.
  - The existing tests stay untouched, and a new test may cover what an existing one covers. The new suite's purpose is a readable prompt and outcome for each behavior. The PR description lists the overlap.
  - The mock model lists one model. Test 4 switches models with `models.use('strong')`, and the switch shows in the label that `models.current()` reports, so a second model would add nothing.
  - In test 25 the owner fails through a refused call, `tasks.cancel('9.9')`, left uncaught. Lua's `error()` puts a stack trace in the run's error, and the trace names line numbers of `crates/runvil-internal/lua/src/__impl_coro.lua`, so the stored log would change with any edit of that file.
  - A prompt reports what it observed with `log()`, which the record holds as an event, and returns a short summary. Each observation is then one line of the stored log.
- Rejected alternatives:
  - A runner on the Engine with its scripted model. It could fix the seed, but it would not go through the Harness or a Host's broker. Revisit if a test needs a fixed seed.
  - A log of events only. It is about a third shorter, and it hides timers and model requests. Revisit if the logs prove too noisy to review.
  - A third file of canned replies beside each prompt. It splits one test across three files. Revisit if the instruction grammar grows past the four forms.
  - One test function that loops over the folder. A failure would not name its test in the test report.
- Assumptions, risks, and notes:
  - The 29 behaviors were checked by reading the code at commit `4afed45fe`, not by running it. A generated log may show that a row of the table is wrong. That is a finding to report, and the row or the code changes before the log is stored.
  - In a trial at commit `4afed45fe`, one prompt with two tasks, a timed join, and a delayed model reply gave the same 30-line record on two runs with different seeds.
  - A change to any event's fields changes every stored log. Bless mode rewrites them, and the commit's diff shows what changed.
  - Two sleeps with the same deadline could complete in either order. Prompts give concurrent model replies different delays.
  - Justin's command-line Host and Kirill's replay need the same answer to what a stored log holds. This plan's format is a proposal to them, not a decision for them.
  - What the build found: every stored log matched its row of the table, with the one change to test 25 above. No row of the list of behaviors had to change.
  - Verification on 2026-11-18: the workspace has 4,083 passing tests, 34 of them new (31 in `tasks`, 2 in `mock_model`, 1 in `golden`), and the lint, format, docs, facade surface, structural, and wording commands pass. `cargo dylint` did not run, because it is not installed on this machine.
  - `vibelight` outlines a crate's `src`, so its run over the workspace says nothing about a test tree. Run on the three new files, it gave three `test-knob` leads, for the words `mock` and `test` in names inside test code. Nothing was changed for them.
  - The 31 stored logs hold 931 lines in total.
  - A search of the existing tests found tests that assert each of the 30 behaviors, in full for 21 and in part for 9. All of them step the Engine, and none runs through the Harness. The PR description lists the closest existing tests for each.

### Deferred and Out of Scope

- Deferred: a test that holds the seed fixed, until a test of the Engine or a replay driver can supply one.
- Deferred: removing an existing test that a new one makes redundant, until mutation testing shows the breakages it catches are still caught.
- Out of scope: moving the tasks code into its own module or crate.
- Out of scope: reading a task's conversation from outside the task, which no `tasks` function does.
- Out of scope: tasks that the model launches.

</decision-record>
<project-survey>

## Project Survey

- The `runvil` crate's suite already runs prompts through the public API: `launch`, `no_packs`, an offline broker, and a tokio clock in `crates/runvil/tests/suite/support.rs`.
- A scripted broker that answers from the last message exists in `crates/runvil/tests/suite/broker.rs`, and brokers that return tool calls exist in `crates/harness-internal/runner/tests/it/scripted.rs`.
- A fixture Pack registered through the facade exists in `crates/runvil/tests/suite/vfs.rs`.
- `MemoryRecorder::records` returns each record's task id, position, kind, effect number, and JSON payload (`crates/harness-internal/runner/src/recorder.rs`, `recorder_memory.rs`).
- The Host's `Timer` supplies both `sleep` and `now`, and the Harness stamps a run's start with `now` (`crates/harness-internal/runner/src/performers.rs`, `harness.rs`).
- The run's default limit on tasks running at once is 8 (`crates/runvil-internal/runtime/src/execute/params.rs`).
- No golden-file runner exists in the repository. The existing tests of tasks are under `crates/runvil-internal/runtime/src/execute/tests/` and `execute/run/`.

</project-survey>
<execution-plan>

## Execution Instructions

- Work items:
  1. On branch `tasks-golden-tests`, cut from upstream `master` at `4afed45fe`, copy this plan to `plan/2026-11-18-1-tasks-golden-tests.plan.md` and commit it alone.
  2. Add `golden`, `mock_model`, the `echo` Pack, the `test-util` feature, and test 1. Edit test 1's stored log by hand, confirm the failure message, and revert the edit.
  3. Add tests 2 to 30 in the order of the table, one commit per area: launching and lifetime (1 to 6), waiting and results (7 to 12), finding out what is running (13 to 17), canceling (18), steering (19 and 20), ownership (21 to 23), endings (24 to 26), ids and repeatability (27 to 29), fanout (30).
  4. For each test, generate the log with `RUNVIL_BLESS=1`, read it against the row of the table, and stop to report any disagreement before storing it.
  5. Search the existing tests for each of the 30 behaviors and write the overlap into the PR description.
  6. Reword Test #29 in Andrew's gist of the list to "Run one prompt twice. The two logs are identical."
  7. Run every command of the Testing Plan, then `vibelight`.
  8. Push the branch to the fork and open one pull request against `cppalliance/runvil-harness` `master`. Andrew tags the human reviewers and posts the link in the channel.
- Dependencies and verification:
  - Item 2 comes before item 3, because every test needs the runner.
  - Item 7 runs after the last code change, and again after any fix it prompts.
  - Commits use the GitHub no-reply address, and nothing is pushed to the cppalliance repository.

</execution-plan>
