//! Store failures: the failure a store operation's error becomes, and the
//! model-facing message of each one. `Access::perform` of `runvil-vfs`
//! runs the operation itself.

use runvil_vfs::{VfsError, VfsOp};

use crate::error::Error;
use crate::error_value::make_store_error;

/// The store failure `cause` with `message` in place of its own text.
#[must_use]
pub fn make_store_failure_error(cause: &VfsError, message: String) -> Error {
    let mut error = make_store_error(cause);
    error.message = message;
    error
}

/// The model-facing message for a failed store operation: what failed and,
/// when there is a fix, how to make it, so the text a model reads names
/// both. The operation supplies the wording its call surface promises (a
/// glob reports "invalid glob pattern", a path reports "invalid path"), and
/// the structured [`VfsError`] supplies the path, rule, anchor, and count.
#[must_use]
pub fn store_error_message(op: &VfsOp, error: &VfsError) -> String {
    match error {
        VfsError::NotFound { path } => format!("file not found in store: {path}"),
        VfsError::AlreadyExists { path } => format!("file already exists in store: {path}"),
        VfsError::NotADirectory { path } => format!("not a directory in store: {path}"),
        VfsError::IsADirectory { path } => format!("is a directory in store: {path}"),
        VfsError::DirectoryNotEmpty { path } => format!("directory not empty in store: {path}"),
        VfsError::NotUtf8 { path, valid_up_to } => {
            format!("file in store is not UTF-8 after byte {valid_up_to}: {path}")
        }
        VfsError::InvalidPath { path, reason } => {
            if matches!(op, VfsOp::Glob { .. }) {
                format!("invalid glob pattern {path:?}: {reason}")
            } else {
                format!("invalid path {path:?}: {reason}")
            }
        }
        VfsError::InvalidRange { path, reason } => {
            format!("invalid line range for {path}: {reason}")
        }
        VfsError::Anchor {
            path,
            anchor,
            count,
        } => {
            if anchor.is_empty() {
                format!("str_replace requires a non-empty anchor: {path}")
            } else if *count == 0 {
                format!("anchor {anchor:?} was not found in {path}, expected exactly one")
            } else {
                format!(
                    "anchor {anchor:?} occurs {count} times in {path}, expected exactly one; \
                     include more surrounding text so it matches once"
                )
            }
        }
        VfsError::PermissionDenied { path, reason } => {
            format!("permission denied for store path {path}: {reason}")
        }
        VfsError::ApprovalRequired { path, reason } => {
            format!("store path {path} needs approval, which this run cannot give: {reason}")
        }
        VfsError::ScopeEnded { path } => format!(
            "store access ended: the run or tool call that owned it has ended, so {path} \
             can no longer be touched through it"
        ),
        VfsError::ReadOnly { path } => format!("store path is read-only: {path}"),
        VfsError::OutsideRoot { path, detail } => {
            format!("store path {path} is outside the mounted root: {detail}")
        }
        VfsError::RootProtected { path } => {
            format!("the store's mounted root cannot be removed, renamed, or replaced: {path}")
        }
        VfsError::CrossMount { path, detail } => {
            format!("store operation on {path} spans two mounts: {detail}")
        }
        VfsError::Io { path, message, .. } => format!("store I/O failure on {path}: {message}"),
        VfsError::Unsupported { path, detail } => {
            format!("unsupported store operation on {path}: {detail}")
        }
        VfsError::Conflict { path, detail } => format!("store conflict on {path}: {detail}"),
        VfsError::Backend { message } => format!("store backend failure: {message}"),
        _ => "store operation failed".to_owned(),
    }
}
