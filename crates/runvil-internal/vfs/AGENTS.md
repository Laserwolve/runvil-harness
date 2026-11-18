# runvil-vfs

- The machinery modules hold no run concepts. A `Store` is generic machinery (an empty memory backend under its own claims ledger, which any access's identity can view), so it lives with the handle; the mode gate stays at the crate root.
- A `Store` checks claims, and a handle from `VfsRef::new` or `VfsRefBuilder::build` checks none. The setting that makes a handle check claims is private to the crate, and a test of the claims code builds its handle with it.
- Origin labels are the most specific available: a section name for a chain, a tool id for a tool, a fixture name for a test.
- Backends and `Access` are synchronous, because storage is synchronous by nature. Both stay synchronous: an async form of either is ruled out.
