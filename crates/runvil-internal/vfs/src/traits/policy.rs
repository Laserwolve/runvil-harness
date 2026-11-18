//! The policy hook: the [`Op`] a policy matches on, its [`Verdict`], the
//! [`Policy`] trait each handle consults, and the [`AllowAll`] default.

use crate::path::VfsPath;

/// The kind of filesystem operation being attempted, which a policy
/// matches on.
///
/// A copy and a rename are asked as what they are made of. Each asks
/// about its source and then about its destination under its own kind,
/// [`Op::Copy`] or [`Op::Rename`], and then about its source as
/// [`Op::Read`] and about its destination as [`Op::Write`]. The first
/// refusal ends the operation, and nothing is copied or moved. So a
/// policy that refuses reads under a path refuses a copy and a rename
/// out of it, and a policy that refuses writes under a path refuses a
/// copy and a rename into it. The questions under the operation's own
/// kind come first, so a policy that treats a copy or a rename as one
/// kind of change gives its own verdict and its own reason for it. A
/// rename is not asked about its source as [`Op::Delete`]: a policy that
/// refuses deletes so that nothing is lost can still allow a move.
///
/// The questions name the two paths of the operation and nothing under
/// them. A rename of a directory moves everything in it, so a policy
/// that guards a path must also refuse a rename of each directory above
/// that path.
///
/// A replacement of text in a file is asked the same way: about its
/// path as [`Op::Write`], and then as [`Op::Read`], because it reads the
/// file and its failure tells how many times the text occurs. So a
/// policy that refuses reads under a path refuses a replacement there,
/// and the failure is that refusal and never a count.
///
/// The enum is `#[non_exhaustive]`, so a `match` on it outside this crate
/// needs a wildcard arm. A policy should deny in that arm, so an operation
/// added later is refused until the policy names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Op {
    /// Reading a file's bytes.
    Read,
    /// Creating or overwriting a file.
    Write,
    /// Appending to a file.
    Append,
    /// Removing a file, link, or directory.
    Delete,
    /// Renaming or moving a path.
    Rename,
    /// Creating a directory.
    Mkdir,
    /// Copying a file.
    Copy,
    /// Testing for existence.
    Exists,
    /// Matching paths against a pattern.
    Glob,
    /// Listing a directory.
    List,
    /// Reading metadata.
    Stat,
    /// Creating a symbolic link.
    Symlink,
    /// Reading a symbolic link's target.
    ReadLink,
    /// Changing mode bits.
    Chmod,
}

/// A policy's decision about one operation.
///
/// Each reason string reaches a reader. The `Deny` string goes back to
/// the model as the tool error, so it tells the model how to recover.
/// The `Ask` string names what is being asked and which rule fired, for
/// the user to read in an approval dialog. The filesystem refuses a
/// `Deny` operation with `VfsError::PermissionDenied` and an `Ask`
/// operation with `VfsError::ApprovalRequired`, so any approval dialog
/// belongs to the application.
///
/// The enum is `#[non_exhaustive]`, so a `match` on it outside this crate
/// needs a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// The operation may proceed.
    Allow,
    /// The operation is refused. The string tells the model how to
    /// recover.
    Deny(String),
    /// The operation needs the user's approval. The string is the text
    /// for an approval dialog.
    Ask(String),
}

/// A hook that decides whether each filesystem operation may proceed.
///
/// Each `VfsRef` has one policy. `Access` asks it about an operation
/// path by path: about a path once that path is valid, and before the
/// operation is admitted. The first check that fails and the first
/// question that the policy refuses end the operation, so the policy is
/// asked nothing that would come after either. A copy and a rename ask
/// four questions, and a replacement of text asks two, as [`Op`] says.
/// A question names the canonical path of the operation and never what
/// a link leads to, so where a backend follows a link, as `RealBackend`
/// does inside its root, a read through a link under an allowed path
/// reaches a file under a guarded one. A policy can change its answers
/// during a run through shared state: the application holds the same
/// `Arc` and changes the state mid-run.
pub trait Policy: Send {
    /// Decides whether `op` on `path` may proceed.
    fn check(&self, op: Op, path: &VfsPath) -> Verdict;
}

/// A policy that allows every operation.
///
/// `VfsRef::new` uses it, and `VfsRefBuilder` uses it by default.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AllowAll;

impl Policy for AllowAll {
    fn check(&self, op: Op, path: &VfsPath) -> Verdict {
        let _ = (op, path);
        Verdict::Allow
    }
}
