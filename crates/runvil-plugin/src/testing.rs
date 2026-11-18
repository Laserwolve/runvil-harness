//! [`TestCall`]: lends a [`ToolContext`] to a Plugin crate's own tests.

use runvil_types::future::BoxFuture;
use runvil_types::tools::{ToolCallOrigin, ToolCaller, ToolId};
use runvil_vfs::{Access, Origin, VfsError, VfsOp, VfsOutcome, VfsRef};

use crate::{ToolContext, ToolFiles};

/// One tool call a test makes directly, without a Harness.
///
/// It owns everything a [`ToolContext`] borrows: the called tool's id, a
/// fresh in-memory filesystem of its own, and an origin naming the
/// section `Test` and a caller that is the script unless
/// [`with_caller`](TestCall::with_caller) names another. The context's
/// files and its access reach that one filesystem, so a test seeds a file
/// through either and reads it back through the other. A request through
/// the files is answered at once: its future is ready at its first poll.
/// A test calls the Pack's factory for the instance it calls.
#[derive(Debug)]
pub struct TestCall {
    tool: ToolId,
    files: AnsweredAtOnce,
    origin: ToolCallOrigin,
}

/// The files of a [`TestCall`]: each request is performed where it is
/// made, on the call's own in-memory filesystem.
#[derive(Debug)]
struct AnsweredAtOnce(Access);

impl ToolFiles for AnsweredAtOnce {
    fn request(&self, op: VfsOp) -> BoxFuture<'_, Result<VfsOutcome, VfsError>> {
        Box::pin(std::future::ready(self.0.perform(&op)))
    }
}

impl TestCall {
    /// A call of `tool` from the script of the section `Test`, over a
    /// fresh in-memory filesystem.
    ///
    /// # Panics
    /// Panics when the fresh in-memory filesystem refuses the access,
    /// which its backend never does.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "a test fixture; the in-memory backend accepts every acquire"
    )]
    pub fn new(tool: ToolId) -> TestCall {
        let access = VfsRef::default()
            .acquire(Origin::new("plugin test"))
            .expect("a fresh memory filesystem acquires");
        let origin = ToolCallOrigin::new("Test", ToolCaller::Script);
        TestCall {
            tool,
            files: AnsweredAtOnce(access),
            origin,
        }
    }

    /// Returns the call with `caller` as the origin's caller, keeping its
    /// section.
    #[must_use]
    pub fn with_caller(mut self, caller: ToolCaller) -> TestCall {
        self.origin = ToolCallOrigin::new("Test", caller);
        self
    }

    /// Lends the context to one call.
    #[must_use]
    pub fn context(&self) -> ToolContext<'_> {
        ToolContext::new(&self.tool, &self.files.0, &self.origin, &self.files)
    }
}
