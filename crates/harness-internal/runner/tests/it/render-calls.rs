//! The rendered tools' calls: each call's events name the vendor of its
//! tool id's Pack, a model-only tool rendered into the Dynamic Set runs
//! through `CallDynamicTool`, and a stop spares a held call only when the
//! render of its entry marks the tool to survive stops.

use super::*;
use crate::harness::{run_beside, until};

#[tokio::test]
async fn each_calls_events_name_the_vendor_of_its_tool_ids_pack_and_an_adhoc_call_none() {
    let seen = Seen::new();
    let broker = calling(vec![(
        "call every kind",
        vec![
            call("c1", "shout_m", json!({ "value": "a" })),
            call("c2", "secret_m", json!({ "value": "b" })),
            call("c3", "grab", json!({})),
            call("c4", "GetDynamicTools", json!({})),
        ],
    )]);
    let source = prompt(&[(
        "Only",
        "tools.offer('render/echo')\n\
         tools.offer(tools.adhoc({ name = 'grab', description = 'Grab a value' }, \
         function() return 'grabbed' end))\n\
         tools.offer_dynamic(tools.adhoc({ name = 'later', description = 'A later tool' }, \
         function() return 'later' end))\n\
         messages:user('call every kind')\nmodels.loop()\nreturn 'done'",
    )]);
    let (_, records) = run(&source, broker, &seen, Value::Null).await;
    let named = |id: &str, call: &Value| {
        (
            call[id].clone(),
            call["tool"].clone(),
            call["vendor"].clone(),
        )
    };
    let expected = [
        (json!("c1"), json!(ECHO), json!("tests")),
        (json!("c2"), json!(SECRET), json!("tests")),
        (json!("c3"), json!("adhoc/grab"), Value::Null),
        (json!("c4"), json!("dynamic/search"), json!("runvil")),
    ];
    let requested: Vec<(Value, Value, Value)> = events(&records, "assistant_tool_calls")
        .iter()
        .flat_map(|event| event["calls"].as_array().cloned().unwrap_or_default())
        .map(|call| named("id", &call))
        .collect();
    assert_eq!(
        requested, expected,
        "the renamed and the model-only call name the render Pack's vendor, the adhoc call \
         none, and the discovery call runvil"
    );
    let mut results: Vec<(Value, Value, Value)> = events(&records, "tool_result")
        .iter()
        .map(|event| named("tool_call_id", event))
        .collect();
    results.sort_by_key(|(id, _, _)| id.as_str().map(str::to_owned));
    assert_eq!(results, expected, "each result names the same vendor");
}

#[tokio::test]
async fn call_dynamic_tool_runs_a_model_only_tool_the_render_put_in_the_dynamic_set() {
    let seen = Seen::new();
    let broker = calling(vec![(
        "call it dynamically",
        vec![call(
            "c1",
            "CallDynamicTool",
            json!({ "toolName": "secret_m", "arguments": { "value": "x" } }),
        )],
    )]);
    let source = prompt(&[(
        "Only",
        "tools.offer_dynamic('render/echo')\nmessages:user('call it dynamically')\n\
         models.loop()\nreturn sys.id .. ' ' .. messages[#messages - 1].content",
    )]);
    let (text, records) = run(&source, broker, &seen, Value::Null).await;
    let (entry, result) = text.split_once(' ').expect("the run returns its entry");
    assert_eq!(
        result, "render/secret x on m",
        "the model reads the model-only tool's answer"
    );
    assert_eq!(seen.calls(), [Called::new(SECRET, entry, MODEL)]);
    let sent: Vec<Value> = answered(&records, "Chat")
        .into_iter()
        .map(|(effect, _)| effect["tools"].clone())
        .collect();
    assert_eq!(
        sent,
        vec![json!(["GetDynamicTools", "CallDynamicTool"]); 2],
        "the rendered tools sit in the Dynamic Set, behind the discovery tools"
    );
    let issued: Vec<(Value, Value)> = answered(&records, "ToolCall")
        .into_iter()
        .map(|(effect, _)| (effect["tool"].clone(), effect["alias"].clone()))
        .collect();
    assert_eq!(
        issued,
        [(json!(SECRET), json!("secret_m"))],
        "the call reaches the Harness under the model-only id and its rendered wire name"
    );
}

/// A section that offers the render Pack's tool, asks for a held call of
/// the model-only tool, and reports how its loop ended.
const HOLDS_THE_SECRET: [(&str, &str); 1] = [(
    "Only",
    "tools.offer('render/echo')\nmessages:user('hold it')\n\
     local ok, err = pcall(models.loop)\n\
     return tostring(ok) .. ':' .. (ok and messages[#messages].content or err.kind)",
)];

#[tokio::test]
async fn a_stop_spares_a_held_call_only_when_its_entrys_render_marks_the_tool_to_survive() {
    // One run for each render: the first marks the model-only tool to
    // survive stops, and the second leaves it unmarked.
    for surviving in [true, false] {
        let seen = Seen::new();
        let recorder = Arc::new(MemoryRecorder::new());
        let broker = calling(vec![(
            "hold it",
            vec![call(
                "c1",
                "secret_m",
                json!({ "value": "x", "hold": true }),
            )],
        )]);
        let marked: &[&str] = if surviving { &[MODEL] } else { &[] };
        let harness = render_harness(
            &recorder,
            broker,
            &seen,
            json!({ "surviving": marked }),
            &prompt(&HOLDS_THE_SECRET),
        );
        let watched = Arc::clone(&seen);
        let report = run_beside(harness, |control| async move {
            until("the held call is in flight", || watched.calls().len() == 1).await;
            control.stop_round();
            if surviving {
                for _ in 0..16 {
                    tokio::task::yield_now().await;
                }
                assert!(
                    watched.dropped().is_empty(),
                    "the stop left in flight the call whose rendered tool survives stops"
                );
                watched.open();
            } else {
                until("the stop drops the call", || !watched.dropped().is_empty()).await;
            }
        })
        .await
        .expect("the run is reported");

        let (ended, outcome, dropped): (&str, &str, &[&str]) = if surviving {
            ("true:done", "render/secret x on m", &[])
        } else {
            ("false:canceled", "Dropped", &[MODEL])
        };
        assert_eq!(
            report.result,
            Ok(ended.to_owned()),
            "the spared call answers and its loop goes on, and the dropped call's loop raises"
        );
        assert_eq!(
            seen.dropped(),
            dropped,
            "the stop drops only the call whose rendered tool does not survive stops"
        );
        let records = recorder.records(report.run_id);
        let rendered: Vec<Value> = answered(&records, "Render")
            .into_iter()
            .map(|(_, answer)| answer["Render"][0]["static_set"].clone())
            .collect();
        assert_eq!(
            rendered,
            [serde_json::to_value(shown(vec![echo()], MODEL, surviving)).unwrap()],
            "the render marks the model-only tool to survive stops only for a model that \
             the Pack's configuration lists"
        );
        let calls: Vec<(Value, String)> = answered(&records, "ToolCall")
            .into_iter()
            .map(|(effect, answer)| (effect["origin"]["model"].clone(), outcome_of(&answer)))
            .collect();
        assert_eq!(calls, [(json!(MODEL), outcome.to_owned())]);
    }
}
