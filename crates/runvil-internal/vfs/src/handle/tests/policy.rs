//! Tests for what a handle's policy is asked: nothing for a path that is
//! not valid, the canonical path for one that is, and each of the two
//! paths of a copy or a rename under that operation's own kind, so a
//! policy that refuses only reads under a path does not keep the content
//! there.

use super::*;

/// A policy that allows every operation and keeps what it was asked: the
/// kind of each operation beside its path.
#[derive(Clone, Default)]
struct AskedPolicy {
    asked: Arc<Mutex<Vec<(Op, String)>>>,
}

impl AskedPolicy {
    /// What the policy was asked so far, in order.
    fn asked(&self) -> Vec<(Op, String)> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Policy for AskedPolicy {
    fn check(&self, op: Op, path: &VfsPath) -> Verdict {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((op, path.to_string()));
        Verdict::Allow
    }
}

/// Refuses every `Op::Read` under `/secret` and allows everything else.
struct NoReadsOfSecrets;

impl Policy for NoReadsOfSecrets {
    fn check(&self, op: Op, path: &VfsPath) -> Verdict {
        if op == Op::Read && path.as_str().starts_with("/secret") {
            Verdict::Deny("no reads under /secret".to_owned())
        } else {
            Verdict::Allow
        }
    }
}

/// `op` on `path`, as an [`AskedPolicy`] keeps it.
fn asked(op: Op, path: &str) -> (Op, String) {
    (op, path.to_owned())
}

#[test]
fn a_path_that_is_not_valid_fails_before_the_policy_is_asked() -> Result<(), VfsError> {
    let policy = AskedPolicy::default();
    let vfs = VfsRef::builder()
        .mount("/", TestFs::seeded(&[("/a.txt", "x")]))
        .policy(policy.clone())
        .build();
    let access = access(&vfs)?;
    assert!(matches!(access.read(""), Err(VfsError::InvalidPath { .. })));
    // The second path leaves the root.
    assert!(matches!(
        access.write("/../a.txt", b"y"),
        Err(VfsError::InvalidPath { .. })
    ));
    assert!(
        policy.asked().is_empty(),
        "neither operation reached the policy"
    );

    access.read("a.txt")?;
    assert_eq!(
        policy.asked(),
        [asked(Op::Read, "/a.txt")],
        "a valid path reaches the policy in its canonical form"
    );
    Ok(())
}

#[test]
fn a_copy_and_a_rename_ask_the_policy_about_both_paths_as_their_own_kind() -> Result<(), VfsError> {
    let policy = AskedPolicy::default();
    let vfs = VfsRef::builder()
        .mount("/", TestFs::seeded(&[("/secret/key", "k")]))
        .policy(policy.clone())
        .build();
    let access = access(&vfs)?;
    access.copy("/secret/key", "/copied")?;
    access.rename("/secret/key", "/moved")?;
    assert_eq!(
        policy.asked(),
        [
            asked(Op::Copy, "/secret/key"),
            asked(Op::Copy, "/copied"),
            asked(Op::Rename, "/secret/key"),
            asked(Op::Rename, "/moved"),
        ],
        "the source of each is asked about as the operation's own kind, and never as a read"
    );
    Ok(())
}

#[test]
fn a_policy_that_refuses_only_reads_under_a_path_lets_a_copy_and_a_rename_take_the_content_out()
-> Result<(), VfsError> {
    let vfs = VfsRef::builder()
        .mount("/", TestFs::seeded(&[("/secret/key", "k")]))
        .policy(NoReadsOfSecrets)
        .build();
    let access = access(&vfs)?;
    assert!(matches!(
        access.read("/secret/key"),
        Err(VfsError::PermissionDenied { .. })
    ));
    access.copy("/secret/key", "/copied")?;
    assert_eq!(
        access.read("/copied")?,
        b"k",
        "the copy holds the content that the policy keeps from a read"
    );
    access.rename("/secret/key", "/moved")?;
    assert_eq!(access.read("/moved")?, b"k");
    Ok(())
}
