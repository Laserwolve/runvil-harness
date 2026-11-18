//! Tests for the file functions of a tool call's step: the operation each
//! takes as its step's request, what each is refused for as a misuse,
//! what an answer lends the step after a request, how a `VfsError` reads
//! to a Plugin, and whole calls that ask the files of a call.

use std::ffi::c_void;
use std::io;
use std::ptr;
use std::slice;
use std::sync::atomic::Ordering;

use runvil_plugin::testing::TestCall;
use runvil_plugin::{Entry, FileType, PathReason, Stat, VfsError, VfsOp, VfsOutcome};
use runvil_plugin_sys::{
    RV_FILE_ERROR_ALREADY_EXISTS, RV_FILE_ERROR_DENIED, RV_FILE_ERROR_IO, RV_FILE_ERROR_NOT_EMPTY,
    RV_FILE_TYPE_OTHER, RV_FILE_TYPE_SYMLINK,
};
use serde_json::json;

use super::{
    Answer, Wanted, describe, file_append, file_copy, file_list, file_mkdir, file_read,
    file_remove, file_rename, file_stat, file_write, raw_stat,
};
use crate::fakes::*;
use crate::session::{self, Verb};

/// The files every whole call below works on.
const FILES: &[(&str, &str)] = &[("/notes/a.md", "hello"), ("/notes/b.md", "")];

/// Runs `step` as one RV_TOOL_CALL step, and returns what it returned
/// beside the request the loader took and the misuse it recorded.
fn in_step<T>(step: impl FnOnce(*mut c_void) -> T) -> (T, Option<VfsOp>, Option<&'static str>) {
    let mut out = None;
    let (_status, session) = session::open(Verb::ToolCall, |ctx| {
        out = Some(step(ctx));
        RV_OK
    });
    (out.expect("the step ran"), session.request, session.misuse)
}

/// A path as the operations below hold it.
fn path(path: &str) -> String {
    path.to_owned()
}

#[test]
fn each_file_function_that_reads_takes_its_request_as_an_operation() {
    // SAFETY: the open step's `ctx` and a live path.
    let stat = in_step(|ctx| unsafe { file_stat(ctx, rv_str("/notes/a.md")) });
    let asked = VfsOp::Stat {
        path: path("/notes/a.md"),
    };
    assert_eq!(stat, (RV_OK, Some(asked), None));
    // SAFETY: as above.
    let list = in_step(|ctx| unsafe { file_list(ctx, rv_str("notes")) });
    let asked = VfsOp::List {
        path: path("notes"),
    };
    assert_eq!(list, (RV_OK, Some(asked), None));
    // SAFETY: as above.
    let read = in_step(|ctx| unsafe { file_read(ctx, rv_str("/notes/a.md"), 1, 3) });
    let asked = VfsOp::ReadRange {
        path: path("/notes/a.md"),
        offset: 1,
        len: 3,
    };
    assert_eq!(read, (RV_OK, Some(asked), None));
}

#[test]
fn each_file_function_that_changes_files_takes_its_request_as_an_operation() {
    let bytes = rv_bytes(b"text");
    // SAFETY: the open step's `ctx`, a live path, and live bytes.
    let write = in_step(|ctx| unsafe { file_write(ctx, rv_str("/a"), bytes) });
    let asked = VfsOp::Write {
        path: path("/a"),
        contents: b"text".to_vec(),
    };
    assert_eq!(write, (RV_OK, Some(asked), None));
    // SAFETY: as above.
    let append = in_step(|ctx| unsafe { file_append(ctx, rv_str("/a"), bytes) });
    let asked = VfsOp::Append {
        path: path("/a"),
        contents: b"text".to_vec(),
    };
    assert_eq!(append, (RV_OK, Some(asked), None));
    // SAFETY: the open step's `ctx` and a live path.
    let mkdir = in_step(|ctx| unsafe { file_mkdir(ctx, rv_str("/a"), 1) });
    let asked = VfsOp::Mkdir {
        path: path("/a"),
        recursive: true,
    };
    assert_eq!(mkdir, (RV_OK, Some(asked), None));
    // SAFETY: as above.
    let remove = in_step(|ctx| unsafe { file_remove(ctx, rv_str("/a"), 0) });
    let asked = VfsOp::Remove {
        path: path("/a"),
        recursive: false,
    };
    assert_eq!(remove, (RV_OK, Some(asked), None));
    // SAFETY: the open step's `ctx` and two live paths.
    let rename = in_step(|ctx| unsafe { file_rename(ctx, rv_str("/a"), rv_str("/b")) });
    let asked = VfsOp::Rename {
        from: path("/a"),
        to: path("/b"),
    };
    assert_eq!(rename, (RV_OK, Some(asked), None));
    // SAFETY: as above.
    let copy = in_step(|ctx| unsafe { file_copy(ctx, rv_str("/a"), rv_str("/b")) });
    let asked = VfsOp::Copy {
        from: path("/a"),
        to: path("/b"),
    };
    assert_eq!(copy, (RV_OK, Some(asked), None));
}

#[test]
fn a_read_asks_for_no_offset_and_no_length_past_what_a_file_can_have() {
    // SAFETY: the open step's `ctx` and a live path.
    let read = in_step(|ctx| unsafe { file_read(ctx, rv_str("/a"), u64::MAX, u64::MAX) });
    let asked = VfsOp::ReadRange {
        path: path("/a"),
        offset: i64::MAX.unsigned_abs(),
        len: u64::try_from(usize::MAX).unwrap_or(u64::MAX),
    };
    assert_eq!(read, (RV_OK, Some(asked), None));
}

#[test]
fn each_breach_of_the_bytes_of_a_write_is_a_recorded_misuse() {
    let byte = 0_u8;
    for (bytes, misuse) in [
        (
            // The length is refused before any byte is read.
            RvBytes {
                ptr: &raw const byte,
                len: 16_777_217,
            },
            "the run of bytes is longer than 16777216 bytes",
        ),
        (
            RvBytes {
                ptr: ptr::null(),
                len: 2,
            },
            "the run of bytes has a NULL pointer and a nonzero length",
        ),
    ] {
        // SAFETY: the function refuses the bytes before it reads through
        // them.
        let write = in_step(|ctx| unsafe { file_write(ctx, rv_str("/a"), bytes) });
        assert_eq!(write, (RV_ERR_MISUSE, None, Some(misuse)));
        // SAFETY: as above.
        let append = in_step(|ctx| unsafe { file_append(ctx, rv_str("/a"), bytes) });
        assert_eq!(append, (RV_ERR_MISUSE, None, Some(misuse)));
    }
    let none = RvBytes {
        ptr: ptr::null(),
        len: 0,
    };
    // SAFETY: a zero length reads no byte.
    let empty = in_step(|ctx| unsafe { file_write(ctx, rv_str("/a"), none) });
    let asked = VfsOp::Write {
        path: path("/a"),
        contents: Vec::new(),
    };
    assert_eq!(empty, (RV_OK, Some(asked), None));
}

#[test]
fn a_recursive_that_is_neither_0_nor_1_is_a_recorded_misuse() {
    let misuse = Some("recursive is neither 0 nor 1");
    // SAFETY: the open step's `ctx` and a live path.
    let mkdir = in_step(|ctx| unsafe { file_mkdir(ctx, rv_str("/a"), 2) });
    assert_eq!(mkdir, (RV_ERR_MISUSE, None, misuse));
    // SAFETY: as above.
    let remove = in_step(|ctx| unsafe { file_remove(ctx, rv_str("/a"), u32::MAX) });
    assert_eq!(remove, (RV_ERR_MISUSE, None, misuse));
}

#[test]
fn each_breach_of_either_path_of_a_move_or_a_copy_is_a_recorded_misuse() {
    let long = "a".repeat(4097);
    let bad = rv_str_bytes(&[0xff, 0xfe]);
    for (from, to, misuse) in [
        (
            rv_str(&long),
            rv_str("/b"),
            "the source path is longer than 4096 bytes",
        ),
        (
            rv_str("/a"),
            rv_str(&long),
            "the destination path is longer than 4096 bytes",
        ),
        (bad, rv_str("/b"), "the source path is not UTF-8"),
        (rv_str("/a"), bad, "the destination path is not UTF-8"),
        // The arguments are checked from left to right.
        (bad, rv_str(&long), "the source path is not UTF-8"),
    ] {
        // SAFETY: the open step's `ctx` and two paths that are live for
        // their lengths.
        let rename = in_step(|ctx| unsafe { file_rename(ctx, from, to) });
        assert_eq!(rename, (RV_ERR_MISUSE, None, Some(misuse)));
        // SAFETY: as above.
        let copy = in_step(|ctx| unsafe { file_copy(ctx, from, to) });
        assert_eq!(copy, (RV_ERR_MISUSE, None, Some(misuse)));
    }
}

#[test]
fn each_breach_of_a_path_is_a_recorded_misuse_and_takes_no_request() {
    let long = "a".repeat(4097);
    for (path, misuse) in [
        (rv_str(&long), "the path is longer than 4096 bytes"),
        (
            RvStr {
                ptr: ptr::null(),
                len: 2,
            },
            "the path has a NULL pointer and a nonzero length",
        ),
        (rv_str_bytes(&[0xff, 0xfe]), "the path is not UTF-8"),
    ] {
        // SAFETY: the function refuses the path before it reads through
        // it, or the path is live.
        let stat = in_step(|ctx| unsafe { file_stat(ctx, path) });
        assert_eq!(stat, (RV_ERR_MISUSE, None, Some(misuse)));
        // The path of a write is checked before its bytes.
        let bytes = RvBytes {
            ptr: ptr::null(),
            len: 2,
        };
        // SAFETY: as above, and the bytes are never reached.
        let write = in_step(|ctx| unsafe { file_write(ctx, path, bytes) });
        assert_eq!(write, (RV_ERR_MISUSE, None, Some(misuse)));
    }
}

#[test]
fn a_second_request_in_one_step_is_a_recorded_misuse_and_the_first_stays() {
    // SAFETY: the open step's `ctx` and live paths.
    let (statuses, request, misuse) = in_step(|ctx| unsafe {
        (
            file_stat(ctx, rv_str("/a")),
            file_read(ctx, rv_str("/b"), 0, 1),
        )
    });
    assert_eq!(statuses, (RV_OK, RV_ERR_MISUSE));
    assert_eq!(request, Some(VfsOp::Stat { path: path("/a") }));
    assert_eq!(misuse, Some("a step made a second file request"));
}

#[test]
fn a_request_and_finish_in_one_step_are_a_recorded_misuse_in_either_order() {
    let done = result(RV_RESULT_TRUSTED, rv_str("done"));
    // SAFETY: the open step's `ctx`, a live path, and a live result.
    let (statuses, request, misuse) = in_step(|ctx| unsafe {
        (
            crate::call::finish(ctx, &raw const done),
            file_stat(ctx, rv_str("/a")),
        )
    });
    assert_eq!(statuses, (RV_OK, RV_ERR_MISUSE));
    assert_eq!(request, None);
    assert_eq!(
        misuse,
        Some("a step made a file request after it called finish")
    );

    // SAFETY: as above.
    let (statuses, request, misuse) = in_step(|ctx| unsafe {
        (
            file_stat(ctx, rv_str("/a")),
            crate::call::finish(ctx, &raw const done),
        )
    });
    assert_eq!(statuses, (RV_OK, RV_ERR_MISUSE));
    assert_eq!(request, Some(VfsOp::Stat { path: path("/a") }));
    assert_eq!(
        misuse,
        Some("a step called finish after it made a file request")
    );
}

#[test]
fn a_step_that_may_not_ask_is_refused_before_its_arguments_are_read() {
    let done = result(RV_RESULT_TRUSTED, rv_str("done"));
    let null = RvStr {
        ptr: ptr::null(),
        len: 2,
    };
    // SAFETY: the open step's `ctx` and a live result, and the function
    // refuses the step before it reads the path.
    let (_statuses, request, misuse) = in_step(|ctx| unsafe {
        (
            crate::call::finish(ctx, &raw const done),
            file_stat(ctx, null),
        )
    });
    assert_eq!(request, None);
    assert_eq!(
        misuse,
        Some("a step made a file request after it called finish")
    );

    // SAFETY: the open step's `ctx` and a live path, and the second
    // function refuses the step before it reads its path.
    let (statuses, _request, misuse) =
        in_step(|ctx| unsafe { (file_stat(ctx, rv_str("/a")), file_stat(ctx, null)) });
    assert_eq!(statuses, (RV_OK, RV_ERR_MISUSE));
    assert_eq!(misuse, Some("a step made a second file request"));
}

#[test]
fn a_file_function_answers_only_the_open_step_through_its_own_ctx() {
    let path = rv_str("/notes/a.md");

    // SAFETY: a `ctx` that is not the open step's is refused before
    // anything is read through it.
    let wrong_ctx = in_step(|_ctx| unsafe { file_stat(ptr::null_mut(), path) });
    assert_eq!(
        wrong_ctx,
        (
            RV_ERR_MISUSE,
            None,
            Some("a tool call function was handed a ctx other than the open RV_TOOL_CALL step's")
        )
    );

    let (status, session) = session::open(Verb::PackNew, |ctx| {
        // SAFETY: a call that is not a step is refused before anything
        // is read.
        unsafe { file_stat(ctx, path) }
    });
    assert_eq!(
        (status, session.misuse),
        (
            RV_ERR_MISUSE,
            Some("a tool call function was called outside RV_TOOL_CALL")
        )
    );
    assert_eq!(session.request, None);

    // SAFETY: with no call open, the function reads nothing.
    let closed = session::closed(|| unsafe { file_stat(ptr::null_mut(), path) });
    assert_eq!(closed, RV_ERR_MISUSE);
}

#[test]
fn each_request_asks_for_the_one_field_its_file_function_names() {
    let path = || "/a".to_owned();
    let contents = || b"x".to_vec();
    for (op, wanted) in [
        (VfsOp::Stat { path: path() }, Wanted::Stat),
        (VfsOp::List { path: path() }, Wanted::Entries),
        (
            VfsOp::ReadRange {
                path: path(),
                offset: 2,
                len: 7,
            },
            Wanted::Bytes(7),
        ),
        (
            VfsOp::Write {
                path: path(),
                contents: contents(),
            },
            Wanted::Nothing,
        ),
        (
            VfsOp::Append {
                path: path(),
                contents: contents(),
            },
            Wanted::Nothing,
        ),
        (
            VfsOp::Mkdir {
                path: path(),
                recursive: true,
            },
            Wanted::Nothing,
        ),
        (
            VfsOp::Remove {
                path: path(),
                recursive: false,
            },
            Wanted::Nothing,
        ),
        (
            VfsOp::Rename {
                from: path(),
                to: path(),
            },
            Wanted::Nothing,
        ),
        (
            VfsOp::Copy {
                from: path(),
                to: path(),
            },
            Wanted::Nothing,
        ),
    ] {
        assert_eq!(Wanted::by(&op), wanted, "{op:?}");
    }
}

/// Whether `answer` holds no stat, no entries, and no bytes.
fn holds_nothing(answer: &RvFileAnswer) -> bool {
    (answer.stat.file_type, answer.stat.reserved, answer.stat.len) == (0, 0, 0)
        && answer.entries.ptr.is_null()
        && answer.entries.len == 0
        && answer.bytes.ptr.is_null()
        && answer.bytes.len == 0
}

/// Whether `answer` holds no error.
fn holds_no_error(answer: &RvFileAnswer) -> bool {
    (answer.error.kind, answer.error.reserved) == (0, 0)
        && answer.error.text.ptr.is_null()
        && answer.error.text.len == 0
}

#[test]
fn the_answer_to_a_stat_holds_the_stat_and_nothing_else() {
    let found = VfsOutcome::Stat(Stat::new(FileType::File, 5));
    let answer = Answer::to(Wanted::Stat, Ok(found));
    let raw = answer.raw();
    assert_eq!((raw.size, raw.status), (size_u32::<RvFileAnswer>(), RV_OK));
    assert_eq!(
        (raw.stat.file_type, raw.stat.reserved, raw.stat.len),
        (RV_FILE_TYPE_FILE, 0, 5)
    );
    assert!(raw.entries.ptr.is_null() && raw.entries.len == 0);
    assert!(raw.bytes.ptr.is_null() && raw.bytes.len == 0);
    assert!(holds_no_error(&raw));
}

#[test]
fn the_answer_to_a_listing_holds_each_entry_with_its_name() {
    let listed = VfsOutcome::Entries(vec![
        Entry::new("a.md", Stat::new(FileType::File, 5)),
        Entry::new("sub", Stat::new(FileType::Directory, 0)),
    ]);
    let answer = Answer::to(Wanted::Entries, Ok(listed));
    let raw = answer.raw();
    assert_eq!(raw.status, RV_OK);
    // SAFETY: the entries and their names are valid while `answer`
    // lives.
    let entries: Vec<(String, u32, u64)> =
        unsafe { slice::from_raw_parts(raw.entries.ptr, raw.entries.len) }
            .iter()
            .map(|entry| (text(entry.name), entry.stat.file_type, entry.stat.len))
            .collect();
    assert_eq!(
        entries,
        [
            ("a.md".to_owned(), RV_FILE_TYPE_FILE, 5),
            ("sub".to_owned(), RV_FILE_TYPE_DIRECTORY, 0),
        ]
    );
    assert_eq!(
        (raw.stat.file_type, raw.stat.reserved, raw.stat.len),
        (0, 0, 0)
    );
    assert!(raw.bytes.ptr.is_null() && raw.bytes.len == 0);
    assert!(holds_no_error(&raw));

    let empty = Answer::to(Wanted::Entries, Ok(VfsOutcome::Entries(Vec::new())));
    let raw = empty.raw();
    assert_eq!(raw.status, RV_OK);
    assert!(holds_nothing(&raw) && holds_no_error(&raw));
}

#[test]
fn the_answer_to_a_read_holds_at_most_the_bytes_the_request_asked_for() {
    let read = || Ok(VfsOutcome::Bytes(b"hello".to_vec()));
    for (most, held) in [(u64::MAX, &b"hello"[..]), (5, b"hello"), (3, b"hel")] {
        let answer = Answer::to(Wanted::Bytes(most), read());
        let raw = answer.raw();
        assert_eq!(raw.status, RV_OK);
        // SAFETY: the bytes are valid while `answer` lives.
        let bytes = unsafe { slice::from_raw_parts(raw.bytes.ptr, raw.bytes.len) };
        assert_eq!(bytes, held, "at most {most} bytes");
        assert!(raw.entries.ptr.is_null() && raw.entries.len == 0);
        assert!(holds_no_error(&raw));
    }
}

#[test]
fn the_answer_to_a_read_of_no_bytes_holds_nothing() {
    for answer in [
        Answer::to(Wanted::Bytes(9), Ok(VfsOutcome::Bytes(Vec::new()))),
        Answer::to(Wanted::Bytes(0), Ok(VfsOutcome::Bytes(b"hello".to_vec()))),
    ] {
        let raw = answer.raw();
        assert_eq!(raw.status, RV_OK);
        assert!(holds_nothing(&raw) && holds_no_error(&raw));
    }
}

#[test]
fn the_answer_to_a_failed_operation_holds_the_error_and_nothing_else() {
    let absent = VfsError::NotFound {
        path: "/absent".to_owned(),
    };
    for wanted in [
        Wanted::Stat,
        Wanted::Entries,
        Wanted::Bytes(9),
        Wanted::Nothing,
    ] {
        let answer = Answer::to(wanted, Err(absent.clone()));
        let raw = answer.raw();
        assert_eq!(raw.status, RV_ERR_FILE, "{wanted:?}");
        assert_eq!(
            (raw.error.kind, raw.error.reserved, text(raw.error.text)),
            (RV_FILE_ERROR_NOT_FOUND, 0, "not found: /absent".to_owned()),
            "{wanted:?}"
        );
        assert!(holds_nothing(&raw), "{wanted:?}");
    }
}

#[test]
fn the_answer_to_a_change_holds_a_status_alone() {
    let raw = Answer::to(Wanted::Nothing, Ok(VfsOutcome::Unit)).raw();
    assert_eq!((raw.size, raw.status), (size_u32::<RvFileAnswer>(), RV_OK));
    assert!(holds_nothing(&raw) && holds_no_error(&raw));
}

#[test]
fn an_outcome_of_another_kind_than_the_request_asked_for_is_a_failed_operation() {
    let stat = || VfsOutcome::Stat(Stat::new(FileType::File, 5));
    for (wanted, outcome) in [
        (Wanted::Stat, VfsOutcome::Unit),
        (Wanted::Entries, stat()),
        (Wanted::Bytes(9), VfsOutcome::Text("hello".to_owned())),
        (Wanted::Nothing, stat()),
    ] {
        let answer = Answer::to(wanted, Ok(outcome));
        let raw = answer.raw();
        assert_eq!(raw.status, RV_ERR_FILE, "{wanted:?}");
        assert_eq!(
            (raw.error.kind, text(raw.error.text)),
            (
                RV_FILE_ERROR_OTHER,
                "the file operation returned something other than what was asked for".to_owned()
            ),
            "{wanted:?}"
        );
        assert!(holds_nothing(&raw), "{wanted:?}");
    }
}

#[test]
fn each_kind_of_node_keeps_its_number_and_any_other_kind_is_other() {
    for (file_type, raw) in [
        (FileType::File, RV_FILE_TYPE_FILE),
        (FileType::Directory, RV_FILE_TYPE_DIRECTORY),
        (FileType::Symlink, RV_FILE_TYPE_SYMLINK),
        (FileType::Fifo, RV_FILE_TYPE_OTHER),
        (FileType::Socket, RV_FILE_TYPE_OTHER),
        (FileType::CharDevice, RV_FILE_TYPE_OTHER),
        (FileType::BlockDevice, RV_FILE_TYPE_OTHER),
    ] {
        let stat = raw_stat(&Stat::new(file_type, 7));
        assert_eq!(
            (stat.file_type, stat.reserved, stat.len),
            (raw, 0, 7),
            "{file_type:?}"
        );
    }
}

#[test]
fn each_error_the_contract_has_a_kind_for_reads_as_that_kind_and_a_text_a_model_can_read() {
    let path = || "/x".to_owned();
    for (error, kind, text) in [
        (
            VfsError::NotFound { path: path() },
            RV_FILE_ERROR_NOT_FOUND,
            "not found: /x",
        ),
        (
            VfsError::AlreadyExists { path: path() },
            RV_FILE_ERROR_ALREADY_EXISTS,
            "already exists: /x",
        ),
        (
            VfsError::NotADirectory { path: path() },
            RV_FILE_ERROR_NOT_A_DIRECTORY,
            "not a directory: /x",
        ),
        (
            VfsError::IsADirectory { path: path() },
            RV_FILE_ERROR_IS_A_DIRECTORY,
            "is a directory: /x",
        ),
        (
            VfsError::DirectoryNotEmpty { path: path() },
            RV_FILE_ERROR_NOT_EMPTY,
            "directory not empty: /x",
        ),
        (
            VfsError::PermissionDenied {
                path: path(),
                reason: "the mode allows only markdown".to_owned(),
            },
            RV_FILE_ERROR_DENIED,
            "the mode allows only markdown",
        ),
        (
            VfsError::ApprovalRequired {
                path: path(),
                reason: "writing /x needs approval".to_owned(),
            },
            RV_FILE_ERROR_DENIED,
            "writing /x needs approval",
        ),
        (
            VfsError::Io {
                path: path(),
                kind: io::ErrorKind::PermissionDenied,
                raw_os_error: Some(13),
                message: "access denied".to_owned(),
            },
            RV_FILE_ERROR_IO,
            "permission denied: access denied",
        ),
    ] {
        assert_eq!(describe(&error), (kind, text.to_owned()), "{error:?}");
    }
}

#[test]
fn each_error_the_contract_has_no_kind_for_reads_as_other_and_a_text_a_model_can_read() {
    let path = || "/x".to_owned();
    for (error, text) in [
        (
            // The files a tool call reaches check no conflict, so no
            // error reads as a conflict, one a backend makes included.
            VfsError::Conflict {
                path: path(),
                detail: "a write by another call".to_owned(),
            },
            "a write by another call",
        ),
        (
            VfsError::InvalidPath {
                path: String::new(),
                reason: PathReason::Empty,
            },
            "path is empty",
        ),
        (
            // The path holds the `": "` that ends a label.
            VfsError::InvalidPath {
                path: "a: b/../..".to_owned(),
                reason: PathReason::Traversal,
            },
            "path contains a traversal segment",
        ),
        (
            VfsError::NotUtf8 {
                path: path(),
                valid_up_to: 3,
            },
            "/x holds a name or contents that are not UTF-8",
        ),
        (
            VfsError::ScopeEnded { path: path() },
            "the run or tool call that owned this access has ended, so /x can no longer be \
             touched through it",
        ),
        (
            VfsError::ReadOnly { path: path() },
            "/x is served read-only and cannot change",
        ),
        (
            VfsError::Backend {
                message: "the backend is gone".to_owned(),
            },
            "the backend is gone",
        ),
    ] {
        assert_eq!(
            describe(&error),
            (RV_FILE_ERROR_OTHER, text.to_owned()),
            "{error:?}"
        );
    }
}

pack_fake!(
    reader,
    {},
    new: |call| {
        set_pack_run(call, instance(PACK_TAG));
        add_tool(call, "stat");
        add_tool(call, "list");
        add_tool(call, "read");
        add_tool(call, "tail");
        add_tool(call, "past")
    },
    step: |call| {
        let args = step_args(call);
        // The arguments are one JSON string, the path.
        let path: String = serde_json::from_str(&text(args.arguments)).unwrap_or_default();
        let tool = text(args.tool);
        if tool_call(call).is_null() {
            // The first step asks, and the second is lent the answer.
            set_tool_call(call, instance(0x300));
            let asked = match tool.as_str() {
                "stat" => ask_stat(call, &path),
                "list" => ask_list(call, &path),
                "read" => ask_read(call, &path, (0, u64::MAX)),
                "tail" => ask_read(call, &path, (3, 100)),
                _ => ask_read(call, &path, (u64::MAX, u64::MAX)),
            };
            return if asked == RV_OK { RV_PENDING } else { ODD_STATUS };
        }
        let said = match tool.as_str() {
            "stat" => format!("{:?}", stat_answer(call)),
            "list" => format!("{:?}", list_answer(call)),
            _ => format!("{:?}", read_answer(call)),
        };
        finish(call, RV_RESULT_TRUSTED, &said)
    }
);

/// What the `reader` Pack's tool `tool` says of `path` over [`FILES`].
fn said(tool: &str, path: &str) -> String {
    let pack = made(reader::entry);
    let call = seeded_call(tool, FILES);
    block_on(pack.call(call.context(), json!(path)))
        .expect("the call succeeds")
        .text()
        .to_owned()
}

/// The text a fake's helper formats for an answer that holds `held`.
fn held(held: &str) -> String {
    format!("Some(Ok({held}))")
}

/// The text a fake's helper formats for the answer of an operation that
/// failed as `kind`, saying `text`.
fn failed(kind: u32, text: &str) -> String {
    format!("Some(Err(({kind}, {text:?})))")
}

#[test]
fn a_stat_is_answered_with_what_the_path_names() {
    assert_eq!(
        said("stat", "/notes/a.md"),
        held(&format!("({RV_FILE_TYPE_FILE}, 5)"))
    );
    assert!(
        said("stat", "/notes").starts_with(&format!("Some(Ok(({RV_FILE_TYPE_DIRECTORY}, ")),
        "{}",
        said("stat", "/notes")
    );
}

#[test]
fn a_listing_is_answered_with_each_entry_its_name_and_its_metadata() {
    assert_eq!(
        said("list", "/notes"),
        held(&format!(
            "[(\"a.md\", {RV_FILE_TYPE_FILE}, 5), (\"b.md\", {RV_FILE_TYPE_FILE}, 0)]"
        ))
    );
}

#[test]
fn a_read_is_answered_with_the_whole_file_or_the_bytes_inside_a_range() {
    assert_eq!(
        said("read", "/notes/a.md"),
        held(&format!("{:?}", b"hello"))
    );
    assert_eq!(said("read", "/notes/b.md"), held("[]"));
    assert_eq!(said("tail", "/notes/a.md"), held(&format!("{:?}", b"lo")));
    assert_eq!(
        said("past", "/notes/a.md"),
        held("[]"),
        "the largest offset is past the end, not a failure"
    );
}

#[test]
fn a_path_without_a_leading_slash_is_taken_from_the_root() {
    assert_eq!(said("read", "notes/a.md"), held(&format!("{:?}", b"hello")));
}

#[test]
fn a_failed_operation_is_answered_with_its_error_and_leaves_the_call_running() {
    assert_eq!(
        said("stat", "/absent"),
        failed(RV_FILE_ERROR_NOT_FOUND, "not found: /absent")
    );
    assert_eq!(
        said("list", "/notes/a.md"),
        failed(
            RV_FILE_ERROR_NOT_A_DIRECTORY,
            "not a directory: /notes/a.md"
        )
    );
    assert_eq!(
        said("read", "/notes"),
        failed(RV_FILE_ERROR_IS_A_DIRECTORY, "is a directory: /notes")
    );
    assert_eq!(
        said("stat", ""),
        failed(RV_FILE_ERROR_OTHER, "path is empty")
    );
}

#[test]
fn a_path_of_exactly_4096_bytes_is_within_the_bound() {
    let longest = format!("/{}", "a".repeat(4095));
    assert_eq!(
        said("stat", &longest),
        failed(RV_FILE_ERROR_NOT_FOUND, &format!("not found: {longest}"))
    );
}

pack_fake!(
    changer,
    {},
    new: |call| {
        set_pack_run(call, instance(PACK_TAG));
        add_tool(call, "change")
    },
    step: |call| {
        // The arguments are a JSON array of three strings: the change,
        // a path, and a text to write or a second path.
        let (change, first, second): (String, String, String) =
            serde_json::from_str(&text(step_args(call).arguments)).unwrap_or_default();
        if tool_call(call).is_null() {
            // The first step asks, and the second is lent the answer.
            set_tool_call(call, instance(0x300));
            let asked = match change.as_str() {
                "write" => ask_write(call, &first, rv_bytes(second.as_bytes())),
                "append" => ask_append(call, &first, rv_bytes(second.as_bytes())),
                "mkdir" => ask_mkdir(call, &first, 0),
                "mkdirs" => ask_mkdir(call, &first, 1),
                "remove" => ask_remove(call, &first, 0),
                "purge" => ask_remove(call, &first, 1),
                "rename" => ask_rename(call, rv_str(&first), rv_str(&second)),
                _ => ask_copy(call, rv_str(&first), rv_str(&second)),
            };
            return if asked == RV_OK { RV_PENDING } else { ODD_STATUS };
        }
        finish(call, RV_RESULT_TRUSTED, &format!("{:?}", change_answer(call)))
    }
);

/// What the `changer` Pack says of the change `change`, with `first` and
/// `second` as its arguments, on the files of `call`.
fn change_said(call: &TestCall, change: &str, first: &str, second: &str) -> String {
    let pack = made(changer::entry);
    block_on(pack.call(call.context(), json!([change, first, second])))
        .expect("the call succeeds")
        .text()
        .to_owned()
}

#[test]
fn each_change_is_performed_on_the_files_of_the_call_and_answered_with_a_status_alone() {
    let call = seeded_call("change", FILES);
    let done = held("()");

    assert_eq!(change_said(&call, "write", "/out/new.md", "one"), done);
    assert_eq!(text_at(&call, "/out/new.md"), Ok("one".to_owned()));
    assert_eq!(change_said(&call, "append", "/out/new.md", " two"), done);
    assert_eq!(text_at(&call, "/out/new.md"), Ok("one two".to_owned()));
    assert_eq!(change_said(&call, "append", "/out/added.md", "made"), done);
    assert_eq!(text_at(&call, "/out/added.md"), Ok("made".to_owned()));
    assert_eq!(
        change_said(&call, "copy", "/out/new.md", "/deep/er/copy.md"),
        done
    );
    assert_eq!(text_at(&call, "/deep/er/copy.md"), Ok("one two".to_owned()));
    assert_eq!(
        change_said(&call, "rename", "/out/new.md", "/moved/new.md"),
        done
    );
    assert_eq!(text_at(&call, "/moved/new.md"), Ok("one two".to_owned()));
    assert_eq!(
        text_at(&call, "/out/new.md"),
        Err(VfsError::NotFound {
            path: "/out/new.md".to_owned()
        })
    );
    assert_eq!(change_said(&call, "mkdirs", "/made/in/depth", ""), done);
    assert_eq!(change_said(&call, "mkdir", "/made/in/depth/more", ""), done);
    assert_eq!(
        change_said(&call, "remove", "/made/in/depth/more", ""),
        done
    );
    assert_eq!(change_said(&call, "purge", "/notes", ""), done);
    assert_eq!(
        text_at(&call, "/notes/a.md"),
        Err(VfsError::NotFound {
            path: "/notes/a.md".to_owned()
        })
    );
}

#[test]
fn a_change_the_files_refuse_is_answered_with_its_kind_and_leaves_the_call_running() {
    let call = seeded_call("change", FILES);
    for (change, first, kind, text) in [
        // Something is at the path already, with `recursive` set too.
        (
            "mkdir",
            "/notes",
            RV_FILE_ERROR_ALREADY_EXISTS,
            "already exists: /notes",
        ),
        (
            "mkdirs",
            "/notes",
            RV_FILE_ERROR_ALREADY_EXISTS,
            "already exists: /notes",
        ),
        (
            "mkdirs",
            "/notes/a.md",
            RV_FILE_ERROR_ALREADY_EXISTS,
            "already exists: /notes/a.md",
        ),
        // A directory on the way is absent, and `recursive` is 0.
        (
            "mkdir",
            "/no/such",
            RV_FILE_ERROR_NOT_FOUND,
            "not found: /no/such",
        ),
        // Nothing is at the path.
        (
            "remove",
            "/absent",
            RV_FILE_ERROR_NOT_FOUND,
            "not found: /absent",
        ),
        (
            "purge",
            "/absent",
            RV_FILE_ERROR_NOT_FOUND,
            "not found: /absent",
        ),
        // The directory has entries, and `recursive` is 0.
        (
            "remove",
            "/notes",
            RV_FILE_ERROR_NOT_EMPTY,
            "directory not empty: /notes",
        ),
        (
            "write",
            "/notes",
            RV_FILE_ERROR_IS_A_DIRECTORY,
            "is a directory: /notes",
        ),
        (
            "rename",
            "/absent",
            RV_FILE_ERROR_NOT_FOUND,
            "not found: /absent",
        ),
        (
            "copy",
            "/absent",
            RV_FILE_ERROR_NOT_FOUND,
            "not found: /absent",
        ),
    ] {
        assert_eq!(
            change_said(&call, change, first, "/other"),
            failed(kind, text),
            "{change} {first}"
        );
    }
    assert_eq!(
        text_at(&call, "/notes/a.md"),
        Ok("hello".to_owned()),
        "a refused change left the files as they were"
    );
}

pack_fake!(
    twice,
    {},
    new: |call| {
        set_pack_run(call, instance(PACK_TAG));
        add_tool(call, "twice")
    },
    step: |call| {
        // The call's state counts its steps.
        let steps = tool_call(call).addr();
        set_tool_call(call, ptr::without_provenance_mut(steps + 1));
        match steps {
            0 => {
                ask_read(call, "/notes/a.md", (0, u64::MAX));
                RV_PENDING
            }
            1 => {
                // The step that is lent an answer may ask again.
                let first = read_answer(call);
                ask_stat(call, "/notes/b.md");
                if first == Some(Ok(b"hello".to_vec())) { RV_PENDING } else { ODD_STATUS }
            }
            _ => finish(call, RV_RESULT_TRUSTED, &format!("{:?}", stat_answer(call))),
        }
    }
);

#[test]
fn a_call_asks_in_one_step_after_another_and_each_next_step_is_lent_the_answer() {
    let pack = made(twice::entry);
    let call = seeded_call("twice", FILES);
    let output = block_on(pack.call(call.context(), json!({}))).expect("the call succeeds");
    assert_eq!(output.text(), held(&format!("({RV_FILE_TYPE_FILE}, 0)")));
}

/// The message of a call that failed because its Plugin misused it.
macro_rules! misuse_reason {
    ($detail:expr) => {
        concat!("the tool's Plugin misused the tool call: ", $detail)
    };
}

/// Defines a test whose fake step runs `$step`, which calls a file
/// function wrongly and returns the step's status. The call must fail as
/// a `ToolOther` error that names the misuse `$detail`.
macro_rules! file_misuse_test {
    ($test:ident, $detail:expr, |$call:ident| $step:expr) => {
        #[test]
        fn $test() {
            pack_fake!(
                plug,
                {},
                new: |call| {
                    set_pack_run(call, instance(PACK_TAG));
                    add_tool(call, "tool")
                },
                step: |$call| $step
            );
            let pack = made(plug::entry);
            let call = seeded_call("tool", FILES);
            let failure = block_on(pack.call(call.context(), json!({})))
                .expect_err("the call fails");
            assert_eq!(failure.code, runvil_plugin::ErrorCode::ToolOther);
            assert_eq!(failure.message, misuse_reason!($detail));
        }
    };
}

file_misuse_test!(
    a_path_over_4096_bytes_fails_the_call,
    "the path is longer than 4096 bytes",
    |call| {
        // The length is refused before any byte is read, so the text
        // behind it need not be that long.
        let long = RvStr {
            ptr: "x".as_ptr().cast(),
            len: 4097,
        };
        ask_stat_as(call, step_args(call).ctx, long)
    }
);

file_misuse_test!(
    a_file_function_through_another_ctx_fails_the_call,
    "a tool call function was handed a ctx other than the open RV_TOOL_CALL step's",
    |call| ask_stat_as(call, ptr::null_mut(), rv_str("/notes/a.md"))
);

file_misuse_test!(
    a_write_of_more_than_16_mebibytes_fails_the_call,
    "the run of bytes is longer than 16777216 bytes",
    |call| {
        // The length is refused before any byte is read, so the bytes
        // behind it need not be that many.
        let long = RvBytes {
            ptr: "x".as_ptr(),
            len: 16_777_217,
        };
        ask_write(call, "/notes/a.md", long)
    }
);

file_misuse_test!(
    a_recursive_of_2_fails_the_call,
    "recursive is neither 0 nor 1",
    |call| ask_remove(call, "/notes", 2)
);

#[test]
fn a_misuse_asks_the_files_of_the_call_for_nothing() {
    pack_fake!(
        plug,
        {},
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            // The write is a request the loader takes, and the second
            // request of the step is the misuse.
            ask_write(call, "/notes/a.md", rv_bytes(b"other"));
            ask_stat(call, "/notes/a.md");
            RV_PENDING
        }
    );
    let pack = made(plug::entry);
    let call = seeded_call("tool", FILES);
    let failure = block_on(pack.call(call.context(), json!({}))).expect_err("the call fails");
    assert_eq!(
        failure.message,
        misuse_reason!("a step made a second file request")
    );
    assert_eq!(
        text_at(&call, "/notes/a.md"),
        Ok("hello".to_owned()),
        "the request of a step that failed its call is not performed"
    );
}

#[test]
fn a_file_function_called_while_a_pack_instance_is_made_is_misuse() {
    pack_fake!(
        plug,
        {},
        new: |call| {
            // SAFETY: the function refuses a call that is not a step
            // before it reads the path.
            unsafe { crate::file::file_stat(pack_args(call).ctx, rv_str("/notes/a.md")) }
        },
        step: |_call| RV_OK
    );
    let failure = new_pack(plug::entry)
        .err()
        .expect("the instance is not made");
    assert_eq!(
        failure.message,
        "the Plugin fake misused the Pack functions: a tool call function was called outside \
         RV_TOOL_CALL"
    );
}

#[test]
fn a_late_file_function_through_a_stashed_ctx_is_misuse() {
    pack_fake!(
        plug,
        {
            pub(super) static STEP_CTX: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            STEP_CTX.store(step_args(call).ctx, Ordering::SeqCst);
            finish(call, RV_RESULT_TRUSTED, "done")
        }
    );
    let pack = made(plug::entry);
    let call = seeded_call("tool", FILES);
    block_on(pack.call(call.context(), json!({}))).expect("the call succeeds");
    let stashed = plug::STEP_CTX.load(Ordering::SeqCst);
    // SAFETY: the loader's function compares a `ctx` with the open call's
    // before it reads through it, and no call is open on this thread.
    let status = unsafe { file_stat(stashed, rv_str("/notes/a.md")) };
    assert_eq!(status, RV_ERR_MISUSE);
}
