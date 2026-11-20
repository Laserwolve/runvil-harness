//! Tests for a Host's resources: which names `provide` takes and what it
//! refuses, the JSON text of the resources as they are at each reading,
//! that a clone shares the functions, and that `Debug` shows no value.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use runvil_plugin::{Error, ErrorCode, ErrorFact};
use serde_json::{Value, json};

use super::HostResources;

/// A plain value may cross threads and be shared, as a `Plugin` that
/// keeps one is.
const _: () = {
    const fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<HostResources>();
};

/// Asserts `error` refuses the resource `name` for `reason`, with its
/// message as its prose.
#[track_caller]
fn assert_refused(error: &Error, reason: &str, name: &str) {
    assert_eq!(error.code, ErrorCode::InvalidValue, "{error:?}");
    assert_eq!(
        error.facts.get(&ErrorFact::Reason),
        Some(&Value::from(reason)),
        "{error:?}"
    );
    assert_eq!(
        error.facts.get(&ErrorFact::Name),
        Some(&Value::from(name)),
        "{error:?}"
    );
    assert_eq!(error.prose, error.message, "{error:?}");
}

#[test]
fn resources_that_offer_nothing_read_as_an_empty_object() {
    assert_eq!(HostResources::new().text(), "{}");
    assert_eq!(HostResources::default().text(), "{}");
}

#[test]
fn each_resource_is_a_key_of_the_object_and_the_names_come_in_order() {
    let mut resources = HostResources::new();
    resources
        .provide("runvil/web-search", || Some(json!({ "version": 1 })))
        .expect("a vendor and a name are accepted");
    resources
        .provide("acme/limit", || Some(json!(3)))
        .expect("a second name is accepted");
    resources
        .provide("acme.corp/note_2", || Some(json!("two words")))
        .expect("each segment may hold a dot, a digit, and an underscore");
    assert_eq!(
        resources.text(),
        r#"{"acme.corp/note_2":"two words","acme/limit":3,"runvil/web-search":{"version":1}}"#
    );
}

#[test]
fn a_resource_whose_function_returns_none_is_left_out() {
    let mut resources = HostResources::new();
    resources
        .provide("acme/absent", || None)
        .expect("the name is accepted");
    assert_eq!(resources.text(), "{}");
    resources
        .provide("acme/present", || Some(Value::Null))
        .expect("the name is accepted");
    assert_eq!(
        resources.text(),
        r#"{"acme/present":null}"#,
        "a JSON null is a value, and it stays"
    );
}

#[test]
fn a_value_that_changes_between_two_readings_changes_the_text() {
    let reads = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&reads);
    let mut resources = HostResources::new();
    resources
        .provide("acme/count", move || {
            // Gone at every third reading.
            let read = counted.fetch_add(1, Ordering::SeqCst) + 1;
            (!read.is_multiple_of(3)).then(|| json!(read))
        })
        .expect("the name is accepted");
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "nothing is read before a text is asked for"
    );
    assert_eq!(resources.text(), r#"{"acme/count":1}"#);
    assert_eq!(resources.text(), r#"{"acme/count":2}"#);
    assert_eq!(resources.text(), "{}");
    assert_eq!(resources.text(), r#"{"acme/count":4}"#);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        4,
        "each text reads each resource once"
    );
}

#[test]
fn a_name_that_is_not_a_vendor_and_a_name_is_refused() {
    let long = format!("acme/{}", "a".repeat(65));
    for name in [
        "",
        "search",
        "acme/",
        "/search",
        "acme/web/search",
        "Acme/search",
        "acme/Search",
        "acme/web search",
        "acme/web\nsearch",
        long.as_str(),
    ] {
        let mut resources = HostResources::new();
        let error = resources
            .provide(name, || Some(json!(1)))
            .expect_err("a name outside the vendor/name grammar is refused");
        assert_refused(&error, "invalid_name", name);
        assert_eq!(
            error.to_string(),
            format!("resource name `{name}` is not a vendor/name pair")
        );
        assert_eq!(resources.text(), "{}", "{name}: nothing is offered");
    }
}

#[test]
fn a_name_of_two_segments_of_64_bytes_is_accepted() {
    let name = format!("{}/{}", "v".repeat(64), "n".repeat(64));
    let mut resources = HostResources::new();
    resources
        .provide(&name, || Some(json!(1)))
        .expect("each segment is within its bound");
    assert_eq!(resources.text(), format!(r#"{{"{name}":1}}"#));
}

#[test]
fn a_name_that_is_offered_already_is_refused_and_the_first_stays() {
    let mut resources = HostResources::new();
    resources
        .provide("acme/limit", || Some(json!("first")))
        .expect("the first is accepted");
    let error = resources
        .provide("acme/limit", || Some(json!("second")))
        .expect_err("a name that is offered already is refused");
    assert_refused(&error, "duplicate_name", "acme/limit");
    assert_eq!(
        error.to_string(),
        "resource `acme/limit` is already offered"
    );
    assert_eq!(resources.text(), r#"{"acme/limit":"first"}"#);
}

#[test]
fn a_clone_shares_the_functions_and_goes_its_own_way_afterward() {
    let reads = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&reads);
    let mut resources = HostResources::new();
    resources
        .provide("acme/count", move || {
            Some(json!(counted.fetch_add(1, Ordering::SeqCst) + 1))
        })
        .expect("the name is accepted");
    let mut clone = resources.clone();
    assert_eq!(resources.text(), r#"{"acme/count":1}"#);
    assert_eq!(
        clone.text(),
        r#"{"acme/count":2}"#,
        "the clone reads through the same function"
    );
    clone
        .provide("acme/extra", || Some(json!(true)))
        .expect("a clone takes a name of its own");
    assert_eq!(clone.text(), r#"{"acme/count":3,"acme/extra":true}"#);
    assert_eq!(
        resources.text(),
        r#"{"acme/count":4}"#,
        "what a clone is offered later is not offered the original"
    );
}

#[test]
fn debug_lists_the_names_and_reads_no_value() {
    let reads = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&reads);
    let mut resources = HostResources::new();
    resources
        .provide("runvil/web-search", move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Some(json!({ "api_key": "hunter2" }))
        })
        .expect("the name is accepted");
    resources
        .provide("acme/limit", || Some(json!(3)))
        .expect("the name is accepted");
    let shown = format!("{resources:?}");
    assert_eq!(
        shown,
        r#"HostResources { names: ["acme/limit", "runvil/web-search"] }"#
    );
    assert!(!shown.contains("hunter2"), "{shown}");
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "showing the names reads no resource"
    );
}
