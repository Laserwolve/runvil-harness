//! The Runvil virtual filesystem and the editor mode policy over it.
//!
//! The machinery modules hold canonical paths, the claims model, the
//! mount router, the operation-observation seam, the backends, and the
//! file operations as values: a [`VfsOp`] is one operation with its
//! arguments, and [`Access::perform`] runs it. The
//! crate root holds runvil policy: [`ModePolicy`], the editor mode
//! gate. The machinery modules hold no runvil policy (no run
//! concepts). A [`Store`] is generic machinery: an empty set of files in
//! memory, under a claims ledger of its own, that any access's identity
//! can view. It sits outside every [`VfsRef`], and a store view's strict
//! logical-path rules are the store's caller contract, not runvil policy.
//!
//! This crate is the permanent bottom of the dependency stack: the library
//! is std only, with no normal or build dependencies.
//!
//! ## Invariants
//!
//! - May depend on: nothing. The library is std only, with no normal or
//!   build dependency, which the manifest test enforces.
//!   `cargo test -p build-xtask` enforces the product and container
//!   boundaries.

mod error;
mod glob;
mod handle;
mod memory;
mod observe;
mod path;
mod real;
mod router;
mod stat;
#[cfg(test)]
mod test_support;
mod traits;
mod vfs_op;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pub use error::{PathReason, VfsError};
pub use handle::{Access, ScopeHandle, Store, VfsRef};
pub use memory::MemoryBackend;
pub use observe::{OpEvent, Origin};
pub use path::{VfsPath, VfsPathBuf};
pub use real::{RealBackend, Writability};
pub use router::VfsRefBuilder;
pub use stat::{Entry, FileType, Stat};
pub use traits::{AcquireContext, AllowAll, ExecId, Op, Policy, Verdict, Vfs, VfsAccess};
pub use vfs_op::{VfsOp, VfsOutcome};

/// The default handle has one mount: an empty in-memory backend at `/`.
impl Default for VfsRef {
    fn default() -> VfsRef {
        VfsRef::builder().mount("/", MemoryBackend::new()).build()
    }
}

/// A permission mode that sets which changes the model may make to
/// storage.
///
/// Reads are allowed in every mode.
///
/// The enum is `#[non_exhaustive]`, so a `match` on it outside this crate
/// needs a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mode {
    /// Every change is refused pending user approval. Reads are allowed.
    Ask,
    /// Changes are allowed only to markdown paths, which are paths that
    /// end in `.md`, matched case-sensitively. Reads are allowed.
    ///
    /// A copy is checked on its source and its destination alike, so
    /// both paths must end in `.md`.
    Plan,
    /// Every operation is allowed.
    Agent,
}

/// A policy that limits changes to storage according to the current
/// `Mode`.
///
/// A `VfsRef` holds one policy and asks it about an operation path by
/// path, about a path once that path is valid and before the operation
/// is admitted, as [`Policy`] says. The mode limits only operations that
/// change storage: writes, appends, deletes, renames, directory
/// creation, copies, symbolic links, and permission changes. A copy and
/// a rename get its verdict under their own kinds, on the source and on
/// the destination alike. The two questions that follow, a read of the
/// source and a write of the destination, never refuse what it allowed
/// there: it allows every read, and a write of a path gets the answer
/// that a copy or a rename of that path got.
///
/// The policy shares its current mode with every [`ModeHandle`] it
/// returns. The caller keeps a handle and can change the mode mid-run.
/// The change reaches the next operation through the shared mode alone.
/// Whether a mode change can be undone depends only on whether anyone
/// still holds a handle.
#[derive(Debug)]
pub struct ModePolicy {
    mode: Arc<Mutex<Mode>>,
}

/// A handle that changes the mode of a `ModePolicy`.
///
/// Clones share one mode with each other and with the policy.
#[derive(Debug, Clone)]
pub struct ModeHandle {
    mode: Arc<Mutex<Mode>>,
}

impl ModePolicy {
    /// Returns a policy starting in `mode`.
    #[must_use]
    pub fn new(mode: Mode) -> ModePolicy {
        ModePolicy {
            mode: Arc::new(Mutex::new(mode)),
        }
    }

    /// Returns a handle that changes this policy's mode. A change made
    /// mid-run takes effect on the next operation.
    #[must_use]
    pub fn handle(&self) -> ModeHandle {
        ModeHandle {
            mode: Arc::clone(&self.mode),
        }
    }
}

impl ModeHandle {
    /// Sets the current mode.
    pub fn set(&self, mode: Mode) {
        *lock(&self.mode) = mode;
    }
}

/// Poison-safe lock on the shared mode cell.
fn lock(mode: &Arc<Mutex<Mode>>) -> MutexGuard<'_, Mode> {
    mode.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether the operation mutates storage. Modes gate mutations, never
/// reads.
fn is_mutation(op: Op) -> bool {
    match op {
        Op::Write
        | Op::Append
        | Op::Delete
        | Op::Rename
        | Op::Mkdir
        | Op::Copy
        | Op::Symlink
        | Op::Chmod => true,
        Op::Read | Op::Exists | Op::Glob | Op::List | Op::Stat | Op::ReadLink => false,
    }
}

/// Plan mode's markdown rule: a `.md` suffix, case-sensitively - the
/// virtual namespace follows POSIX path rules strictly, so `.MD` is not
/// markdown and the case-insensitive suggestion does not apply.
#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "virtual paths are POSIX-strict; case-insensitive extension matching is an operating-system notion"
)]
fn is_markdown(path: &VfsPath) -> bool {
    path.as_str().ends_with(".md")
}

impl Policy for ModePolicy {
    fn check(&self, op: Op, path: &VfsPath) -> Verdict {
        if !is_mutation(op) {
            return Verdict::Allow;
        }
        match *lock(&self.mode) {
            Mode::Agent => Verdict::Allow,
            Mode::Ask => Verdict::Ask(format!(
                "{op:?} on {path} needs user approval: the Ask mode refuses all mutations"
            )),
            // Under its own kind a copy is checked per path, source and
            // destination alike, so Plan refuses to even read a
            // non-markdown source. The policy cannot tell the two apart
            // there; conservative refusal is the safe side.
            Mode::Plan if is_markdown(path) => Verdict::Allow,
            Mode::Plan => Verdict::Deny(format!(
                "{op:?} on {path} is refused: the Plan mode allows mutations only to markdown paths"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, ModePolicy};
    use crate::{Access, FileType, Origin, VfsError, VfsRef};

    /// A manifest section is a dependency table when it is exactly
    /// `dependencies` or `build-dependencies`, a sub-table of one
    /// (`[dependencies.foo]` declares a dependency the same way), or either
    /// of those under a `target.<cfg>` qualifier. A cfg string may itself
    /// hold dots (`"sse4.1"`), so a target section is matched on any
    /// dot-separated key rather than on a fixed position.
    fn is_dependency_table(section: &str) -> bool {
        const TABLES: [&str; 2] = ["dependencies", "build-dependencies"];
        let mut keys = section.split('.');
        match keys.next() {
            Some("target") => keys.any(|key| TABLES.contains(&key)),
            first => first.is_some_and(|key| TABLES.contains(&key)),
        }
    }

    /// Every entry `manifest` declares under a dependency table, each
    /// naming its section and entry.
    fn dependency_declarations(manifest: &str) -> Vec<String> {
        let mut section = String::new();
        let mut declared = Vec::new();
        for raw_line in manifest.lines() {
            let line = raw_line.trim();
            if line.starts_with('[') {
                section = line.trim_matches(['[', ']']).to_owned();
                continue;
            }
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if is_dependency_table(&section) {
                declared.push(format!("[{section}] declares `{line}`"));
            }
        }
        declared
    }

    /// The zero-dependency rule is load-bearing: the library compiles alone
    /// and never rebuilds for a dependency rev, so the manifest must never
    /// declare a normal or build dependency. Dev-dependencies serve only the
    /// tests. This test reads the crate's own Cargo.toml and fails if a
    /// normal or build dependency table has an entry.
    #[test]
    fn the_manifest_declares_no_normal_or_build_dependencies() -> Result<(), std::io::Error> {
        let manifest = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
        )?;
        let declared = dependency_declarations(&manifest);
        assert!(
            declared.is_empty(),
            "zero-dependency rule violated: {declared:?}"
        );
        Ok(())
    }

    #[test]
    fn a_manifest_with_a_target_qualified_dependency_table_is_refused() {
        let manifest = |table: &str, entry: &str| {
            format!(
                "[package]\nname = \"fixture\"\n\n[target.'cfg(windows)']\n\n[{table}]\n{entry}\n"
            )
        };
        for (table, entry) in [
            (
                "target.'cfg(target_feature = \"sse4.1\")'.build-dependencies",
                "foo = \"1\"",
            ),
            ("target.'cfg(windows)'.dependencies.foo", "version = \"1\""),
        ] {
            assert_eq!(
                dependency_declarations(&manifest(table, entry)),
                [format!("[{table}] declares `{entry}`")],
                "[{table}] must be refused"
            );
        }
        let table = "target.'cfg(windows)'.dev-dependencies";
        assert_eq!(
            dependency_declarations(&manifest(table, "foo = \"1\"")),
            Vec::<String>::new(),
            "[{table}] must pass"
        );
    }

    #[test]
    fn dependency_sub_tables_count_as_dependency_tables() {
        // `[dependencies.foo]` declares a dependency without its section
        // name equalling a dependency table's.
        for section in [
            "dependencies",
            "dependencies.foo",
            "build-dependencies",
            "build-dependencies.foo",
            "target.'cfg(windows)'.dependencies",
        ] {
            assert!(is_dependency_table(section), "[{section}] must be caught");
        }
        for section in [
            "package",
            "lints",
            "features",
            "dependenciesfoo",
            "dev-dependencies",
            "dev-dependencies.foo",
            "target.'cfg(windows)'",
        ] {
            assert!(!is_dependency_table(section), "[{section}] must pass");
        }
    }

    #[test]
    fn a_mode_flip_through_the_shared_handle_is_visible_on_the_next_operation()
    -> Result<(), VfsError> {
        let policy = ModePolicy::new(Mode::Ask);
        let handle = policy.handle();
        let vfs = VfsRef::with_policy(VfsRef::default(), policy);
        let access = vfs.acquire(Origin::new("mode flip test"))?;
        let path = "notes.md";
        match access.write(path, b"x") {
            Err(VfsError::ApprovalRequired { path, .. }) => assert_eq!(path, "/notes.md"),
            other => panic!("expected an approval refusal, got {other:?}"),
        }
        // The UI flips the mode mid-run through the shared handle; the
        // very next operation sees it.
        handle.set(Mode::Agent);
        access.write(path, b"x")?;
        assert_eq!(access.read(path)?, b"x");
        Ok(())
    }

    #[test]
    fn plan_mode_allows_mutations_only_to_markdown_paths() -> Result<(), VfsError> {
        let policy = ModePolicy::new(Mode::Plan);
        let vfs = VfsRef::with_policy(VfsRef::default(), policy);
        let access = vfs.acquire(Origin::new("plan mode test"))?;
        let markdown = "notes.md";
        let binary = "data.bin";
        access.write(markdown, b"# ok")?;
        match access.write(binary, b"x") {
            Err(VfsError::PermissionDenied { path, .. }) => assert_eq!(path, "/data.bin"),
            other => panic!("expected a denial, got {other:?}"),
        }
        // The denied write never partially applied.
        assert!(!access.exists(binary)?);
        Ok(())
    }

    #[test]
    fn a_copy_and_a_rename_get_the_verdict_and_the_reason_of_their_own_kind_in_each_mode()
    -> Result<(), VfsError> {
        /// A copy or a rename of `from` to `to` through an access.
        type Change = fn(&Access, &str, &str) -> Result<(), VfsError>;
        let changes: [(&str, Change); 2] = [
            ("Copy", |access, from, to| access.copy(from, to)),
            ("Rename", |access, from, to| access.rename(from, to)),
        ];
        for (kind, change) in changes {
            let policy = ModePolicy::new(Mode::Agent);
            let handle = policy.handle();
            let vfs = VfsRef::with_policy(VfsRef::default(), policy);
            let access = vfs.acquire(Origin::new("mode change test"))?;
            access.write("/a.md", b"markdown")?;
            access.write("/a.bin", b"binary")?;

            // The Ask mode answers the first question, the operation's
            // own kind on its source.
            handle.set(Mode::Ask);
            assert_eq!(
                change(&access, "/a.md", "/b.md"),
                Err(VfsError::ApprovalRequired {
                    path: "/a.md".to_owned(),
                    reason: format!(
                        "{kind} on /a.md needs user approval: the Ask mode refuses all mutations"
                    ),
                }),
                "{kind} in the Ask mode"
            );

            // The Plan mode refuses the first path that is not markdown,
            // under the operation's own kind.
            handle.set(Mode::Plan);
            let refused = |path: &str| {
                Err(VfsError::PermissionDenied {
                    path: path.to_owned(),
                    reason: format!(
                        "{kind} on {path} is refused: the Plan mode allows mutations only to \
                         markdown paths"
                    ),
                })
            };
            assert_eq!(
                change(&access, "/a.bin", "/b.md"),
                refused("/a.bin"),
                "{kind} of a source that is not markdown, in the Plan mode"
            );
            assert_eq!(
                change(&access, "/a.md", "/b.bin"),
                refused("/b.bin"),
                "{kind} to a destination that is not markdown, in the Plan mode"
            );
            assert!(
                !access.exists("/b.md")? && !access.exists("/b.bin")?,
                "a refused {kind} made nothing"
            );
            assert_eq!(change(&access, "/a.md", "/b.md"), Ok(()), "{kind} in Plan");
            assert_eq!(access.read("/b.md")?, b"markdown");

            handle.set(Mode::Agent);
            assert_eq!(
                change(&access, "/a.bin", "/b.bin"),
                Ok(()),
                "{kind} in Agent"
            );
            assert_eq!(access.read("/b.bin")?, b"binary");
        }
        Ok(())
    }

    #[test]
    fn a_replacement_of_text_gets_the_verdict_and_the_reason_of_a_write_in_each_mode()
    -> Result<(), VfsError> {
        let policy = ModePolicy::new(Mode::Agent);
        let handle = policy.handle();
        let vfs = VfsRef::with_policy(VfsRef::default(), policy);
        let access = vfs.acquire(Origin::new("mode replacement test"))?;
        access.write("/a.md", b"one")?;
        access.write("/a.bin", b"one")?;

        handle.set(Mode::Ask);
        assert_eq!(
            access.str_replace("/a.md", "one", "two"),
            Err(VfsError::ApprovalRequired {
                path: "/a.md".to_owned(),
                reason: "Write on /a.md needs user approval: the Ask mode refuses all mutations"
                    .to_owned(),
            })
        );

        handle.set(Mode::Plan);
        assert_eq!(
            access.str_replace("/a.bin", "one", "two"),
            Err(VfsError::PermissionDenied {
                path: "/a.bin".to_owned(),
                reason: "Write on /a.bin is refused: the Plan mode allows mutations only to \
                         markdown paths"
                    .to_owned(),
            })
        );
        assert_eq!(access.read("/a.bin")?, b"one", "a refused replacement");
        access.str_replace("/a.md", "one", "two")?;
        assert_eq!(access.read("/a.md")?, b"two", "markdown, in the Plan mode");

        handle.set(Mode::Agent);
        access.str_replace("/a.bin", "one", "two")?;
        assert_eq!(access.read("/a.bin")?, b"two", "in the Agent mode");
        Ok(())
    }

    #[test]
    fn exactly_the_operations_that_change_storage_are_mutations() {
        use super::is_mutation;
        use crate::Op;
        let mutations = [
            Op::Write,
            Op::Append,
            Op::Delete,
            Op::Rename,
            Op::Mkdir,
            Op::Copy,
            Op::Symlink,
            Op::Chmod,
        ];
        let reads = [
            Op::Read,
            Op::Exists,
            Op::Glob,
            Op::List,
            Op::Stat,
            Op::ReadLink,
        ];
        assert!(mutations.into_iter().all(is_mutation));
        assert!(!reads.into_iter().any(is_mutation));
    }

    #[test]
    fn modes_gate_mutations_never_reads() -> Result<(), VfsError> {
        let policy = ModePolicy::new(Mode::Agent);
        let handle = policy.handle();
        let vfs = VfsRef::with_policy(VfsRef::default(), policy);
        let path = "paper.md";
        vfs.acquire(Origin::new("read gate test"))?
            .write(path, b"text")?;
        // Even in Ask, the strictest mode, reads flow.
        handle.set(Mode::Ask);
        let access = vfs.acquire(Origin::new("read gate test"))?;
        assert_eq!(access.read(path)?, b"text");
        assert!(access.exists(path)?);
        Ok(())
    }

    #[test]
    fn the_default_handle_is_one_empty_memory_mount_at_the_root() -> Result<(), VfsError> {
        let access = VfsRef::default().acquire(Origin::new("default handle test"))?;
        // The mount at `/` serves the whole namespace, and holds nothing.
        assert_eq!(access.stat("/")?.file_type, FileType::Directory);
        assert!(access.list("/")?.is_empty());
        // A write lands with no directory made first, and memory tracks
        // no time for it.
        access.write("/notes/draft.md", b"# draft")?;
        assert_eq!(access.read("notes/draft.md")?, b"# draft");
        assert_eq!(access.stat("/notes/draft.md")?.modified, None);
        // Each default handle has a memory of its own.
        let other = VfsRef::default().acquire(Origin::new("default handle test"))?;
        assert!(!other.exists("/notes/draft.md")?);
        Ok(())
    }
}
