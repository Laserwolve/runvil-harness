//! `TestCall`: the context a Plugin crate's own test lends a call it makes
//! directly, whose files answer each request at once from the call's own
//! filesystem, at the request's first poll and not before.

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

/// A write of `text` to `path`.
fn write(path: &str, text: &str) -> VfsOp {
    VfsOp::Write {
        path: path.to_owned(),
        contents: text.as_bytes().to_vec(),
    }
}

/// A read of the first 64 bytes of `path`.
fn read(path: &str) -> VfsOp {
    VfsOp::ReadRange {
        path: path.to_owned(),
        offset: 0,
        len: 64,
    }
}

#[test]
fn a_test_calls_files_answer_each_request_at_once_from_the_calls_own_filesystem() {
    let call = TestCall::new(inspect());
    assert_eq!(
        asked(&call, write("/notes.md", "asked for")),
        Ok(VfsOutcome::Unit)
    );
    assert_eq!(
        asked(&call, read("/notes.md")),
        Ok(VfsOutcome::Bytes(b"asked for".to_vec())),
        "a later request reads what an earlier one wrote"
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
fn a_request_that_is_dropped_unpolled_changes_no_file() {
    let call = TestCall::new(inspect());
    // The write is built and dropped, and nothing polls it in between.
    drop(
        call.context()
            .files()
            .request(write("/notes.md", "never asked for")),
    );
    let exists = VfsOp::Exists {
        path: "/notes.md".to_owned(),
    };
    assert_eq!(
        asked(&call, exists),
        Ok(VfsOutcome::Bool(false)),
        "a request that was never polled was never performed"
    );
}

#[test]
fn a_test_call_lends_its_tool_a_script_origin_and_a_working_filesystem() {
    let call = TestCall::new(inspect());
    let cx = call.context();
    assert_eq!(cx.tool(), &inspect());
    assert_eq!(cx.origin().caller, ToolCaller::Script);
    // A test seeds a file before the call and reads what the call left
    // after it, each through a context of its own.
    assert_eq!(
        asked(&call, write("notes.md", "hello")),
        Ok(VfsOutcome::Unit)
    );
    assert_eq!(
        asked(&call, read("notes.md")),
        Ok(VfsOutcome::Bytes(b"hello".to_vec())),
        "every context of one call lends the same files"
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
    assert_eq!(asked(&first, write("left.txt", "1")), Ok(VfsOutcome::Unit));
    let second = TestCall::new(inspect());
    let exists = VfsOp::Exists {
        path: "left.txt".to_owned(),
    };
    assert_eq!(
        asked(&second, exists),
        Ok(VfsOutcome::Bool(false)),
        "a second call starts from an empty filesystem"
    );
}
