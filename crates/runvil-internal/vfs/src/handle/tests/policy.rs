//! Tests for what a handle's policy is asked: nothing for a path that is
//! not valid, the canonical path for one that is, and four questions for
//! a copy or a rename. Each of those two asks about its source and its
//! destination under its own kind, then about its source as a read and
//! its destination as a write, so a policy that refuses reads under a
//! path keeps the content there, and one that refuses writes under a
//! path keeps new content out. The first refusal ends the operation, and
//! a rename asks about no delete. A replacement of text asks about its
//! path as a write and then as a read, so a policy that refuses reads
//! under a path refuses it there, with its own refusal and never with a
//! count of what the file holds.

use super::*;

/// What a policy was asked: the kind of the operation beside the path.
type Asked = (Op, String);

/// A policy that keeps each question it was asked, in order, and denies
/// the questions that `refuses` picks, each with the reason
/// `<kind> on <path> is refused`. It allows every other question.
#[derive(Clone)]
struct AskedPolicy {
    asked: Arc<Mutex<Vec<Asked>>>,
    refuses: fn(Op, &str) -> bool,
}

impl AskedPolicy {
    /// A policy that allows every question.
    fn allowing() -> AskedPolicy {
        AskedPolicy::refusing(|_, _| false)
    }

    /// A policy that denies the questions `refuses` picks.
    fn refusing(refuses: fn(Op, &str) -> bool) -> AskedPolicy {
        AskedPolicy {
            asked: Arc::default(),
            refuses,
        }
    }

    /// Takes what the policy was asked since the last call, in order.
    fn take(&self) -> Vec<Asked> {
        std::mem::take(&mut *self.asked.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Policy for AskedPolicy {
    fn check(&self, op: Op, path: &VfsPath) -> Verdict {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((op, path.to_string()));
        if (self.refuses)(op, path.as_str()) {
            Verdict::Deny(format!("{op:?} on {path} is refused"))
        } else {
            Verdict::Allow
        }
    }
}

/// `op` on `path`, as an [`AskedPolicy`] keeps it.
fn asked(op: Op, path: &str) -> Asked {
    (op, path.to_owned())
}

/// The denial an [`AskedPolicy`] gives `op` on `path`.
fn denied<T>(op: Op, path: &str) -> Result<T, VfsError> {
    Err(VfsError::PermissionDenied {
        path: path.to_owned(),
        reason: format!("{op:?} on {path} is refused"),
    })
}

/// A copy or a rename of `from` to `to` through an access.
type Transfer = fn(&Access, &str, &str) -> Result<(), VfsError>;

/// A copy and a rename, each beside the kind the policy is asked about
/// first.
fn transfers() -> [(Op, Transfer); 2] {
    [
        (Op::Copy, |access, from, to| access.copy(from, to)),
        (Op::Rename, |access, from, to| access.rename(from, to)),
    ]
}

/// An access to a handle that holds `/secret/key` and `/open/note`
/// behind `policy`. The files are seeded under the backend, so the
/// policy is asked nothing for them.
fn guarded(policy: &AskedPolicy) -> Result<Access, VfsError> {
    let vfs = VfsRef::builder()
        .mount(
            "/",
            TestFs::seeded(&[("/secret/key", "k"), ("/open/note", "n")]),
        )
        .policy(policy.clone())
        .build();
    access(&vfs)
}

#[test]
fn a_path_that_is_not_valid_fails_before_the_policy_is_asked() -> Result<(), VfsError> {
    let policy = AskedPolicy::allowing();
    let access = guarded(&policy)?;
    assert!(matches!(access.read(""), Err(VfsError::InvalidPath { .. })));
    // The second path leaves the root.
    assert!(matches!(
        access.write("/../a.txt", b"y"),
        Err(VfsError::InvalidPath { .. })
    ));
    assert!(
        policy.take().is_empty(),
        "neither operation reached the policy"
    );

    access.read("open/note")?;
    assert_eq!(
        policy.take(),
        [asked(Op::Read, "/open/note")],
        "a valid path reaches the policy in its canonical form"
    );
    Ok(())
}

#[test]
fn a_copy_and_a_rename_ask_about_both_paths_as_their_own_kind_then_as_a_read_and_a_write()
-> Result<(), VfsError> {
    for (kind, transfer) in transfers() {
        let policy = AskedPolicy::allowing();
        let access = guarded(&policy)?;
        transfer(&access, "/open/note", "/open/taken")?;
        assert_eq!(
            policy.take(),
            [
                asked(kind, "/open/note"),
                asked(kind, "/open/taken"),
                asked(Op::Read, "/open/note"),
                asked(Op::Write, "/open/taken"),
            ],
            "{kind:?}: the two paths under the operation's own kind, then the read of the \
             source and the write of the destination, and no delete of the source"
        );
        assert_eq!(access.read("/open/taken")?, b"n", "{kind:?}");
    }
    Ok(())
}

#[test]
fn the_first_refusal_ends_a_copy_or_a_rename_before_any_later_question() -> Result<(), VfsError> {
    // A source whose validation fails ends the operation before any
    // question, and a destination whose validation fails ends it after
    // the one question about the source.
    for (kind, transfer) in transfers() {
        let policy = AskedPolicy::allowing();
        let access = guarded(&policy)?;
        assert!(matches!(
            transfer(&access, "", "/open/taken"),
            Err(VfsError::InvalidPath { .. })
        ));
        assert!(policy.take().is_empty(), "{kind:?}: an invalid source");
        assert!(matches!(
            transfer(&access, "/open/note", "/../taken"),
            Err(VfsError::InvalidPath { .. })
        ));
        assert_eq!(
            policy.take(),
            [asked(kind, "/open/note")],
            "{kind:?}: an invalid destination"
        );
    }

    // A refusal of the operation's own kind on the source is the one
    // question asked.
    for (kind, transfer) in transfers() {
        let policy = AskedPolicy::refusing(|op, path| {
            matches!(op, Op::Copy | Op::Rename) && path == "/open/note"
        });
        let access = guarded(&policy)?;
        assert_eq!(
            transfer(&access, "/open/note", "/open/taken"),
            denied(kind, "/open/note")
        );
        assert_eq!(policy.take(), [asked(kind, "/open/note")], "{kind:?}");
    }

    // A refusal of the operation's own kind on the destination comes
    // before the read and the write are asked about.
    for (kind, transfer) in transfers() {
        let policy = AskedPolicy::refusing(|op, path| {
            matches!(op, Op::Copy | Op::Rename) && path == "/open/taken"
        });
        let access = guarded(&policy)?;
        assert_eq!(
            transfer(&access, "/open/note", "/open/taken"),
            denied(kind, "/open/taken")
        );
        assert_eq!(
            policy.take(),
            [asked(kind, "/open/note"), asked(kind, "/open/taken")],
            "{kind:?}"
        );
    }
    Ok(())
}

#[test]
fn a_policy_that_refuses_reads_under_a_path_refuses_a_copy_and_a_rename_out_of_it()
-> Result<(), VfsError> {
    for (kind, transfer) in transfers() {
        let policy =
            AskedPolicy::refusing(|op, path| op == Op::Read && path.starts_with("/secret"));
        let access = guarded(&policy)?;
        assert_eq!(access.read("/secret/key"), denied(Op::Read, "/secret/key"));
        policy.take();

        assert_eq!(
            transfer(&access, "/secret/key", "/open/taken"),
            denied(Op::Read, "/secret/key"),
            "{kind:?}: the refusal is the policy's own, for the read of the source"
        );
        assert_eq!(
            policy.take(),
            [
                asked(kind, "/secret/key"),
                asked(kind, "/open/taken"),
                asked(Op::Read, "/secret/key"),
            ],
            "{kind:?}: the write of the destination is not asked about"
        );
        assert!(
            !access.exists("/open/taken")?,
            "{kind:?}: the destination does not appear"
        );
        assert!(
            access.exists("/secret/key")?,
            "{kind:?}: the source stays where it is"
        );
    }
    Ok(())
}

#[test]
fn a_policy_that_refuses_writes_under_a_path_refuses_a_copy_and_a_rename_into_it()
-> Result<(), VfsError> {
    for (kind, transfer) in transfers() {
        let policy =
            AskedPolicy::refusing(|op, path| op == Op::Write && path.starts_with("/secret"));
        let access = guarded(&policy)?;
        assert_eq!(
            access.write("/secret/new", b"x"),
            denied(Op::Write, "/secret/new")
        );
        policy.take();

        assert_eq!(
            transfer(&access, "/open/note", "/secret/taken"),
            denied(Op::Write, "/secret/taken"),
            "{kind:?}: the refusal is the policy's own, for the write of the destination"
        );
        assert_eq!(
            policy.take(),
            [
                asked(kind, "/open/note"),
                asked(kind, "/secret/taken"),
                asked(Op::Read, "/open/note"),
                asked(Op::Write, "/secret/taken"),
            ],
            "{kind:?}"
        );
        assert!(
            !access.exists("/secret/taken")?,
            "{kind:?}: the destination does not appear"
        );
        assert_eq!(
            access.read("/open/note")?,
            b"n",
            "{kind:?}: the source stays where it is"
        );
    }
    Ok(())
}

#[test]
fn a_rename_of_a_directory_asks_about_its_own_path_and_nothing_under_it() -> Result<(), VfsError> {
    // The policy guards `/vault/secret`, and the rename names the
    // directory above it, so no question names a guarded path.
    let policy =
        AskedPolicy::refusing(|op, path| op == Op::Read && path.starts_with("/vault/secret"));
    let vfs = VfsRef::builder()
        .mount("/", TestFs::seeded(&[("/vault/secret/key", "k")]))
        .policy(policy.clone())
        .build();
    let access = access(&vfs)?;
    assert_eq!(
        access.read("/vault/secret/key"),
        denied(Op::Read, "/vault/secret/key")
    );
    policy.take();

    access.rename("/vault", "/taken")?;
    assert_eq!(
        policy.take(),
        [
            asked(Op::Rename, "/vault"),
            asked(Op::Rename, "/taken"),
            asked(Op::Read, "/vault"),
            asked(Op::Write, "/taken"),
        ]
    );
    assert_eq!(
        access.read("/taken/secret/key")?,
        b"k",
        "the move took the guarded file with it, so a policy that guards a path has to \
         refuse a rename of each directory above it"
    );
    Ok(())
}

#[test]
fn a_policy_that_refuses_deletes_still_allows_a_rename() -> Result<(), VfsError> {
    let policy = AskedPolicy::refusing(|op, _| op == Op::Delete);
    let access = guarded(&policy)?;
    assert_eq!(
        access.remove("/open/note", false),
        denied(Op::Delete, "/open/note")
    );
    access.rename("/open/note", "/open/moved")?;
    assert!(!access.exists("/open/note")?);
    assert_eq!(access.read("/open/moved")?, b"n");
    Ok(())
}

#[test]
fn a_replacement_of_text_asks_about_its_path_as_a_write_and_then_as_a_read() -> Result<(), VfsError>
{
    let policy = AskedPolicy::allowing();
    let access = guarded(&policy)?;
    access.str_replace("/open/note", "n", "m")?;
    assert_eq!(
        policy.take(),
        [
            asked(Op::Write, "/open/note"),
            asked(Op::Read, "/open/note")
        ]
    );
    assert_eq!(access.read("/open/note")?, b"m");

    // A refusal of the write is the one question asked.
    let policy = AskedPolicy::refusing(|op, _| op == Op::Write);
    let access = guarded(&policy)?;
    assert_eq!(
        access.str_replace("/open/note", "n", "m"),
        denied(Op::Write, "/open/note")
    );
    assert_eq!(policy.take(), [asked(Op::Write, "/open/note")]);
    Ok(())
}

#[test]
fn a_policy_that_refuses_reads_under_a_path_refuses_a_replacement_of_text_there()
-> Result<(), VfsError> {
    let seeded = || TestFs::seeded(&[("/secret/key", "one two two")]);
    let behind = |policy: &AskedPolicy, files: &TestFs| {
        let vfs = VfsRef::builder()
            .mount("/", files.clone())
            .policy(policy.clone())
            .build();
        access(&vfs)
    };

    // Under a policy that allows the read, the failure of a replacement
    // tells how many times the text occurs.
    let files = seeded();
    assert_eq!(
        behind(&AskedPolicy::allowing(), &files)?.str_replace("/secret/key", "two", "x"),
        Err(VfsError::Anchor {
            path: "/secret/key".to_owned(),
            anchor: "two".to_owned(),
            count: 2,
        })
    );

    // The text occurs zero times, once, and twice.
    for text in ["three", "one", "two"] {
        let policy =
            AskedPolicy::refusing(|op, path| op == Op::Read && path.starts_with("/secret"));
        let files = seeded();
        let access = behind(&policy, &files)?;
        assert_eq!(
            access.str_replace("/secret/key", text, "x"),
            denied(Op::Read, "/secret/key"),
            "{text}: the failure is the policy's refusal of the read and never a count"
        );
        assert_eq!(
            policy.take(),
            [
                asked(Op::Write, "/secret/key"),
                asked(Op::Read, "/secret/key")
            ],
            "{text}"
        );
        assert!(
            !files
                .calls()
                .iter()
                .any(|call| matches!(call, Call::Op(..))),
            "{text}: the file is neither read nor written: {:?}",
            files.calls()
        );
        assert_eq!(files.files()["/secret/key"], b"one two two", "{text}");
    }
    Ok(())
}
