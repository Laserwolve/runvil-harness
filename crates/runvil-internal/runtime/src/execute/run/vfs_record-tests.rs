//! The record forms of a file operation and of its outcome: each of the
//! operations records as one object keyed by its name and reads back
//! identical, the bytes of a write record as text when they are UTF-8 and
//! as hex otherwise, each outcome records whole and reads back identical,
//! a time before the Unix epoch keeps its place, and a form that names no
//! operation or outcome is refused.

use std::fmt;

use serde::de::DeserializeOwned;

use super::*;

/// An operation behind the record form [`serialize_op`] writes.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Op(#[serde(serialize_with = "serialize_op", deserialize_with = "deserialize_op")] VfsOp);

/// An operation's answer behind the record form [`serialize_answer`]
/// writes, with a failure as its text.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Answer(
    #[serde(
        serialize_with = "serialize_answer",
        deserialize_with = "deserialize_answer"
    )]
    Result<VfsOutcome, String>,
);

/// Asserts that `value` records as exactly `line`, and that `line` reads
/// back as `value`.
#[track_caller]
fn assert_records_as<T>(value: &T, line: &str)
where
    T: Serialize + DeserializeOwned + PartialEq + fmt::Debug,
{
    assert_eq!(
        serde_json::to_string(value).expect("a record form serializes"),
        line
    );
    let back: T = serde_json::from_str(line).expect("a record form reads back");
    assert_eq!(&back, value, "{line}");
}

/// The error of reading `line` as a `T`.
#[track_caller]
fn refusal<T: DeserializeOwned + fmt::Debug>(line: &str) -> String {
    serde_json::from_str::<T>(line)
        .expect_err("the line reads as no record form")
        .to_string()
}

/// The path the operations of the record cases name.
fn path() -> String {
    "a.md".to_owned()
}

#[test]
fn each_operation_of_a_store_call_records_as_one_object_keyed_by_its_name() {
    let cases = [
        (
            VfsOp::Write {
                path: path(),
                contents: b"hello".to_vec(),
            },
            r#"{"Write":{"path":"a.md","contents":"hello"}}"#,
        ),
        (
            VfsOp::Append {
                path: path(),
                contents: b"more".to_vec(),
            },
            r#"{"Append":{"path":"a.md","contents":"more"}}"#,
        ),
        (
            VfsOp::Read {
                path: path(),
                start: Some(2),
                end: None,
            },
            r#"{"Read":{"path":"a.md","start":2,"end":null}}"#,
        ),
        (
            VfsOp::ReadNumbered {
                path: path(),
                start: None,
                end: None,
            },
            r#"{"ReadNumbered":{"path":"a.md","start":null,"end":null}}"#,
        ),
        (
            VfsOp::StrReplace {
                path: path(),
                old: "one".to_owned(),
                new: "two".to_owned(),
            },
            r#"{"StrReplace":{"path":"a.md","old":"one","new":"two"}}"#,
        ),
        (
            VfsOp::Delete { path: path() },
            r#"{"Delete":{"path":"a.md"}}"#,
        ),
        (
            VfsOp::Glob {
                pattern: "*.md".to_owned(),
            },
            r#"{"Glob":{"pattern":"*.md"}}"#,
        ),
        (
            VfsOp::Exists { path: path() },
            r#"{"Exists":{"path":"a.md"}}"#,
        ),
    ];
    for (op, line) in cases {
        assert_records_as(&Op(op), line);
    }
}

#[test]
fn each_operation_only_a_tool_asks_for_records_as_one_object_keyed_by_its_name() {
    let cases = [
        (VfsOp::Stat { path: path() }, r#"{"Stat":{"path":"a.md"}}"#),
        (
            VfsOp::List {
                path: "docs".to_owned(),
            },
            r#"{"List":{"path":"docs"}}"#,
        ),
        (
            VfsOp::ReadRange {
                path: path(),
                offset: 4,
                len: u64::MAX,
            },
            r#"{"ReadRange":{"path":"a.md","offset":4,"len":18446744073709551615}}"#,
        ),
        (
            VfsOp::Mkdir {
                path: "docs".to_owned(),
                recursive: true,
            },
            r#"{"Mkdir":{"path":"docs","recursive":true}}"#,
        ),
        (
            VfsOp::Remove {
                path: "docs".to_owned(),
                recursive: false,
            },
            r#"{"Remove":{"path":"docs","recursive":false}}"#,
        ),
        (
            VfsOp::Rename {
                from: path(),
                to: "b.md".to_owned(),
            },
            r#"{"Rename":{"from":"a.md","to":"b.md"}}"#,
        ),
        (
            VfsOp::Copy {
                from: path(),
                to: "b.md".to_owned(),
            },
            r#"{"Copy":{"from":"a.md","to":"b.md"}}"#,
        ),
    ];
    for (op, line) in cases {
        assert_records_as(&Op(op), line);
    }
}

#[test]
fn the_bytes_of_a_write_record_as_text_when_they_are_utf8_and_as_hex_otherwise() {
    assert_records_as(
        &Op(VfsOp::Write {
            path: "note.md".to_owned(),
            contents: "caf\u{e9} \u{2713}".as_bytes().to_vec(),
        }),
        "{\"Write\":{\"path\":\"note.md\",\"contents\":\"caf\u{e9} \u{2713}\"}}",
    );
    assert_records_as(
        &Op(VfsOp::Write {
            path: "blob.bin".to_owned(),
            contents: vec![0xff, 0x00, 0x0a, b'a'],
        }),
        r#"{"Write":{"path":"blob.bin","contents_hex":"ff000a61"}}"#,
    );
    assert_records_as(
        &Op(VfsOp::Append {
            path: "blob.bin".to_owned(),
            contents: vec![0x80],
        }),
        r#"{"Append":{"path":"blob.bin","contents_hex":"80"}}"#,
    );
    assert_records_as(
        &Op(VfsOp::Write {
            path: "empty.md".to_owned(),
            contents: Vec::new(),
        }),
        r#"{"Write":{"path":"empty.md","contents":""}}"#,
    );
}

#[test]
fn a_form_that_names_no_operation_is_refused() {
    for (line, reason) in [
        (
            r#"{"Write":{"path":"a","contents_hex":"abc"}}"#,
            "two digits for each byte, got 3 digits",
        ),
        (
            r#"{"Write":{"path":"a","contents_hex":"zz"}}"#,
            "no hex digit",
        ),
        (
            "{\"Write\":{\"path\":\"a\",\"contents_hex\":\"\u{e9}\u{e9}\"}}",
            "no hex digit",
        ),
        (r#"{"Shred":{"path":"a"}}"#, "unknown variant"),
    ] {
        let refused = refusal::<Op>(line);
        assert!(refused.contains(reason), "{line}: {refused}");
    }
}

/// The metadata of a three-byte file, with no mode or times.
fn bare_stat() -> Stat {
    Stat::new(FileType::File, 3)
}

#[test]
fn every_outcome_records_whole() {
    let cases = [
        (VfsOutcome::Unit, r#"{"Ok":"Unit"}"#),
        (
            VfsOutcome::Text("the read".to_owned()),
            r#"{"Ok":{"Text":"the read"}}"#,
        ),
        (
            VfsOutcome::Paths(vec!["a.txt".to_owned(), "b.txt".to_owned()]),
            r#"{"Ok":{"Paths":["a.txt","b.txt"]}}"#,
        ),
        (VfsOutcome::Bool(true), r#"{"Ok":{"Bool":true}}"#),
        (
            VfsOutcome::Stat(bare_stat()),
            r#"{"Ok":{"Stat":{"file_type":"file","size":3,"mode":null,"modified":null,"created":null}}}"#,
        ),
        (
            VfsOutcome::Entries(vec![
                Entry::new("a.md", bare_stat()),
                Entry::new("sub", Stat::new(FileType::Directory, 0))
                    .with_description(Some("notes".to_owned())),
            ]),
            r#"{"Ok":{"Entries":[{"name":"a.md","stat":{"file_type":"file","size":3,"mode":null,"modified":null,"created":null},"description":null},{"name":"sub","stat":{"file_type":"directory","size":0,"mode":null,"modified":null,"created":null},"description":"notes"}]}}"#,
        ),
        (
            VfsOutcome::Bytes(b"abc".to_vec()),
            r#"{"Ok":{"Bytes":{"contents":"abc"}}}"#,
        ),
        (
            VfsOutcome::Bytes(vec![0xff, 0x00]),
            r#"{"Ok":{"Bytes":{"contents_hex":"ff00"}}}"#,
        ),
    ];
    for (outcome, line) in cases {
        assert_records_as(&Answer(Ok(outcome)), line);
    }
    assert_records_as(&Answer(Err("gone".to_owned())), r#"{"Err":"gone"}"#);
}

#[test]
fn a_stat_records_its_mode_and_its_times_on_either_side_of_the_epoch() {
    let stat = Stat::new(FileType::Symlink, 9)
        .with_mode(Some(0o755))
        .with_modified(Some(UNIX_EPOCH + Duration::from_millis(1500)))
        .with_created(Some(UNIX_EPOCH - Duration::from_millis(1500)));
    assert_records_as(
        &Answer(Ok(VfsOutcome::Stat(stat))),
        r#"{"Ok":{"Stat":{"file_type":"symlink","size":9,"mode":493,"modified":{"secs":1,"nanos":500000000},"created":{"secs":-2,"nanos":500000000}}}}"#,
    );
    let whole = Stat::new(FileType::File, 0)
        .with_modified(Some(UNIX_EPOCH))
        .with_created(Some(UNIX_EPOCH - Duration::from_secs(3)));
    assert_records_as(
        &Answer(Ok(VfsOutcome::Stat(whole))),
        r#"{"Ok":{"Stat":{"file_type":"file","size":0,"mode":null,"modified":{"secs":0,"nanos":0},"created":{"secs":-3,"nanos":0}}}}"#,
    );
}

#[test]
fn every_kind_of_file_records_under_its_own_name() {
    for (file_type, name) in [
        (FileType::File, "file"),
        (FileType::Directory, "directory"),
        (FileType::Symlink, "symlink"),
        (FileType::Fifo, "fifo"),
        (FileType::Socket, "socket"),
        (FileType::CharDevice, "char_device"),
        (FileType::BlockDevice, "block_device"),
    ] {
        let line = format!(
            r#"{{"Ok":{{"Stat":{{"file_type":"{name}","size":0,"mode":null,"modified":null,"created":null}}}}}}"#
        );
        assert_records_as(
            &Answer(Ok(VfsOutcome::Stat(Stat::new(file_type, 0)))),
            &line,
        );
    }
}

#[test]
fn a_form_that_names_no_outcome_is_refused() {
    for (line, reason) in [
        (
            r#"{"Ok":{"Stat":{"file_type":"other","size":0,"mode":null,"modified":null,"created":null}}}"#,
            "names no kind of file",
        ),
        (
            r#"{"Ok":{"Bytes":{"contents_hex":"f"}}}"#,
            "two digits for each byte, got 1 digits",
        ),
        (r#"{"Ok":{"Lines":3}}"#, "unknown variant"),
    ] {
        let refused = refusal::<Answer>(line);
        assert!(refused.contains(reason), "{line}: {refused}");
    }
}
