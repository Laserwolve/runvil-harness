//! `TestCall`: the context a Plugin crate's own test lends a call it makes
//! directly, whose files answer each request at once from the call's own
//! filesystem.

use std::task::{Context, Poll, Waker};

use runvil_plugin::testing::TestCall;
use runvil_plugin::{ToolCaller, ToolId, VfsError, VfsOp, VfsOutcome};

fn inspect() -> ToolId {
    ToolId::parse("acme/inspect").expect("a two-segment tool id parses")
}

/// The outcome of `op`, asked for through `call`'s files and taken at the
/// request's first poll.
fn asked(call: &TestCall, op: VfsOp) -> Result<VfsOutcome, VfsError> {
    let mut request = call.context().files().request(op);
    match request
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(outcome) => outcome,
        Poll::Pending => panic!("a test call answers each request at once"),
    }
}

#[test]
fn a_test_calls_files_answer_each_request_at_once_from_the_calls_own_filesystem() {
    let call = TestCall::new(inspect());
    let write = VfsOp::Write {
        path: "/notes.md".to_owned(),
        contents: b"asked for".to_vec(),
    };
    assert_eq!(asked(&call, write), Ok(VfsOutcome::Unit));
    assert_eq!(
        call.context()
            .access()
            .read("/notes.md")
            .expect("the access reads what the files wrote"),
        b"asked for"
    );

    call.context()
        .access()
        .write("/seeded.md", b"seeded")
        .expect("the in-memory filesystem accepts a write");
    let read = VfsOp::ReadRange {
        path: "/seeded.md".to_owned(),
        offset: 0,
        len: 64,
    };
    assert_eq!(
        asked(&call, read),
        Ok(VfsOutcome::Bytes(b"seeded".to_vec()))
    );

    let missing = VfsOp::Remove {
        path: "/gone.md".to_owned(),
        recursive: false,
    };
    assert_eq!(
        asked(&call, missing),
        Err(VfsError::NotFound {
            path: "/gone.md".to_owned(),
        })
    );
}

#[test]
fn a_test_call_lends_its_tool_a_script_origin_and_a_working_filesystem() {
    let call = TestCall::new(inspect());
    let cx = call.context();
    assert_eq!(cx.tool(), &inspect());
    assert_eq!(cx.origin().caller, ToolCaller::Script);
    cx.access()
        .write("notes.md", b"hello")
        .expect("the in-memory filesystem accepts a write");
    assert_eq!(
        cx.access()
            .read("notes.md")
            .expect("the written file reads back"),
        b"hello"
    );
}

#[test]
fn a_test_call_with_a_caller_lends_that_caller() {
    let call = TestCall::new(inspect()).with_caller(ToolCaller::Model);
    let cx = call.context();
    assert_eq!(cx.tool(), &inspect());
    assert_eq!(cx.origin().caller, ToolCaller::Model);
    assert_eq!(cx.origin().section, "Test");
}

#[test]
fn each_test_call_has_a_filesystem_of_its_own() {
    let first = TestCall::new(inspect());
    first
        .context()
        .access()
        .write("left.txt", b"1")
        .expect("the in-memory filesystem accepts a write");
    let second = TestCall::new(inspect());
    assert!(
        !second
            .context()
            .access()
            .exists("left.txt")
            .expect("an existence check succeeds"),
        "a second call starts from an empty filesystem"
    );
}
