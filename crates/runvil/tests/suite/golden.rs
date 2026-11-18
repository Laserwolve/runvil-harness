//! The golden runner: it runs a prompt file through a Harness and compares
//! the run's log with the log stored beside the prompt.
//!
//! A golden test is two files in `tests/golden/tasks/`: `<stem>.md`, an
//! ordinary prompt, and `<stem>.log`, the log its run must produce. The log
//! holds one line per record the run's recorder received, in the order
//! received: the record's task id, its position in that task, its kind,
//! the number of its effect or `-`, and its payload as JSON. One last line
//! holds the run's result.
//!
//! Every golden run has the same Host and the same setup, so a test
//! differs from another only in its prompt: the Host finds prompts by file
//! name in the tests' folder and in its `helpers/` folder, which holds the
//! prompts a test executes, holds the built-in
//! Packs and [`ECHO`], sleeps on tokio's clock, which each test pauses, and
//! stamps every run's start as the Unix epoch. The model is the
//! [`MockModel`]. The Harness draws a new seed for every run, so a golden
//! prompt uses no randomness.
//!
//! Setting [`BLESS`] in the environment stores each run's log in place of
//! comparing it.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use runvil::harness::{MemoryRecorder, Packs, PromptSet, Record, Timer};
use runvil::packs::PackId;
use runvil::plugin::{Pack, PackInfo, PackRegistry, ToolContext};
use runvil::{
    BoxFuture, Error, ErrorCode, Harness, Host, RunSetup, Timestamp, ToolId, ToolInfo, ToolOutput,
    make_tool_error,
};
use serde_json::{Value, json};

use crate::mock_model::{MODEL, MockModel};

/// The environment variable that, when set, makes [`check`] store each
/// run's log instead of comparing it.
const BLESS: &str = "RUNVIL_BLESS";

/// The Pack `acme/echo`, whose one tool, `echo/say`, answers the `text` it
/// is given: the tool a golden prompt's model loop calls.
const ECHO: PackInfo = PackInfo::new("acme/echo").description("Says back what it is told");

/// One run's instance of [`ECHO`].
struct Echo {
    say: ToolInfo,
}

impl Pack for Echo {
    fn tools(&self) -> Vec<ToolInfo> {
        vec![self.say.clone()]
    }

    fn call<'a>(
        &'a self,
        cx: ToolContext<'a>,
        args: Value,
    ) -> BoxFuture<'a, Result<ToolOutput, Error>> {
        Box::pin(async move {
            match args.get("text").and_then(Value::as_str) {
                Some(text) => Ok(ToolOutput::trusted(text)),
                None => Err(make_tool_error(
                    ErrorCode::ToolInvalidArguments,
                    format!("{} requires a string `text`, got {args}", cx.tool()),
                )),
            }
        })
    }
}

/// Makes one run's [`Echo`], whose tool sits under the Pack id `id`.
fn new_echo(id: &PackId) -> Result<Arc<dyn Pack>, Error> {
    let say = ToolInfo::new(
        ToolId::parse(&format!("{id}/say"))?,
        "Says back the text it is given.",
        json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"],
        }),
    );
    Ok(Arc::new(Echo { say }))
}

/// Sleeps on tokio's clock and reads every run's start as the Unix epoch.
struct PausedClock;

impl Timer for PausedClock {
    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(duration))
    }

    fn now(&self) -> Timestamp {
        Timestamp::UNIX_EPOCH
    }
}

/// Whether [`BLESS`] is set, so this process stores logs and compares
/// none.
pub(crate) fn blessing() -> bool {
    std::env::var_os(BLESS).is_some()
}

/// The folder that holds the golden prompts and their logs.
pub(crate) fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/tasks")
}

/// The Host every golden run shares the shape of.
fn host() -> Arc<Host> {
    let mut registry = PackRegistry::new();
    if let Err(error) = registry.insert(ECHO, new_echo) {
        panic!("the registry takes the echo Pack: {error}");
    }
    let mut prompts = PromptSet::new();
    for directory in [folder(), folder().join("helpers")] {
        if let Err(error) = prompts.insert_dir(&directory) {
            panic!(
                "the golden folder {} is listed: {error}",
                directory.display()
            );
        }
    }
    Arc::new(Host {
        packs: Packs::new(registry),
        prompts: Arc::new(prompts),
        timer: Arc::new(PausedClock),
    })
}

/// Runs the prompt `<stem>.md` through a Harness and returns its log.
async fn run(stem: &str) -> String {
    let host = host();
    let prompt = match host.parse(&format!("{stem}.md")) {
        Ok(prompt) => prompt,
        Err(error) => panic!("{stem}.md does not parse: {error}"),
    };
    let model = match MockModel::new() {
        Ok(model) => model,
        Err(error) => panic!("the mock model's catalog builds: {error}"),
    };
    let recorder = Arc::new(MemoryRecorder::new());
    let setup = RunSetup {
        selected_model: Some(MODEL.to_owned()),
        ..RunSetup::new(recorder.clone(), Arc::new(model))
    };
    match Harness::new(host, prompt, setup).run().await {
        Ok(report) => render(&recorder.records(report.run_id), &report.result),
        Err(error) => panic!("the memory recorder takes every write of {stem}.md: {error}"),
    }
}

/// The log of a run: one line per record, then the run's result.
fn render(records: &[Record], result: &Result<String, Error>) -> String {
    let mut lines: Vec<String> = records
        .iter()
        .map(|record| {
            let effect = record
                .effect_id
                .map_or_else(|| "-".to_owned(), |id| id.to_string());
            format!(
                "{} {} {} {effect} {}",
                record.task_id,
                record.task_seq,
                record.kind.as_str(),
                record.payload
            )
        })
        .collect();
    lines.push(match result {
        Ok(text) => format!("result ok {}", json!(text)),
        Err(error) => format!("result error {} {}", error.code.tag(), json!(error.message)),
    });
    lines.join("\n") + "\n"
}

/// Where `actual` first differs from `expected`, as the line number with
/// the expected and the actual line, or `None` when the two are the same.
fn difference(expected: &str, actual: &str) -> Option<String> {
    let show = |line: Option<&str>| line.map_or_else(|| "no line".to_owned(), str::to_owned);
    let (mut expected, mut actual) = (expected.lines(), actual.lines());
    let mut number = 0_usize;
    loop {
        number += 1;
        let (wanted, got) = (expected.next(), actual.next());
        if wanted.is_none() && got.is_none() {
            return None;
        }
        if wanted != got {
            return Some(format!(
                "line {number}\n  expected: {}\n    actual: {}",
                show(wanted),
                show(got)
            ));
        }
    }
}

/// Runs the prompt `<stem>.md` and requires its log to be the one stored
/// in `<stem>.log`, then returns the log. With [`BLESS`] set, it stores the
/// run's log there instead.
pub(crate) async fn check(stem: &str) -> String {
    let actual = run(stem).await;
    let path = folder().join(format!("{stem}.log"));
    if blessing() {
        if let Err(error) = std::fs::write(&path, &actual) {
            panic!("cannot store {}: {error}", path.display());
        }
        return actual;
    }
    let expected = match std::fs::read_to_string(&path) {
        Ok(expected) => expected,
        Err(error) => panic!(
            "cannot read the stored log {}: {error}; run with {BLESS}=1 to store the run's log",
            path.display()
        ),
    };
    if let Some(difference) = difference(&expected, &actual) {
        panic!(
            "the run of {stem}.md differs from {stem}.log at {difference}\n\
             run with {BLESS}=1 to store the run's log if the change is intended"
        );
    }
    actual
}

#[test]
fn a_difference_names_the_first_line_that_differs_with_both_lines() {
    assert_eq!(difference("a\nb\n", "a\nb\n"), None);
    assert_eq!(
        difference("a\nb\nc\n", "a\nB\nC\n"),
        Some("line 2\n  expected: b\n    actual: B".to_owned())
    );
    assert_eq!(
        difference("a\n", "a\nb\n"),
        Some("line 2\n  expected: no line\n    actual: b".to_owned())
    );
    assert_eq!(
        difference("a\nb\n", "a\n"),
        Some("line 2\n  expected: b\n    actual: no line".to_owned())
    );
}
