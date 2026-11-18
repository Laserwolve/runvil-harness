//! The records of a file operation that a tool call asked for: the
//! effect's record names the call by its effect id where a `store` call's
//! names its store, the answer's record holds the receipt of the outcome
//! and never the content, and a receipt is the count of an outcome's
//! bytes, entries, or paths.

use runvil_vfs::{Entry, FileType, Stat};

use super::*;

/// A fresh access over an empty memory filesystem.
fn access() -> Arc<Access> {
    Arc::new(
        runvil_vfs::VfsRef::default()
            .acquire(runvil_vfs::Origin::new("effect record fixture"))
            .expect("the stock backend acquires"),
    )
}

#[test]
fn a_tools_file_effect_records_its_call_where_a_store_call_records_its_store() {
    let op = || VfsOp::Write {
        path: "note.md".to_owned(),
        contents: b"hello".to_vec(),
    };
    let asked_by_a_tool = Effect::Vfs {
        asker: VfsAsker::Tool { call: EffectId(7) },
        access: access(),
        op: op(),
    };
    assert_eq!(
        serde_json::to_string(&asked_by_a_tool.to_record()).expect("a record serializes"),
        r#"{"Vfs":{"call":7,"op":{"Write":{"path":"note.md","contents":"hello"}}}}"#
    );
    let asked_by_lua = Effect::Vfs {
        asker: VfsAsker::Lua {
            store: ChainId::root(),
        },
        access: access(),
        op: op(),
    };
    assert_eq!(
        serde_json::to_string(&asked_by_lua.to_record()).expect("a record serializes"),
        r#"{"Vfs":{"store":"0","op":{"Write":{"path":"note.md","contents":"hello"}}}}"#
    );
    for effect in [asked_by_a_tool, asked_by_lua] {
        let record = effect.to_record();
        let text = serde_json::to_string(&record).expect("a record serializes");
        let back: EffectRecord = serde_json::from_str(&text).expect("a record reads back");
        assert_eq!(back, record, "{text}");
    }
}

#[test]
fn a_vfs_record_that_names_no_asker_does_not_read_back() {
    let read = serde_json::from_str::<EffectRecord>(r#"{"Vfs":{"op":{"Delete":{"path":"a"}}}}"#);
    assert!(read.is_err(), "{read:?}");
}

#[test]
fn a_receipt_answer_records_the_receipt_and_never_the_content() {
    let stat = Stat::new(FileType::File, 3);
    let cases = [
        (VfsReceipt::Done, r#"{"VfsReceipt":{"Ok":"Done"}}"#),
        (
            VfsReceipt::Bytes(12),
            r#"{"VfsReceipt":{"Ok":{"Bytes":12}}}"#,
        ),
        (VfsReceipt::Count(3), r#"{"VfsReceipt":{"Ok":{"Count":3}}}"#),
        (
            VfsReceipt::Bool(true),
            r#"{"VfsReceipt":{"Ok":{"Bool":true}}}"#,
        ),
        (
            VfsReceipt::Stat(stat),
            r#"{"VfsReceipt":{"Ok":{"Stat":{"file_type":"file","size":3,"mode":null,"modified":null,"created":null}}}}"#,
        ),
    ];
    for (receipt, line) in cases {
        let record = EffectAnswer::VfsReceipt(Ok(receipt)).to_record();
        assert_eq!(
            serde_json::to_string(&record).expect("a receipt answer records"),
            line
        );
        assert_eq!(answer_round_trip(&record), record);
    }
}

#[test]
fn a_failed_receipt_answer_records_the_vfs_errors_tag_and_display_text() {
    let error = VfsError::NotFound {
        path: "/gone.md".to_owned(),
    };
    let record = EffectAnswer::VfsReceipt(Err(error)).to_record();
    assert_eq!(
        serde_json::to_string(&record).expect("a receipt answer records"),
        r#"{"VfsReceipt":{"Err":{"kind":"not_found","message":"not found: /gone.md"}}}"#
    );
    assert_eq!(answer_round_trip(&record), record);
}

#[test]
fn a_receipt_is_the_outcome_with_a_count_in_place_of_its_content() {
    let stat = Stat::new(FileType::Directory, 0);
    let entries = vec![
        Entry::new("a.md", Stat::new(FileType::File, 1)),
        Entry::new("b.md", Stat::new(FileType::File, 2)),
    ];
    let cases = [
        (VfsOutcome::Unit, VfsReceipt::Done),
        (
            VfsOutcome::Text("h\u{e9}llo".to_owned()),
            VfsReceipt::Bytes(6),
        ),
        (
            VfsOutcome::Bytes(vec![0xff, 0x00, 0x01]),
            VfsReceipt::Bytes(3),
        ),
        (
            VfsOutcome::Paths(vec!["a.md".to_owned(), "b.md".to_owned()]),
            VfsReceipt::Count(2),
        ),
        (VfsOutcome::Entries(entries), VfsReceipt::Count(2)),
        (VfsOutcome::Bool(false), VfsReceipt::Bool(false)),
        (VfsOutcome::Stat(stat.clone()), VfsReceipt::Stat(stat)),
    ];
    for (outcome, receipt) in cases {
        assert_eq!(VfsReceipt::of(&outcome), receipt, "{outcome:?}");
    }
}
