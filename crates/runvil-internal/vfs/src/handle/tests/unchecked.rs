//! Tests for a handle that a caller builds, which checks no claims: two
//! unordered accesses to one path both run and the later write is the
//! content, whichever public constructor built the handle and whichever
//! operations the two make, and the handle's ledger stays empty.

use super::*;
use crate::MemoryBackend;

/// A handle from each public constructor, each over empty memory, beside
/// the constructor's name.
fn built() -> [(&'static str, VfsRef); 4] {
    [
        ("VfsRef::new", VfsRef::new(MemoryBackend::new())),
        ("VfsRef::default", VfsRef::default()),
        (
            "VfsRefBuilder::build",
            VfsRef::builder().mount("/", MemoryBackend::new()).build(),
        ),
        (
            "VfsRef::overlay",
            VfsRef::default().overlay("/over", MemoryBackend::new()),
        ),
    ]
}

#[test]
fn two_live_scopes_of_a_built_handle_use_one_path_and_the_later_write_is_the_content()
-> Result<(), VfsError> {
    for (how, vfs) in built() {
        // Both scopes are live at once, and nothing orders one before
        // the other.
        let first = access(&vfs)?;
        let second = access(&vfs)?;
        first.write("/notes.md", b"first")?;
        assert_eq!(second.read("/notes.md")?, b"first", "{how}");
        second.write("/notes.md", b"second")?;
        assert_eq!(
            first.read("/notes.md")?,
            b"second",
            "{how}: the later write is the content"
        );
        first.append("/notes.md", b"!")?;
        assert_eq!(second.read_string("/notes.md")?, "second!", "{how}");
    }
    Ok(())
}

#[test]
fn two_unjoined_forks_of_a_built_handle_make_every_kind_of_operation_on_one_tree()
-> Result<(), VfsError> {
    let (parent, scope) = VfsRef::default().acquire_scope(test_origin())?;
    let left = scope.spawn(&parent, test_origin())?;
    let right = scope.spawn(&parent, test_origin())?;
    // Each step below would conflict with the one before it on a handle
    // that checks claims: a read, a listing, or a glob against an
    // unordered write, and a write, a rename, a copy, or a removal
    // against an unordered read or write.
    left.write("/d/a.md", b"left")?;
    assert_eq!(right.read("/d/a.md")?, b"left");
    assert!(right.exists("/d")?);
    assert_eq!(right.stat("/d/a.md")?.size, 4);
    assert_eq!(right.list("/d")?.len(), 1);
    assert_eq!(right.glob("/d/*")?, ["/d/a.md"]);
    right.str_replace("/d/a.md", "left", "right")?;
    left.mkdir("/d/sub", true)?;
    right.rename("/d/a.md", "/d/sub/b.md")?;
    left.copy("/d/sub/b.md", "/d/c.md")?;
    assert_eq!(parent.read("/d/c.md")?, b"right");
    assert!(right.remove("/d", true)?);
    assert_eq!(left.glob("/**")?, Vec::<String>::new());
    // The parent never joined either fork.
    parent.write("/d/a.md", b"parent")?;
    assert_eq!(left.read("/d/a.md")?, b"parent");
    Ok(())
}

#[test]
fn a_built_handle_records_no_claim_and_registers_no_scope() -> Result<(), VfsError> {
    for (how, vfs) in built() {
        let (parent, scope) = vfs.acquire_scope(test_origin())?;
        let child = scope.spawn(&parent, test_origin())?;
        parent.write("/a/b.md", b"x")?;
        child.read("/a/b.md")?;
        child.list("/a")?;
        child.glob("/a/*")?;
        child.copy("/a/b.md", "/a/c.md")?;
        child.rename("/a/c.md", "/a/d.md")?;
        child.remove("/a", true)?;
        let tables = vfs.volume.claims.tables();
        assert_eq!(tables.entries, 0, "{how}: no claim was counted");
        assert!(
            tables.paths.is_empty()
                && tables.children.is_empty()
                && tables.patterns.is_empty()
                && tables.subtrees.is_empty()
                && tables.ancestors.is_empty(),
            "{how}: no region holds a claim"
        );
        assert!(tables.scopes.is_empty(), "{how}: no scope is registered");
    }
    Ok(())
}

#[test]
fn a_built_handle_mounted_in_a_handle_that_checks_claims_is_checked_there_alone()
-> Result<(), VfsError> {
    // The outer handle checks claims on its own namespace, and the
    // handle mounted in it checks none of its own.
    let inner = VfsRef::default();
    let outer = handle(inner.clone());
    let first = access(&outer)?;
    first.write("/f.txt", b"1")?;
    conflict_message(access(&outer)?.write("/f.txt", b"2"));
    // Through the mounted handle itself, a second scope writes the path.
    access(&inner)?.write("/f.txt", b"3")?;
    assert_eq!(first.read("/f.txt")?, b"3");
    Ok(())
}
