//! The nine file functions an `RvToolCallArgs` carries, each of which
//! takes one request of its step as a `VfsOp` and fills nothing, and
//! [`Answer`], what the loader lends the step after a request.

use std::ffi::c_void;
use std::ptr;

use runvil_plugin::{Entry, FileType, Stat, VfsError, VfsOp, VfsOutcome};
use runvil_plugin_sys::{
    RV_ERR_FILE, RV_FILE_ERROR_ALREADY_EXISTS, RV_FILE_ERROR_DENIED, RV_FILE_ERROR_IO,
    RV_FILE_ERROR_IS_A_DIRECTORY, RV_FILE_ERROR_NOT_A_DIRECTORY, RV_FILE_ERROR_NOT_EMPTY,
    RV_FILE_ERROR_NOT_FOUND, RV_FILE_ERROR_OTHER, RV_FILE_TYPE_DIRECTORY, RV_FILE_TYPE_FILE,
    RV_FILE_TYPE_OTHER, RV_FILE_TYPE_SYMLINK, RV_MAX_PATH, RV_MAX_WRITE, RV_OK, RvBytes,
    RvDirEntries, RvDirEntry, RvFileAnswer, RvFileError, RvFileErrorKind, RvStat, RvStatus, RvStr,
};

use crate::session::{Field, TOOL_CALL, answer, field, rv_str, size_field};

#[cfg(test)]
#[path = "file-tests.rs"]
mod tests;

const PATH: Field = field!("the path", 4096, RV_MAX_PATH);
const FROM: Field = field!("the source path", 4096, RV_MAX_PATH);
const TO: Field = field!("the destination path", 4096, RV_MAX_PATH);
const BYTES: Field = field!("the run of bytes", 16_777_216, RV_MAX_WRITE);

/// What a `recursive` that is neither 0 nor 1 is called.
const RECURSIVE: &str = "recursive is neither 0 nor 1";

/// What a second file request in one step is called.
const SECOND_REQUEST: &str = "a step made a second file request";
/// What a file request in a step that called `finish` is called.
const REQUEST_AFTER_FINISH: &str = "a step made a file request after it called finish";
/// What `finish` in a step that made a file request is called.
pub(crate) const FINISH_AFTER_REQUEST: &str = "a step called finish after it made a file request";

/// Takes the operation `read` reads out of a file function's arguments,
/// as the one request of the open step; returns RV_OK, or RV_ERR_MISUSE
/// for a request the step may not make and for arguments `read` refuses.
/// The step's own state is checked first, so `read` runs only for a
/// request the step may make.
fn take(ctx: *mut c_void, read: impl FnOnce() -> Result<VfsOp, &'static str>) -> RvStatus {
    answer(ctx, &TOOL_CALL, |session| {
        if session.result.is_some() {
            return Err(REQUEST_AFTER_FINISH);
        }
        if session.request.is_some() {
            return Err(SECOND_REQUEST);
        }
        session.request = Some(read()?);
        Ok(RV_OK)
    })
}

/// Takes a request for what `path` names; returns RV_OK or
/// RV_ERR_MISUSE, and fills nothing.
pub(crate) unsafe extern "C" fn file_stat(ctx: *mut c_void, path: RvStr) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes `path` valid for its length during
        // this call.
        let path = unsafe { PATH.read(path) }?;
        Ok(VfsOp::Stat { path })
    })
}

/// Takes a request for the entries of the directory at `path`; returns
/// RV_OK or RV_ERR_MISUSE, and fills nothing.
pub(crate) unsafe extern "C" fn file_list(ctx: *mut c_void, path: RvStr) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes `path` valid for its length during
        // this call.
        let path = unsafe { PATH.read(path) }?;
        Ok(VfsOp::List { path })
    })
}

/// Takes a request for up to `len` bytes of the file at `path` from the
/// byte `offset` on; returns RV_OK or RV_ERR_MISUSE, and fills nothing.
pub(crate) unsafe extern "C" fn file_read(
    ctx: *mut c_void,
    path: RvStr,
    offset: u64,
    len: u64,
) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes `path` valid for its length during
        // this call.
        let path = unsafe { PATH.read(path) }?;
        // A real file cannot be positioned past this offset, and no file
        // is that long, so the read finds the end and returns no bytes.
        let offset = offset.min(i64::MAX.unsigned_abs());
        // No read returns more than memory can hold, and a backend that
        // slices a whole file refuses a length it cannot address.
        let len = usize::try_from(len).unwrap_or(usize::MAX);
        let len = u64::try_from(len).unwrap_or(u64::MAX);
        Ok(VfsOp::ReadRange { path, offset, len })
    })
}

/// Takes a request to write `bytes` as the whole contents of the file at
/// `path`; returns RV_OK or RV_ERR_MISUSE.
pub(crate) unsafe extern "C" fn file_write(
    ctx: *mut c_void,
    path: RvStr,
    bytes: RvBytes,
) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes `path` and `bytes` valid for their
        // lengths during this call.
        let (path, contents) = unsafe { (PATH.read(path)?, BYTES.bytes(bytes.ptr, bytes.len)?) };
        Ok(VfsOp::Write { path, contents })
    })
}

/// Takes a request to add `bytes` to the end of the file at `path`;
/// returns RV_OK or RV_ERR_MISUSE.
pub(crate) unsafe extern "C" fn file_append(
    ctx: *mut c_void,
    path: RvStr,
    bytes: RvBytes,
) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes `path` and `bytes` valid for their
        // lengths during this call.
        let (path, contents) = unsafe { (PATH.read(path)?, BYTES.bytes(bytes.ptr, bytes.len)?) };
        Ok(VfsOp::Append { path, contents })
    })
}

/// Takes a request to make a directory at `path`; returns RV_OK or
/// RV_ERR_MISUSE.
pub(crate) unsafe extern "C" fn file_mkdir(
    ctx: *mut c_void,
    path: RvStr,
    recursive: u32,
) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes `path` valid for its length during
        // this call.
        let path = unsafe { PATH.read(path) }?;
        let recursive = flag(recursive)?;
        Ok(VfsOp::Mkdir { path, recursive })
    })
}

/// Takes a request to remove the file or the directory at `path`;
/// returns RV_OK or RV_ERR_MISUSE.
pub(crate) unsafe extern "C" fn file_remove(
    ctx: *mut c_void,
    path: RvStr,
    recursive: u32,
) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes `path` valid for its length during
        // this call.
        let path = unsafe { PATH.read(path) }?;
        let recursive = flag(recursive)?;
        Ok(VfsOp::Remove { path, recursive })
    })
}

/// Takes a request to move the file or the directory at `from` to `to`;
/// returns RV_OK or RV_ERR_MISUSE.
pub(crate) unsafe extern "C" fn file_rename(ctx: *mut c_void, from: RvStr, to: RvStr) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes both paths valid for their lengths
        // during this call.
        let (from, to) = unsafe { (FROM.read(from)?, TO.read(to)?) };
        Ok(VfsOp::Rename { from, to })
    })
}

/// Takes a request to copy the file at `from` to `to`; returns RV_OK or
/// RV_ERR_MISUSE.
pub(crate) unsafe extern "C" fn file_copy(ctx: *mut c_void, from: RvStr, to: RvStr) -> RvStatus {
    take(ctx, || {
        // SAFETY: the contract makes both paths valid for their lengths
        // during this call.
        let (from, to) = unsafe { (FROM.read(from)?, TO.read(to)?) };
        Ok(VfsOp::Copy { from, to })
    })
}

/// `recursive` as the contract spells it: 0 for off and 1 for on.
fn flag(recursive: u32) -> Result<bool, &'static str> {
    match recursive {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RECURSIVE),
    }
}

/// What a request asks for: the one field its answer holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wanted {
    /// What a path names, which `file_stat` asks for.
    Stat,
    /// The entries of a directory, which `file_list` asks for.
    Entries,
    /// At most this many bytes of a file, which `file_read` asks for.
    Bytes(u64),
    /// Nothing but a status, which a function that changes the files
    /// asks for.
    Nothing,
}

impl Wanted {
    /// What the request for `op` asks for.
    pub(crate) fn by(op: &VfsOp) -> Wanted {
        match op {
            VfsOp::Stat { .. } => Wanted::Stat,
            VfsOp::List { .. } => Wanted::Entries,
            VfsOp::ReadRange { len, .. } => Wanted::Bytes(*len),
            // The file functions take no other read, so every other
            // operation is one of their six changes.
            _ => Wanted::Nothing,
        }
    }
}

/// The answer to one request, with the memory it points into. A call
/// makes it from the outcome of its request and keeps it until the step
/// that receives it has returned.
pub(crate) struct Answer {
    /// What a path names, or zero when the request asked for no stat.
    stat: RvStat,
    /// The bytes of a read.
    bytes: Vec<u8>,
    /// The names the entries of a listing point at, kept only so that
    /// they stay alive.
    _names: Vec<String>,
    /// The entries of a listing.
    entries: Vec<RvDirEntry>,
    /// The kind and the text of the failure, when the operation failed.
    error: Option<(RvFileErrorKind, String)>,
}

// SAFETY: the pointers in `entries` lead into the heap allocations of
// `_names`, which this value owns and never changes, so the value moves
// to another thread whole.
unsafe impl Send for Answer {}

impl Answer {
    /// The answer to a request that asked for `wanted` and whose
    /// operation ended as `outcome`: the one thing the request asked for,
    /// nothing for a change, or the failure. An outcome of another kind
    /// than the request asked for is a failure too, so a step never
    /// reads a field the operation did not fill.
    pub(crate) fn to(wanted: Wanted, outcome: Result<VfsOutcome, VfsError>) -> Answer {
        match (wanted, outcome) {
            (_, Err(error)) => Answer::failed(&error),
            (Wanted::Stat, Ok(VfsOutcome::Stat(stat))) => Answer {
                stat: raw_stat(&stat),
                ..Answer::empty()
            },
            (Wanted::Entries, Ok(VfsOutcome::Entries(entries))) => Answer::of_entries(entries),
            (Wanted::Bytes(most), Ok(VfsOutcome::Bytes(mut bytes))) => {
                // The contract promises at most `len` bytes, whatever the
                // backend returned.
                bytes.truncate(usize::try_from(most).unwrap_or(usize::MAX));
                Answer {
                    bytes,
                    ..Answer::empty()
                }
            }
            (Wanted::Nothing, Ok(VfsOutcome::Unit)) => Answer::empty(),
            (_, Ok(_)) => Answer::failed(&VfsError::Backend {
                message: "the file operation returned something other than what was asked for"
                    .to_owned(),
            }),
        }
    }

    /// An answer that holds nothing.
    fn empty() -> Answer {
        Answer {
            stat: RvStat {
                file_type: 0,
                reserved: 0,
                len: 0,
            },
            bytes: Vec::new(),
            _names: Vec::new(),
            entries: Vec::new(),
            error: None,
        }
    }

    /// The answer that holds `entries`.
    fn of_entries(entries: Vec<Entry>) -> Answer {
        let stats: Vec<RvStat> = entries.iter().map(|entry| raw_stat(&entry.stat)).collect();
        let names: Vec<String> = entries.into_iter().map(|entry| entry.name).collect();
        // Each name's bytes sit in its own allocation, so moving `names`
        // into the answer leaves the pointers below valid.
        let entries = names
            .iter()
            .zip(stats)
            .map(|(name, stat)| RvDirEntry {
                name: rv_str(name),
                stat,
            })
            .collect();
        Answer {
            _names: names,
            entries,
            ..Answer::empty()
        }
    }

    /// The answer of an operation that failed with `error`.
    fn failed(error: &VfsError) -> Answer {
        Answer {
            error: Some(describe(error)),
            ..Answer::empty()
        }
    }

    /// The answer as the contract lends it to a step: the one field the
    /// request asked for, or `error`, and every other field zero. It
    /// points into this value, which the caller keeps until the step has
    /// returned.
    pub(crate) fn raw(&self) -> RvFileAnswer {
        let (status, error) = match &self.error {
            Some((kind, text)) => (
                RV_ERR_FILE,
                RvFileError {
                    kind: *kind,
                    reserved: 0,
                    text: rv_str(text),
                },
            ),
            None => (
                RV_OK,
                RvFileError {
                    kind: 0,
                    reserved: 0,
                    text: RvStr {
                        ptr: ptr::null(),
                        len: 0,
                    },
                },
            ),
        };
        RvFileAnswer {
            size: size_field::<RvFileAnswer>(),
            status,
            stat: self.stat,
            entries: RvDirEntries {
                ptr: first(&self.entries),
                len: self.entries.len(),
            },
            bytes: RvBytes {
                ptr: first(&self.bytes),
                len: self.bytes.len(),
            },
            error,
        }
    }
}

/// Where `values` begin, or NULL when there are none, so that a field
/// with nothing in it is zero.
fn first<T>(values: &[T]) -> *const T {
    if values.is_empty() {
        ptr::null()
    } else {
        values.as_ptr()
    }
}

/// `stat` as an answer holds it. A kind of node the contract does not
/// name is `RV_FILE_TYPE_OTHER`.
fn raw_stat(stat: &Stat) -> RvStat {
    RvStat {
        file_type: match stat.file_type {
            FileType::File => RV_FILE_TYPE_FILE,
            FileType::Directory => RV_FILE_TYPE_DIRECTORY,
            FileType::Symlink => RV_FILE_TYPE_SYMLINK,
            _ => RV_FILE_TYPE_OTHER,
        },
        reserved: 0,
        len: stat.size,
    }
}

/// The contract's kind for `error`, and the text a model reads.
///
/// A missing path, a path that is there already, a path that is not a
/// directory, a path that is one, and a directory that has entries keep
/// the error's own sentence. A policy denial carries the policy's own
/// reason. An I/O failure carries the kind of the failure and the
/// system's description. Of the errors the contract has no kind for, a
/// refused path carries the rule it broke, and every other one its
/// sentence without the leading label. No error reads as
/// `RV_FILE_ERROR_CONFLICT`: the files a tool call reaches check no
/// conflict.
fn describe(error: &VfsError) -> (RvFileErrorKind, String) {
    match error {
        VfsError::NotFound { .. } => (RV_FILE_ERROR_NOT_FOUND, error.to_string()),
        VfsError::AlreadyExists { .. } => (RV_FILE_ERROR_ALREADY_EXISTS, error.to_string()),
        VfsError::NotADirectory { .. } => (RV_FILE_ERROR_NOT_A_DIRECTORY, error.to_string()),
        VfsError::IsADirectory { .. } => (RV_FILE_ERROR_IS_A_DIRECTORY, error.to_string()),
        VfsError::DirectoryNotEmpty { .. } => (RV_FILE_ERROR_NOT_EMPTY, error.to_string()),
        VfsError::PermissionDenied { reason, .. } | VfsError::ApprovalRequired { reason, .. } => {
            (RV_FILE_ERROR_DENIED, reason.clone())
        }
        VfsError::Io { kind, message, .. } => (RV_FILE_ERROR_IO, format!("{kind}: {message}")),
        // The path is quoted in the sentence and may hold `": "` itself,
        // so the rule is taken from the error, not cut out of the text.
        VfsError::InvalidPath { reason, .. } => (RV_FILE_ERROR_OTHER, reason.to_string()),
        VfsError::NotUtf8 { path, .. } => (
            RV_FILE_ERROR_OTHER,
            format!("{path} holds a name or contents that are not UTF-8"),
        ),
        other => (RV_FILE_ERROR_OTHER, without_label(&other.to_string())),
    }
}

/// `text` without its leading label: the text after the first `": "`, or
/// the whole text when it has none.
fn without_label(text: &str) -> String {
    text.split_once(": ")
        .map_or(text, |(_, rest)| rest)
        .to_owned()
}
