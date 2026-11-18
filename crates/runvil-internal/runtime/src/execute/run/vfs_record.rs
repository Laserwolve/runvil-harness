//! The record forms of a file operation, of who asked for it, and of its
//! outcome.
//!
//! `runvil-vfs` is std only, so its [`VfsOp`] and [`VfsOutcome`] have no
//! serde form of their own. A `Vfs` effect's record and its answer's
//! record write them through the private forms here, and read them back.
//!
//! An operation records in serde's externally tagged form: one object
//! keyed by the operation's name, such as
//! `{"Read":{"path":"a.md","start":null,"end":null}}`. The bytes of a
//! write or an append record as text under `contents` when they are
//! UTF-8, and as lowercase hex under `contents_hex` otherwise. An outcome
//! records the same way: `Unit` is the bare string `"Unit"`, and every
//! other outcome is a one-key object, such as `{"Text":"..."}`,
//! `{"Paths":["a.txt"]}`, or `{"Bool":true}`.
//!
//! The asker records as one key beside the operation: `store`, the id of
//! the chain the store was made for, when the prompt's Lua asked, and
//! `call`, the id of the tool call's effect, when a tool call asked. The
//! outcome of an operation that a tool call asked for records as its
//! [`VfsReceipt`]: `"Done"`, or a one-key object such as `{"Bytes":12}`.
//!
//! [`VfsOp`] and [`VfsOutcome`] are closed enums, so every operation and
//! every outcome has a form here. The kind of a file is
//! `#[non_exhaustive]`: a kind this module does not name records as
//! `other`, which does not read back.

use std::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use runvil_types::ids::ChainId;
use runvil_vfs::{Entry, FileType, Stat, VfsOp, VfsOutcome};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::effect::{EffectId, VfsAsker, VfsReceipt};

/// Bytes as a record holds them: as text when they are UTF-8, and as hex
/// otherwise. Flattened into an operation's record, the variant's name
/// is the key the bytes sit under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum BytesRecord {
    /// UTF-8 bytes, as their text.
    #[serde(rename = "contents")]
    Text(String),
    /// Bytes that are not UTF-8, as two lowercase hex digits for each.
    #[serde(rename = "contents_hex")]
    Hex(String),
}

impl BytesRecord {
    /// The record form of `bytes`.
    fn of(bytes: Vec<u8>) -> BytesRecord {
        match String::from_utf8(bytes) {
            Ok(text) => BytesRecord::Text(text),
            Err(not_text) => {
                let bytes = not_text.as_bytes();
                let mut hex = String::with_capacity(bytes.len() * 2);
                for byte in bytes {
                    // Writing to a String is infallible, so the result is
                    // discarded.
                    let _ = write!(hex, "{byte:02x}");
                }
                BytesRecord::Hex(hex)
            }
        }
    }

    /// The bytes this record holds.
    ///
    /// # Errors
    /// Returns a sentence that names the fault when the hex holds a
    /// character that is no hex digit, or an odd number of digits.
    fn into_bytes(self) -> Result<Vec<u8>, String> {
        const NOT_HEX: &str = "recorded hex holds a character that is no hex digit";
        let hex = match self {
            BytesRecord::Text(text) => return Ok(text.into_bytes()),
            BytesRecord::Hex(hex) => hex,
        };
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(NOT_HEX.to_owned());
        }
        if !hex.len().is_multiple_of(2) {
            return Err(format!(
                "recorded hex must hold two digits for each byte, got {} digits",
                hex.len()
            ));
        }
        hex.as_bytes()
            .chunks(2)
            .map(|pair| {
                std::str::from_utf8(pair)
                    .ok()
                    .and_then(|digits| u8::from_str_radix(digits, 16).ok())
                    .ok_or_else(|| NOT_HEX.to_owned())
            })
            .collect()
    }
}

/// A [`VfsOp`] as a record holds it: the same operation, with the bytes
/// of a write or an append as a [`BytesRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum VfsOpRecord {
    /// `VfsOp::Write`.
    Write {
        /// The path of the file.
        path: String,
        /// The bytes the file holds afterward.
        #[serde(flatten)]
        contents: BytesRecord,
    },
    /// `VfsOp::Append`.
    Append {
        /// The path of the file.
        path: String,
        /// The bytes to add.
        #[serde(flatten)]
        contents: BytesRecord,
    },
    /// `VfsOp::Read`.
    Read {
        /// The path of the file.
        path: String,
        /// The optional 1-based first line.
        start: Option<i64>,
        /// The optional 1-based last line.
        end: Option<i64>,
    },
    /// `VfsOp::ReadNumbered`.
    ReadNumbered {
        /// The path of the file.
        path: String,
        /// The optional 1-based first line.
        start: Option<i64>,
        /// The optional 1-based last line.
        end: Option<i64>,
    },
    /// `VfsOp::StrReplace`.
    StrReplace {
        /// The path of the file.
        path: String,
        /// The anchor text.
        old: String,
        /// The replacement text.
        new: String,
    },
    /// `VfsOp::Delete`.
    Delete {
        /// The path to remove.
        path: String,
    },
    /// `VfsOp::Glob`.
    Glob {
        /// The glob pattern.
        pattern: String,
    },
    /// `VfsOp::Exists`.
    Exists {
        /// The path to test.
        path: String,
    },
    /// `VfsOp::Stat`.
    Stat {
        /// The path to describe.
        path: String,
    },
    /// `VfsOp::List`.
    List {
        /// The path of the directory.
        path: String,
    },
    /// `VfsOp::ReadRange`.
    ReadRange {
        /// The path of the file.
        path: String,
        /// The byte the read starts at.
        offset: u64,
        /// The most bytes the read returns.
        len: u64,
    },
    /// `VfsOp::Mkdir`.
    Mkdir {
        /// The path of the directory.
        path: String,
        /// Whether the missing parent directories are created too.
        recursive: bool,
    },
    /// `VfsOp::Remove`.
    Remove {
        /// The path to remove.
        path: String,
        /// Whether a directory goes with everything under it.
        recursive: bool,
    },
    /// `VfsOp::Rename`.
    Rename {
        /// The path to move.
        from: String,
        /// The path it moves to.
        to: String,
    },
    /// `VfsOp::Copy`.
    Copy {
        /// The path of the file to copy.
        from: String,
        /// The path of the copy.
        to: String,
    },
}

impl VfsOpRecord {
    /// The record form of `op`.
    fn of(op: &VfsOp) -> VfsOpRecord {
        match op.clone() {
            VfsOp::Write { path, contents } => VfsOpRecord::Write {
                path,
                contents: BytesRecord::of(contents),
            },
            VfsOp::Append { path, contents } => VfsOpRecord::Append {
                path,
                contents: BytesRecord::of(contents),
            },
            VfsOp::Read { path, start, end } => VfsOpRecord::Read { path, start, end },
            VfsOp::ReadNumbered { path, start, end } => {
                VfsOpRecord::ReadNumbered { path, start, end }
            }
            VfsOp::StrReplace { path, old, new } => VfsOpRecord::StrReplace { path, old, new },
            VfsOp::Delete { path } => VfsOpRecord::Delete { path },
            VfsOp::Glob { pattern } => VfsOpRecord::Glob { pattern },
            VfsOp::Exists { path } => VfsOpRecord::Exists { path },
            VfsOp::Stat { path } => VfsOpRecord::Stat { path },
            VfsOp::List { path } => VfsOpRecord::List { path },
            VfsOp::ReadRange { path, offset, len } => VfsOpRecord::ReadRange { path, offset, len },
            VfsOp::Mkdir { path, recursive } => VfsOpRecord::Mkdir { path, recursive },
            VfsOp::Remove { path, recursive } => VfsOpRecord::Remove { path, recursive },
            VfsOp::Rename { from, to } => VfsOpRecord::Rename { from, to },
            VfsOp::Copy { from, to } => VfsOpRecord::Copy { from, to },
        }
    }

    /// The operation this record holds.
    ///
    /// # Errors
    /// Returns a sentence that names the fault when the bytes of a write
    /// or an append are hex that does not decode.
    fn into_op(self) -> Result<VfsOp, String> {
        Ok(match self {
            VfsOpRecord::Write { path, contents } => VfsOp::Write {
                path,
                contents: contents.into_bytes()?,
            },
            VfsOpRecord::Append { path, contents } => VfsOp::Append {
                path,
                contents: contents.into_bytes()?,
            },
            VfsOpRecord::Read { path, start, end } => VfsOp::Read { path, start, end },
            VfsOpRecord::ReadNumbered { path, start, end } => {
                VfsOp::ReadNumbered { path, start, end }
            }
            VfsOpRecord::StrReplace { path, old, new } => VfsOp::StrReplace { path, old, new },
            VfsOpRecord::Delete { path } => VfsOp::Delete { path },
            VfsOpRecord::Glob { pattern } => VfsOp::Glob { pattern },
            VfsOpRecord::Exists { path } => VfsOp::Exists { path },
            VfsOpRecord::Stat { path } => VfsOp::Stat { path },
            VfsOpRecord::List { path } => VfsOp::List { path },
            VfsOpRecord::ReadRange { path, offset, len } => VfsOp::ReadRange { path, offset, len },
            VfsOpRecord::Mkdir { path, recursive } => VfsOp::Mkdir { path, recursive },
            VfsOpRecord::Remove { path, recursive } => VfsOp::Remove { path, recursive },
            VfsOpRecord::Rename { from, to } => VfsOp::Rename { from, to },
            VfsOpRecord::Copy { from, to } => VfsOp::Copy { from, to },
        })
    }
}

/// The kind of a file as a record holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileTypeRecord {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link.
    Symlink,
    /// A named pipe.
    Fifo,
    /// A socket.
    Socket,
    /// A character device.
    CharDevice,
    /// A block device.
    BlockDevice,
    /// A kind this module does not name.
    Other,
}

impl FileTypeRecord {
    /// The record form of `file_type`.
    fn of(file_type: FileType) -> FileTypeRecord {
        match file_type {
            FileType::File => FileTypeRecord::File,
            FileType::Directory => FileTypeRecord::Directory,
            FileType::Symlink => FileTypeRecord::Symlink,
            FileType::Fifo => FileTypeRecord::Fifo,
            FileType::Socket => FileTypeRecord::Socket,
            FileType::CharDevice => FileTypeRecord::CharDevice,
            FileType::BlockDevice => FileTypeRecord::BlockDevice,
            _ => FileTypeRecord::Other,
        }
    }

    /// The kind this record holds.
    ///
    /// # Errors
    /// Returns a sentence that names the fault for `other`, which stands
    /// for no one kind.
    fn into_file_type(self) -> Result<FileType, String> {
        Ok(match self {
            FileTypeRecord::File => FileType::File,
            FileTypeRecord::Directory => FileType::Directory,
            FileTypeRecord::Symlink => FileType::Symlink,
            FileTypeRecord::Fifo => FileType::Fifo,
            FileTypeRecord::Socket => FileType::Socket,
            FileTypeRecord::CharDevice => FileType::CharDevice,
            FileTypeRecord::BlockDevice => FileType::BlockDevice,
            FileTypeRecord::Other => {
                return Err("a recorded file type of `other` names no kind of file".to_owned());
            }
        })
    }
}

/// The nanoseconds in one second.
const NANOS_PER_SECOND: u32 = 1_000_000_000;

/// A point in time as a record holds it: `secs` whole seconds after the
/// Unix epoch, negative for a time before it, and `nanos` nanoseconds
/// after that second. Both are integers, so the time reads back exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct TimeRecord {
    /// Whole seconds after the Unix epoch.
    secs: i64,
    /// Nanoseconds after that second, below one second.
    nanos: u32,
}

impl TimeRecord {
    /// The record form of `time`. A time further from the epoch than
    /// `i64` seconds records as the furthest second.
    fn of(time: SystemTime) -> TimeRecord {
        let whole = |seconds: u64| i64::try_from(seconds).unwrap_or(i64::MAX);
        match time.duration_since(UNIX_EPOCH) {
            Ok(after) => TimeRecord {
                secs: whole(after.as_secs()),
                nanos: after.subsec_nanos(),
            },
            Err(before) => {
                let before = before.duration();
                let secs = -whole(before.as_secs());
                match before.subsec_nanos() {
                    0 => TimeRecord { secs, nanos: 0 },
                    nanos => TimeRecord {
                        secs: secs.saturating_sub(1),
                        nanos: NANOS_PER_SECOND - nanos,
                    },
                }
            }
        }
    }

    /// The time this record holds.
    ///
    /// # Errors
    /// Returns a sentence that names the fault for a time the platform's
    /// clock cannot represent.
    fn into_time(self) -> Result<SystemTime, String> {
        let second = match u64::try_from(self.secs) {
            Ok(after) => UNIX_EPOCH.checked_add(Duration::from_secs(after)),
            Err(_) => UNIX_EPOCH.checked_sub(Duration::from_secs(self.secs.unsigned_abs())),
        };
        second
            .and_then(|second| second.checked_add(Duration::new(0, self.nanos)))
            .ok_or_else(|| {
                format!(
                    "a recorded time of {} seconds and {} nanoseconds is outside the clock's range",
                    self.secs, self.nanos
                )
            })
    }
}

/// A [`Stat`] as a record holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StatRecord {
    /// What kind of node this is.
    file_type: FileTypeRecord,
    /// Size in bytes.
    size: u64,
    /// POSIX mode bits, when the backend tracks them.
    mode: Option<u32>,
    /// Last modification time, when the backend tracks it.
    modified: Option<TimeRecord>,
    /// Creation time, when the backend tracks it.
    created: Option<TimeRecord>,
}

impl StatRecord {
    /// The record form of `stat`.
    fn of(stat: &Stat) -> StatRecord {
        StatRecord {
            file_type: FileTypeRecord::of(stat.file_type),
            size: stat.size,
            mode: stat.mode,
            modified: stat.modified.map(TimeRecord::of),
            created: stat.created.map(TimeRecord::of),
        }
    }

    /// The metadata this record holds.
    ///
    /// # Errors
    /// Returns a sentence that names the fault for a kind of file or a
    /// time that does not read back.
    fn into_stat(self) -> Result<Stat, String> {
        let modified = self.modified.map(TimeRecord::into_time).transpose()?;
        let created = self.created.map(TimeRecord::into_time).transpose()?;
        Ok(Stat::new(self.file_type.into_file_type()?, self.size)
            .with_mode(self.mode)
            .with_modified(modified)
            .with_created(created))
    }
}

/// An [`Entry`] as a record holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EntryRecord {
    /// The entry's name within its directory.
    name: String,
    /// The entry's metadata.
    stat: StatRecord,
    /// The annotation shown beside the entry, when it has one.
    description: Option<String>,
}

impl EntryRecord {
    /// The record form of `entry`.
    fn of(entry: &Entry) -> EntryRecord {
        EntryRecord {
            name: entry.name.clone(),
            stat: StatRecord::of(&entry.stat),
            description: entry.description.clone(),
        }
    }

    /// The entry this record holds.
    ///
    /// # Errors
    /// Returns a sentence that names the fault when the entry's metadata
    /// does not read back.
    fn into_entry(self) -> Result<Entry, String> {
        Ok(Entry::new(self.name, self.stat.into_stat()?).with_description(self.description))
    }
}

/// A [`VfsOutcome`] as a record holds it: the whole outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum VfsOutcomeRecord {
    /// `VfsOutcome::Unit`.
    Unit,
    /// `VfsOutcome::Text`.
    Text(String),
    /// `VfsOutcome::Paths`.
    Paths(Vec<String>),
    /// `VfsOutcome::Bool`.
    Bool(bool),
    /// `VfsOutcome::Stat`.
    Stat(StatRecord),
    /// `VfsOutcome::Entries`.
    Entries(Vec<EntryRecord>),
    /// `VfsOutcome::Bytes`.
    Bytes(BytesRecord),
}

impl VfsOutcomeRecord {
    /// The record form of `outcome`.
    fn of(outcome: &VfsOutcome) -> VfsOutcomeRecord {
        match outcome.clone() {
            VfsOutcome::Unit => VfsOutcomeRecord::Unit,
            VfsOutcome::Text(text) => VfsOutcomeRecord::Text(text),
            VfsOutcome::Paths(paths) => VfsOutcomeRecord::Paths(paths),
            VfsOutcome::Bool(flag) => VfsOutcomeRecord::Bool(flag),
            VfsOutcome::Stat(stat) => VfsOutcomeRecord::Stat(StatRecord::of(&stat)),
            VfsOutcome::Entries(entries) => {
                VfsOutcomeRecord::Entries(entries.iter().map(EntryRecord::of).collect())
            }
            VfsOutcome::Bytes(bytes) => VfsOutcomeRecord::Bytes(BytesRecord::of(bytes)),
        }
    }

    /// The outcome this record holds.
    ///
    /// # Errors
    /// Returns a sentence that names the fault when a stat, an entry, or
    /// hex bytes do not read back.
    fn into_outcome(self) -> Result<VfsOutcome, String> {
        Ok(match self {
            VfsOutcomeRecord::Unit => VfsOutcome::Unit,
            VfsOutcomeRecord::Text(text) => VfsOutcome::Text(text),
            VfsOutcomeRecord::Paths(paths) => VfsOutcome::Paths(paths),
            VfsOutcomeRecord::Bool(flag) => VfsOutcome::Bool(flag),
            VfsOutcomeRecord::Stat(stat) => VfsOutcome::Stat(stat.into_stat()?),
            VfsOutcomeRecord::Entries(entries) => VfsOutcome::Entries(
                entries
                    .into_iter()
                    .map(EntryRecord::into_entry)
                    .collect::<Result<_, _>>()?,
            ),
            VfsOutcomeRecord::Bytes(bytes) => VfsOutcome::Bytes(bytes.into_bytes()?),
        })
    }
}

/// Who asked for an operation, as a record holds it. Flattened into the
/// effect's record, the variant's name is the one key that says who.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum VfsAskerRecord {
    /// A `store` call of the prompt's Lua: the id of the chain the store
    /// was made for.
    #[serde(rename = "store")]
    Lua(ChainId),
    /// A tool call: the id of its effect.
    #[serde(rename = "call")]
    Tool(u64),
}

/// A [`VfsReceipt`] as a record holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum VfsReceiptRecord {
    /// `VfsReceipt::Done`.
    Done,
    /// `VfsReceipt::Bytes`.
    Bytes(u64),
    /// `VfsReceipt::Count`.
    Count(u64),
    /// `VfsReceipt::Bool`.
    Bool(bool),
    /// `VfsReceipt::Stat`.
    Stat(StatRecord),
}

impl VfsReceiptRecord {
    /// The record form of `receipt`.
    fn of(receipt: &VfsReceipt) -> VfsReceiptRecord {
        match receipt {
            VfsReceipt::Done => VfsReceiptRecord::Done,
            VfsReceipt::Bytes(count) => VfsReceiptRecord::Bytes(*count),
            VfsReceipt::Count(count) => VfsReceiptRecord::Count(*count),
            VfsReceipt::Bool(flag) => VfsReceiptRecord::Bool(*flag),
            VfsReceipt::Stat(stat) => VfsReceiptRecord::Stat(StatRecord::of(stat)),
        }
    }

    /// The receipt this record holds.
    ///
    /// # Errors
    /// Returns a sentence that names the fault when a stat does not read
    /// back.
    fn into_receipt(self) -> Result<VfsReceipt, String> {
        Ok(match self {
            VfsReceiptRecord::Done => VfsReceipt::Done,
            VfsReceiptRecord::Bytes(count) => VfsReceipt::Bytes(count),
            VfsReceiptRecord::Count(count) => VfsReceipt::Count(count),
            VfsReceiptRecord::Bool(flag) => VfsReceipt::Bool(flag),
            VfsReceiptRecord::Stat(stat) => VfsReceipt::Stat(stat.into_stat()?),
        })
    }
}

/// Writes `asker` as the one key that says who asked, for a flattened
/// field's `serialize_with`.
///
/// # Errors
/// Returns the serializer's own error.
pub(super) fn serialize_asker<S: Serializer>(
    asker: &VfsAsker,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let record = match asker {
        VfsAsker::Lua { store } => VfsAskerRecord::Lua(store.clone()),
        VfsAsker::Tool { call } => VfsAskerRecord::Tool(call.get()),
    };
    record.serialize(serializer)
}

/// Reads who asked back from the key [`serialize_asker`] writes, for a
/// flattened field's `deserialize_with`.
///
/// # Errors
/// Returns the deserializer's error for a record that holds neither key.
pub(super) fn deserialize_asker<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<VfsAsker, D::Error> {
    Ok(match VfsAskerRecord::deserialize(deserializer)? {
        VfsAskerRecord::Lua(store) => VfsAsker::Lua { store },
        VfsAskerRecord::Tool(call) => VfsAsker::Tool {
            call: EffectId(call),
        },
    })
}

/// Writes `answer`, the receipt of an operation's outcome or its failure,
/// with the receipt in its record form, for a record's `serialize_with`.
///
/// # Errors
/// Returns the serializer's own error.
pub(super) fn serialize_receipt<S, E>(
    answer: &Result<VfsReceipt, E>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    E: Serialize,
{
    answer
        .as_ref()
        .map(VfsReceiptRecord::of)
        .serialize(serializer)
}

/// Reads the receipt of an operation's outcome, or its failure, back from
/// the form [`serialize_receipt`] writes, for a record's
/// `deserialize_with`.
///
/// # Errors
/// Returns the deserializer's error for a value of another form, or for a
/// stat that does not read back.
pub(super) fn deserialize_receipt<'de, D, E>(
    deserializer: D,
) -> Result<Result<VfsReceipt, E>, D::Error>
where
    D: Deserializer<'de>,
    E: Deserialize<'de>,
{
    match Result::<VfsReceiptRecord, E>::deserialize(deserializer)? {
        Ok(record) => record.into_receipt().map(Ok).map_err(D::Error::custom),
        Err(failure) => Ok(Err(failure)),
    }
}

/// Writes `op` as its record form, for a record's `serialize_with`.
///
/// # Errors
/// Returns the serializer's own error.
pub(super) fn serialize_op<S: Serializer>(op: &VfsOp, serializer: S) -> Result<S::Ok, S::Error> {
    VfsOpRecord::of(op).serialize(serializer)
}

/// Reads an operation back from its record form, for a record's
/// `deserialize_with`.
///
/// # Errors
/// Returns the deserializer's error for a value that is no operation's
/// record form, or whose hex bytes do not decode.
pub(super) fn deserialize_op<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<VfsOp, D::Error> {
    VfsOpRecord::deserialize(deserializer)?
        .into_op()
        .map_err(D::Error::custom)
}

/// Writes `answer`, an operation's whole outcome or its failure, with the
/// outcome in its record form, for a record's `serialize_with`.
///
/// # Errors
/// Returns the serializer's own error.
pub(super) fn serialize_answer<S, E>(
    answer: &Result<VfsOutcome, E>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    E: Serialize,
{
    answer
        .as_ref()
        .map(VfsOutcomeRecord::of)
        .serialize(serializer)
}

/// Reads an operation's whole outcome, or its failure, back from the form
/// [`serialize_answer`] writes, for a record's `deserialize_with`.
///
/// # Errors
/// Returns the deserializer's error for a value of another form, or for
/// an outcome that does not read back: a kind of file of `other`, a time
/// outside the clock's range, or hex bytes that do not decode.
pub(super) fn deserialize_answer<'de, D, E>(
    deserializer: D,
) -> Result<Result<VfsOutcome, E>, D::Error>
where
    D: Deserializer<'de>,
    E: Deserialize<'de>,
{
    match Result::<VfsOutcomeRecord, E>::deserialize(deserializer)? {
        Ok(record) => record.into_outcome().map(Ok).map_err(D::Error::custom),
        Err(failure) => Ok(Err(failure)),
    }
}

#[cfg(test)]
#[path = "vfs_record-tests.rs"]
mod tests;
