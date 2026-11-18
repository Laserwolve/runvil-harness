//! The file operations as values: [`VfsOp`], one operation with its
//! arguments, and [`VfsOutcome`], what a performed operation returns.

use crate::stat::{Entry, Stat};
#[cfg(doc)]
use crate::{Access, Op, VfsError};

/// One file operation with its arguments, which [`Access::perform`] runs.
///
/// The list serves two askers. The `store` functions of a prompt's Lua
/// use the first eight operations. A tool uses `Write`, `Append`, and the
/// last seven.
///
/// The separate [`Op`] names only the kind of a file operation (`Read`,
/// `Write`, `Rename`, and so on), which a policy matches on and a watcher
/// is told about.
///
/// The line bounds of `Read` and `ReadNumbered` are `i64`. A negative
/// bound converts to 0 when the operation runs, so the range check
/// rejects it with the same error a zero bound produces.
///
/// The type holds only plain data, so an operation can be kept, compared,
/// and written to a record.
///
/// The enum is closed on purpose: it has no `#[non_exhaustive]`, so a
/// `match` on it names every operation and needs no wildcard arm. A new
/// operation breaks the build at each such `match`, rather than reaching
/// code that cannot perform it, record it, or pass it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsOp {
    /// Creates the file at `path`, or replaces its content, with
    /// `contents`.
    Write {
        /// The path of the file.
        path: String,
        /// The bytes the file holds afterward.
        contents: Vec<u8>,
    },
    /// Adds `contents` to the end of the file at `path`, and creates the
    /// file when it is absent.
    Append {
        /// The path of the file.
        path: String,
        /// The bytes to add.
        contents: Vec<u8>,
    },
    /// Reads the file at `path` as text.
    ///
    /// With both bounds omitted, it reads the whole file. With `start`, it
    /// reads a 1-based inclusive range of lines. An `end` given alone is
    /// an error.
    Read {
        /// The path of the file.
        path: String,
        /// The optional 1-based first line.
        start: Option<i64>,
        /// The optional 1-based last line.
        end: Option<i64>,
    },
    /// Reads like `Read` and prefixes each line with its line number in
    /// the file. The bounds work the same way.
    ReadNumbered {
        /// The path of the file.
        path: String,
        /// The optional 1-based first line.
        start: Option<i64>,
        /// The optional 1-based last line.
        end: Option<i64>,
    },
    /// Replaces `old` with `new` in the file at `path`.
    StrReplace {
        /// The path of the file.
        path: String,
        /// The anchor text, which must occur exactly once.
        old: String,
        /// The replacement text.
        new: String,
    },
    /// Removes the file or empty directory at `path`, and succeeds when
    /// the path is absent. A directory that still holds entries is
    /// refused.
    Delete {
        /// The path to remove.
        path: String,
    },
    /// Lists the paths that match `pattern`.
    Glob {
        /// The glob pattern.
        pattern: String,
    },
    /// Reports whether anything exists at `path`.
    Exists {
        /// The path to test.
        path: String,
    },
    /// Returns the type and the length of what `path` names.
    Stat {
        /// The path to describe.
        path: String,
    },
    /// Lists the entries of the directory at `path`.
    List {
        /// The path of the directory.
        path: String,
    },
    /// Reads up to `len` bytes of the file at `path`, from the byte
    /// `offset` on.
    ReadRange {
        /// The path of the file.
        path: String,
        /// The byte the read starts at.
        offset: u64,
        /// The most bytes the read returns.
        len: u64,
    },
    /// Creates the directory at `path`.
    Mkdir {
        /// The path of the directory.
        path: String,
        /// Whether to create the missing parent directories too.
        recursive: bool,
    },
    /// Removes the file or directory at `path`, and fails with
    /// [`VfsError::NotFound`] when the path is absent.
    Remove {
        /// The path to remove.
        path: String,
        /// Whether to remove a directory that holds entries, with
        /// everything under it.
        recursive: bool,
    },
    /// Moves the file or directory at `from` to `to`.
    Rename {
        /// The path to move.
        from: String,
        /// The path it moves to.
        to: String,
    },
    /// Copies the file at `from` to `to`.
    Copy {
        /// The path of the file to copy.
        from: String,
        /// The path of the copy.
        to: String,
    },
}

/// What one performed [`VfsOp`] returns.
///
/// Closed on purpose, like [`VfsOp`]: a new outcome breaks the build at
/// each `match` that names every outcome, rather than reaching code that
/// has no record form for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsOutcome {
    /// The operation succeeded and returns nothing: a write, an append, a
    /// replacement, a delete, a directory creation, a removal, a rename,
    /// or a copy.
    Unit,
    /// The text of a `Read` or a `ReadNumbered`: the whole file, or the
    /// requested range of lines.
    Text(String),
    /// The paths that match the pattern of a `Glob`, in sorted order.
    Paths(Vec<String>),
    /// Whether the path of an `Exists` exists.
    Bool(bool),
    /// The metadata of the path of a `Stat`.
    Stat(Stat),
    /// The entries of the directory of a `List`.
    Entries(Vec<Entry>),
    /// The bytes of a `ReadRange`.
    Bytes(Vec<u8>),
}
