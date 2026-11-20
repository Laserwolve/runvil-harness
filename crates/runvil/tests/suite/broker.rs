//! The inference broker a launcher hands each run's `RunSetup`: a section's own
//! round reaches it with origin `Chat`, and a nested `models.infer` round
//! with origin `Infer`, each under its run-wide round id. Each round asks
//! it for the model the Host selected, or for the first model it lists
//! when the Host selected none.

use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use runvil::effect::Round;
use runvil::event::ReplyOrigin;
use runvil::harness::{InferenceBroker, MemoryRecorder};
use runvil::ids::RoundId;
use runvil::model::{
    Completion, CompletionOptions, CompletionResult, Message, ModelBinding, ModelCatalog, ModelId,
    ModelInfo, ThinkingMode, ToolSchema,
};
use runvil::{BoxFuture, Error, RunSetup};

use crate::support::{launch, no_packs};

/// The one model the scripted broker lists and the Host selects.
const MODEL: &str = "scripted-model";

/// A program that infers once and then chats once: the nested round
/// first, then the section's own round over its message list.
const INFERS_THEN_CHATS: &str = "---\nname: rounds\ndescription: infers then chats\n\
    runvil: 0\n---\n\n\
    ## Only\n\n```lua\n\
    local inferred = models.infer('infer round')\n\
    messages:user('chat round')\n\
    models.loop()\n\
    return inferred .. '|' .. messages[#messages].content\n\
    ```\n";

/// Lists its catalog, notes each round it serves and the model the round
/// asks for, and replies `re: <last message>` to every round.
struct ScriptedBroker {
    catalog: ModelCatalog,
    rounds: Mutex<Vec<Round>>,
    asked: Mutex<Vec<String>>,
}

/// A scripted broker that lists a model of each of `names`, in that order.
fn listing(names: &[&str]) -> Result<Arc<ScriptedBroker>, Box<dyn std::error::Error>> {
    let window = NonZeroU32::new(131_072).ok_or("131072 is non-zero")?;
    let mut models = Vec::new();
    for name in names {
        let id = ModelId::gateway(*name)?;
        models.push(ModelInfo::new(
            id,
            "a scripted model",
            window,
            ThinkingMode::Never,
        ));
    }
    Ok(Arc::new(ScriptedBroker {
        catalog: ModelCatalog::new(models)?,
        rounds: Mutex::new(Vec::new()),
        asked: Mutex::new(Vec::new()),
    }))
}

impl InferenceBroker for ScriptedBroker {
    fn models(&self) -> BoxFuture<'static, Result<ModelCatalog, Error>> {
        let catalog = self.catalog.clone();
        Box::pin(async move { Ok(catalog) })
    }

    fn chat(
        &self,
        _binding: ModelBinding,
        messages: Vec<Message>,
        _tools: Vec<ToolSchema>,
        options: CompletionOptions,
        round: Round,
    ) -> BoxFuture<'static, Result<Box<Completion>, Error>> {
        self.rounds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(round);
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(options.model().to_owned());
        let asked = messages
            .last()
            .map(|message| message.content().to_owned())
            .unwrap_or_default();
        Box::pin(async move {
            Completion::from_result(CompletionResult::Text(format!("re: {asked}")), MODEL)
                .map(Box::new)
        })
    }
}

#[tokio::test]
async fn a_sections_round_reaches_the_broker_as_chat_and_a_nested_infer_round_as_infer() {
    let broker = listing(&[MODEL]).expect("one model is a valid catalog");
    let setup = RunSetup {
        selected_model: Some(MODEL.to_owned()),
        ..RunSetup::new(Arc::new(MemoryRecorder::new()), broker.clone())
    };
    let report = launch(no_packs(), INFERS_THEN_CHATS, setup)
        .run()
        .await
        .expect("the run is reported");

    assert_eq!(
        report.result,
        Ok("re: infer round|re: chat round".to_owned())
    );
    assert_eq!(
        *broker.rounds.lock().expect("the round log is healthy"),
        [
            Round::new(RoundId::new(0), ReplyOrigin::Infer),
            Round::new(RoundId::new(1), ReplyOrigin::Chat),
        ],
        "each round reaches the broker numbered in dispatch order, under the path that sent it"
    );
}

/// Sends the shipped `hello.md` from a Host that selected `selected` to a
/// broker that lists the models `first` and `second`, in that order, and
/// returns the model each round asked the broker for.
async fn asked_for(selected: Option<&str>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../prompts/hello.md");
    let hello = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let broker = listing(&["first", "second"])?;
    let setup = RunSetup {
        selected_model: selected.map(str::to_owned),
        ..RunSetup::new(Arc::new(MemoryRecorder::new()), broker.clone())
    };
    let report = launch(no_packs(), &hello, setup).run().await?;
    assert_eq!(
        report.result,
        Ok("re: Say \"Hello, world!\"".to_owned()),
        "the run ends with the broker's reply"
    );
    let asked = broker.asked.lock().unwrap_or_else(PoisonError::into_inner);
    Ok(asked.clone())
}

#[tokio::test]
async fn a_host_that_selects_no_model_has_the_broker_asked_for_the_first_model_it_lists()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(asked_for(None).await?, ["first"]);
    Ok(())
}

#[tokio::test]
async fn a_host_that_selects_a_model_has_the_broker_asked_for_that_model()
-> Result<(), Box<dyn std::error::Error>> {
    for selected in ["second", "first"] {
        assert_eq!(asked_for(Some(selected)).await?, [selected]);
    }
    Ok(())
}
