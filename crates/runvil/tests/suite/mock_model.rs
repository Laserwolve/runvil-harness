//! The mock model of the golden tests: an inference broker that answers
//! each round from an instruction in the request's own messages, so a
//! prompt says what its model replies and a test needs no script beside
//! it.
//!
//! The instruction is the newest user message of the request. It holds one
//! or more steps joined by ` then `, and the broker takes one step per
//! round: the step numbered by the assistant messages that follow the
//! instruction. A step is one of:
//!
//! - `reply <text>` answers `<text>` at once.
//! - `after <seconds> reply <text>` answers `<text>` once that many seconds
//!   have passed on tokio's clock, which the golden tests pause.
//! - `call <tool> <json>` asks for one call of the tool with that wire
//!   name, passing the JSON object as its arguments.
//!
//! Any other request fails its round with an error that quotes what the
//! broker read, so a prompt with a misspelled instruction fails its test.

use std::num::NonZeroU32;
use std::time::Duration;

use runvil::effect::Round;
use runvil::harness::InferenceBroker;
use runvil::model::{
    Completion, CompletionOptions, CompletionResult, Message, ModelBinding, ModelCatalog, ModelId,
    ModelInfo, Role, ThinkingMode, ToolCall, ToolSchema, make_completion_error,
};
use runvil::{BoxFuture, Error, ErrorCode};
use serde_json::Value;

/// The model a golden run selects.
pub(crate) const MODEL: &str = "mock-model";

/// The context window the model lists: 131,072 tokens.
const WINDOW: NonZeroU32 = NonZeroU32::MIN.saturating_add(131_071);

/// What separates an instruction's steps.
const THEN: &str = " then ";

/// One step of an instruction: what the model does in one round.
#[derive(Debug, PartialEq)]
enum Step {
    /// Answer `text` once `after` has passed.
    Reply { after: Duration, text: String },
    /// Ask for one call of `tool` with `args`.
    Call { tool: String, args: Value },
}

/// Lists one model and answers each round by its request's instruction.
pub(crate) struct MockModel {
    catalog: ModelCatalog,
}

impl MockModel {
    /// A mock model listing [`MODEL`].
    pub(crate) fn new() -> Result<MockModel, Error> {
        let id = ModelId::gateway(MODEL)?;
        let model = ModelInfo::new(id, "the mock model", WINDOW, ThinkingMode::Never);
        Ok(MockModel {
            catalog: ModelCatalog::new([model])?,
        })
    }
}

impl InferenceBroker for MockModel {
    fn models(&self) -> BoxFuture<'static, Result<ModelCatalog, Error>> {
        let catalog = self.catalog.clone();
        Box::pin(async move { Ok(catalog) })
    }

    fn chat(
        &self,
        binding: ModelBinding,
        messages: Vec<Message>,
        _tools: Vec<ToolSchema>,
        _options: CompletionOptions,
        round: Round,
    ) -> BoxFuture<'static, Result<Box<Completion>, Error>> {
        let model = binding.id().name().to_owned();
        let step = step_of(&messages);
        Box::pin(async move {
            let result = match step.map_err(refusal)? {
                Step::Reply { after, text } => {
                    tokio::time::sleep(after).await;
                    CompletionResult::Text(text)
                }
                Step::Call { tool, args } => {
                    let id = format!("call-{}", round.id.get());
                    CompletionResult::ToolCalls(vec![ToolCall::from_parts(id, tool, args)?])
                }
            };
            Completion::from_result(result, model).map(Box::new)
        })
    }
}

/// The round's failure for a request the mock model cannot answer.
fn refusal(why: String) -> Error {
    make_completion_error(ErrorCode::Unavailable, why)
}

/// The step `messages` ask for: the step of the newest user message that
/// the assistant messages after it have not taken yet.
fn step_of(messages: &[Message]) -> Result<Step, String> {
    let Some(at) = messages
        .iter()
        .rposition(|message| message.role() == Role::User)
    else {
        return Err("the mock model needs a user message to read its instruction from".to_owned());
    };
    let instruction = messages[at].content();
    let taken = messages[at..]
        .iter()
        .filter(|message| message.role() == Role::Assistant)
        .count();
    let steps: Vec<&str> = instruction.split(THEN).collect();
    let Some(step) = steps.get(taken) else {
        return Err(format!(
            "the mock model's instruction {instruction:?} has {} steps, and round {} needs one more",
            steps.len(),
            taken + 1
        ));
    };
    parse_step(step)
}

/// One step's text as a [`Step`], or what the text should have been.
fn parse_step(step: &str) -> Result<Step, String> {
    let unknown = || {
        format!(
            "the mock model cannot read the step {step:?}: required `reply <text>`, \
             `after <seconds> reply <text>`, or `call <tool> <json>`"
        )
    };
    if let Some(text) = step.strip_prefix("reply ") {
        return Ok(Step::Reply {
            after: Duration::ZERO,
            text: text.to_owned(),
        });
    }
    if let Some(rest) = step.strip_prefix("after ") {
        let (seconds, text) = rest.split_once(" reply ").ok_or_else(unknown)?;
        let seconds: u64 = seconds.parse().map_err(|_| unknown())?;
        return Ok(Step::Reply {
            after: Duration::from_secs(seconds),
            text: text.to_owned(),
        });
    }
    if let Some(rest) = step.strip_prefix("call ") {
        let (tool, json) = rest.split_once(' ').ok_or_else(unknown)?;
        let args = serde_json::from_str(json).map_err(|_| unknown())?;
        return Ok(Step::Call {
            tool: tool.to_owned(),
            args,
        });
    }
    Err(unknown())
}

#[test]
fn a_step_is_the_one_the_assistant_messages_after_the_instruction_have_not_taken() {
    let instruction = Message::user("call echo_say {\"text\":\"a\"} then after 5 reply done");
    let first = step_of(std::slice::from_ref(&instruction));
    assert_eq!(
        first,
        Ok(Step::Call {
            tool: "echo_say".to_owned(),
            args: serde_json::json!({ "text": "a" }),
        })
    );
    let second = step_of(&[
        Message::user("reply an older instruction"),
        instruction.clone(),
        Message::assistant(""),
        Message::tool("call-0", "a"),
    ]);
    assert_eq!(
        second,
        Ok(Step::Reply {
            after: Duration::from_secs(5),
            text: "done".to_owned(),
        })
    );
    let third = step_of(&[instruction, Message::assistant(""), Message::assistant("")]);
    assert!(
        third
            .as_ref()
            .is_err_and(|why| why.contains("has 2 steps, and round 3 needs one more")),
        "{third:?}"
    );
}

#[test]
fn a_step_the_mock_model_cannot_read_is_refused_with_the_forms_it_takes() {
    for step in [
        "say hello",
        "after soon reply x",
        "call echo_say",
        "call echo_say {",
    ] {
        let refused = parse_step(step);
        assert!(
            refused
                .as_ref()
                .is_err_and(|why| why.contains(step) && why.contains("required `reply <text>`")),
            "{step}: {refused:?}"
        );
    }
}
