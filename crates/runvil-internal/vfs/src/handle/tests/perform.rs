//! Tests for `Access::perform`: each operation of the list runs as the
//! method it names and returns that method's outcome, a text read applies
//! its line bounds, `Delete` and `Remove` differ on an absent path, and a
//! store view reports an operation's error in the caller's form.

use super::*;
use crate::{FileType, Stat, Store, VfsOp, VfsOutcome};

/// The path every single-file case works on.
const FILE: &str = "/docs/a.md";

/// A write of `text` to `path`.
fn write(path: &str, text: &str) -> VfsOp {
    VfsOp::Write {
        path: path.to_owned(),
        contents: text.as_bytes().to_vec(),
    }
}

/// A read of `path` under the line bounds `start` and `end`.
fn read(path: &str, start: Option<i64>, end: Option<i64>) -> VfsOp {
    VfsOp::Read {
        path: path.to_owned(),
        start,
        end,
    }
}

/// A numbered read of `path` under the line bounds `start` and `end`.
fn read_numbered(path: &str, start: Option<i64>, end: Option<i64>) -> VfsOp {
    VfsOp::ReadNumbered {
        path: path.to_owned(),
        start,
        end,
    }
}

/// The text `op`, a read, returns through `access`.
fn text_of(access: &Access, op: &VfsOp) -> Result<String, VfsError> {
    match access.perform(op)? {
        VfsOutcome::Text(text) => Ok(text),
        other => panic!("a read returns text, got {other:?}"),
    }
}

/// The refusal of `op`, a read whose line bounds are invalid: its path
/// and its reason.
#[track_caller]
fn range_refusal(access: &Access, op: &VfsOp) -> (String, &'static str) {
    match access.perform(op) {
        Err(VfsError::InvalidRange { path, reason }) => (path, reason),
        other => panic!("expected an invalid range, got {other:?}"),
    }
}

#[test]
fn each_operation_runs_as_the_method_it_names_and_returns_its_outcome() -> Result<(), VfsError> {
    let access = access(&VfsRef::default())?;
    let done = Ok(VfsOutcome::Unit);
    let path = || FILE.to_owned();

    let mkdir = VfsOp::Mkdir {
        path: "/docs".to_owned(),
        recursive: false,
    };
    assert_eq!(access.perform(&mkdir), done);
    assert_eq!(access.perform(&write(FILE, "one\ntwo")), done);
    let append = VfsOp::Append {
        path: path(),
        contents: b"\nthree".to_vec(),
    };
    assert_eq!(access.perform(&append), done);
    assert_eq!(
        access.perform(&read(FILE, None, None)),
        Ok(VfsOutcome::Text("one\ntwo\nthree".to_owned()))
    );
    assert_eq!(
        access.perform(&read_numbered(FILE, Some(2), Some(3))),
        Ok(VfsOutcome::Text("2| two\n3| three".to_owned()))
    );
    let replace = VfsOp::StrReplace {
        path: path(),
        old: "two".to_owned(),
        new: "TWO".to_owned(),
    };
    assert_eq!(access.perform(&replace), done);
    assert_eq!(
        access.perform(&VfsOp::Exists { path: path() }),
        Ok(VfsOutcome::Bool(true))
    );
    assert_eq!(
        access.perform(&VfsOp::Stat { path: path() }),
        Ok(VfsOutcome::Stat(Stat::new(FileType::File, 13)))
    );
    let listed = access.perform(&VfsOp::List {
        path: "/docs".to_owned(),
    })?;
    let VfsOutcome::Entries(entries) = listed else {
        panic!("a listing returns entries, got {listed:?}");
    };
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["a.md"]);
    let range = VfsOp::ReadRange {
        path: path(),
        offset: 4,
        len: 3,
    };
    assert_eq!(
        access.perform(&range),
        Ok(VfsOutcome::Bytes(b"TWO".to_vec()))
    );
    let glob = VfsOp::Glob {
        pattern: "/docs/*.md".to_owned(),
    };
    assert_eq!(access.perform(&glob), Ok(VfsOutcome::Paths(vec![path()])));

    let copy = VfsOp::Copy {
        from: path(),
        to: "/docs/b.md".to_owned(),
    };
    assert_eq!(access.perform(&copy), done);
    let rename = VfsOp::Rename {
        from: "/docs/b.md".to_owned(),
        to: "/docs/c.md".to_owned(),
    };
    assert_eq!(access.perform(&rename), done);
    assert_eq!(access.glob("/docs/*")?, [FILE, "/docs/c.md"]);
    let remove = VfsOp::Remove {
        path: "/docs/c.md".to_owned(),
        recursive: false,
    };
    assert_eq!(access.perform(&remove), done);
    assert_eq!(access.perform(&VfsOp::Delete { path: path() }), done);
    assert_eq!(access.glob("/docs/*")?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_write_and_a_byte_read_hold_bytes_that_are_not_text() -> Result<(), VfsError> {
    let access = access(&VfsRef::default())?;
    let bytes = vec![0xff, 0x00, b'a'];
    let write = VfsOp::Write {
        path: "/blob.bin".to_owned(),
        contents: bytes.clone(),
    };
    access.perform(&write)?;
    let whole = VfsOp::ReadRange {
        path: "/blob.bin".to_owned(),
        offset: 0,
        len: u64::MAX,
    };
    assert_eq!(access.perform(&whole), Ok(VfsOutcome::Bytes(bytes)));
    // A text read of the same file is refused, at the first byte that is
    // not UTF-8.
    assert_eq!(
        access.perform(&read("/blob.bin", None, None)),
        Err(VfsError::NotUtf8 {
            path: "/blob.bin".to_owned(),
            valid_up_to: 0,
        })
    );
    Ok(())
}

#[test]
fn delete_succeeds_on_an_absent_path_and_remove_fails_on_one() -> Result<(), VfsError> {
    let access = access(&VfsRef::default())?;
    let gone = || "/gone.md".to_owned();
    assert_eq!(
        access.perform(&VfsOp::Delete { path: gone() }),
        Ok(VfsOutcome::Unit)
    );
    for recursive in [false, true] {
        assert_eq!(
            access.perform(&VfsOp::Remove {
                path: gone(),
                recursive,
            }),
            Err(VfsError::NotFound { path: gone() }),
            "recursive: {recursive}"
        );
    }
    Ok(())
}

#[test]
fn remove_takes_a_directory_that_holds_entries_only_on_request() -> Result<(), VfsError> {
    let access = access(&VfsRef::default())?;
    access.write("/docs/a.md", b"kept")?;
    let remove = |recursive| VfsOp::Remove {
        path: "/docs".to_owned(),
        recursive,
    };
    assert_eq!(
        access.perform(&remove(false)),
        Err(VfsError::DirectoryNotEmpty {
            path: "/docs".to_owned(),
        })
    );
    assert_eq!(access.read("/docs/a.md")?, b"kept");
    assert_eq!(access.perform(&remove(true)), Ok(VfsOutcome::Unit));
    assert!(!access.exists("/docs")?);
    Ok(())
}

#[test]
fn a_read_takes_a_one_based_inclusive_range_of_lines() -> Result<(), VfsError> {
    let access = access(&VfsRef::default())?;
    access.write("/f.txt", b"a\nb\nc\nd\n")?;
    let lines = |start, end| text_of(&access, &read("/f.txt", start, end));
    assert_eq!(
        lines(None, None)?,
        "a\nb\nc\nd\n",
        "the whole file verbatim"
    );
    assert_eq!(lines(Some(2), Some(3))?, "b\nc");
    assert_eq!(lines(Some(3), None)?, "c\nd", "no end means the last line");
    assert_eq!(
        lines(Some(2), Some(99))?,
        "b\nc\nd",
        "an end past the last line is the last line"
    );
    assert_eq!(lines(Some(4), Some(4))?, "d");
    assert_eq!(
        lines(Some(5), None)?,
        "",
        "a start past the last line reads nothing"
    );
    Ok(())
}

#[test]
fn a_read_refuses_invalid_line_bounds_with_the_reason() -> Result<(), VfsError> {
    let access = access(&VfsRef::default())?;
    access.write("/f.txt", b"a\nb\nc")?;
    let refused = |start, end| range_refusal(&access, &read("/f.txt", start, end));
    let at = |reason| ("/f.txt".to_owned(), reason);
    assert_eq!(refused(Some(0), None), at("start must be at least 1"));
    assert_eq!(
        refused(Some(-3), Some(2)),
        at("start must be at least 1"),
        "a negative bound is refused as a zero bound is"
    );
    assert_eq!(
        refused(Some(3), Some(2)),
        at("end must not be before start")
    );
    assert_eq!(
        refused(Some(2), Some(-1)),
        at("end must not be before start")
    );
    assert_eq!(
        refused(None, Some(2)),
        at("start is required when end is given")
    );
    assert_eq!(
        range_refusal(&access, &read_numbered("/f.txt", None, Some(2))),
        at("start is required when end is given"),
        "a numbered read takes the same bounds"
    );
    Ok(())
}

#[test]
fn a_numbered_read_prefixes_each_line_with_its_number_in_the_file() -> Result<(), VfsError> {
    let access = access(&VfsRef::default())?;
    let lines: Vec<String> = (1..=11).map(|line| format!("line {line}")).collect();
    access.write("/f.txt", lines.join("\n").as_bytes())?;
    let numbered = |start, end| text_of(&access, &read_numbered("/f.txt", start, end));
    assert_eq!(
        numbered(Some(9), Some(11))?,
        " 9| line 9\n10| line 10\n11| line 11",
        "each number is right-aligned to the widest one"
    );
    assert_eq!(numbered(Some(2), Some(3))?, "2| line 2\n3| line 3");
    assert!(
        numbered(None, None)?.starts_with(" 1| line 1\n 2| line 2\n"),
        "no bounds numbers the whole file"
    );
    assert_eq!(numbered(Some(12), None)?, "");
    Ok(())
}

#[test]
fn a_store_view_reports_an_operations_error_in_the_callers_form() -> Result<(), VfsError> {
    let view = Store::new().view(&access(&VfsRef::default())?)?;
    assert_eq!(
        view.perform(&VfsOp::Remove {
            path: "gone.md".to_owned(),
            recursive: false,
        }),
        Err(VfsError::NotFound {
            path: "gone.md".to_owned(),
        })
    );
    assert_eq!(
        view.perform(&read("gone.md", Some(1), Some(2))),
        Err(VfsError::NotFound {
            path: "gone.md".to_owned(),
        })
    );
    view.perform(&write("notes.md", "one\ntwo"))?;
    assert_eq!(
        range_refusal(&view, &read("notes.md", Some(0), None)),
        ("notes.md".to_owned(), "start must be at least 1")
    );
    assert_eq!(text_of(&view, &read("notes.md", Some(2), None))?, "two");
    Ok(())
}
