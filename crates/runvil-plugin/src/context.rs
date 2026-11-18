//! [`ToolContext`], what one tool call receives, [`ToolFiles`], through
//! which the call asks for file operations, and [`RenderContext`], what a
//! render learns about its section entry.

use std::fmt;

use runvil_types::future::BoxFuture;
use runvil_types::models::ModelInfo;
use runvil_types::tools::{ToolCallOrigin, ToolId};
use runvil_vfs::{VfsError, VfsOp, VfsOutcome};

/// The files of one tool call: the call asks for a file operation and
/// waits for its outcome.
///
/// The Harness reports each request to the run, which issues it as an
/// effect. The Harness performs the operation on the run's filesystem
/// with one blocking call, under the call's own identity and the run's
/// policy, records it, and resolves the request with the whole outcome.
/// So a run's record holds every operation a tool asks for here, in
/// order, and a later poll of the call receives the answer.
///
/// The operations are the ones [`VfsOp`] lists, with paths rooted at `/`
/// of the filesystem the Host gave the run. No path reaches a store.
pub trait ToolFiles: Send + Sync {
    /// Asks for `op` and resolves with its outcome.
    ///
    /// The request joins the call's queue when the returned future is
    /// first polled, and it goes out when the Harness takes it from the
    /// queue, which the Harness does only between two polls of the call.
    /// A future that is never polled, or that is dropped in the poll that
    /// first polled it, sends nothing. Dropping it later may be too late
    /// to take the request back: once it has gone out, the Harness
    /// performs the operation while the call is in flight and discards
    /// the outcome. A call that must know whether an operation happened
    /// awaits its outcome. A call may wait on several requests at once,
    /// and the Harness performs them one after another, in the order the
    /// call made them.
    ///
    /// The future resolves only while the Harness polls the call, so a
    /// call awaits it in its own future and never blocks on it.
    ///
    /// # Errors
    /// Resolves with the [`VfsError`] of an operation that failed: a path
    /// that is absent or refused, a policy that denies the operation, or
    /// a backend that fails.
    fn request(&self, op: VfsOp) -> BoxFuture<'_, Result<VfsOutcome, VfsError>>;
}

/// What one tool call lends its Pack: facts about that call alone, the
/// called tool's id, the call's files, and who made the call.
///
/// The files are the call's one way to the run's filesystem. Every
/// operation the call asks for runs under one identity, the call's own,
/// which ends with the call, on paths rooted at `/`, and the run's record
/// holds each one. The filesystem checks no conflicts: an operation runs
/// whatever another call or task has read or written, and the later
/// write to a path is its content. A tool that edits a file as a read
/// and a later write can therefore lose a change that another task makes
/// between the two, where one [`VfsOp::StrReplace`] cannot. The context
/// borrows everything for the call alone, so a Pack cannot store the
/// files, move them into a spawned task or thread, or keep them past the
/// call.
///
/// A call gets no services. A Plugin's `install` keeps the Host-wide
/// services its Pack needs in the Pack's
/// [`PackFactory`](crate::PackFactory), which hands them to each run's
/// instance it makes at [`new_pack`](crate::PackFactory::new_pack).
pub struct ToolContext<'a> {
    tool: &'a ToolId,
    origin: &'a ToolCallOrigin,
    files: &'a dyn ToolFiles,
}

impl fmt::Debug for ToolContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolContext")
            .field("tool", &self.tool)
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

impl<'a> ToolContext<'a> {
    /// Facts about one call: the tool id, its origin, and its files. The
    /// Harness builds one per call.
    #[must_use]
    pub fn new(tool: &'a ToolId, origin: &'a ToolCallOrigin, files: &'a dyn ToolFiles) -> Self {
        Self {
            tool,
            origin,
            files,
        }
    }

    /// The tool that was called. A Pack with several tools matches on
    /// its [`name`](ToolId::name) to pick one.
    #[must_use]
    pub fn tool(&self) -> &'a ToolId {
        self.tool
    }

    /// The call's files, through which it asks for each file operation
    /// as an effect the Harness performs and records.
    #[must_use]
    pub fn files(&self) -> &'a dyn ToolFiles {
        self.files
    }

    /// Who made the call and where.
    #[must_use]
    pub fn origin(&self) -> &'a ToolCallOrigin {
        self.origin
    }
}

/// What a Pack learns about the section entry it renders for, in
/// [`Pack::render`](crate::Pack::render).
///
/// A section entry is one execution of a section: re-entering a section,
/// a fanout arm, a task, and a child started with `call` are each their
/// own entry. The context holds plain data only, free of Rust-only handles
/// such as [`HostServices`](crate::HostServices), so a later C verb can
/// pass it serialized. The Harness builds it with [`RenderContext::new`],
/// and later facts join as fields without breaking existing Packs.
#[derive(Debug)]
#[non_exhaustive]
pub struct RenderContext<'a> {
    /// The section entry, the value Lua reads as `sys.id`.
    pub entry: &'a str,
    /// The model of the section's first round.
    pub model: &'a ModelInfo,
}

impl<'a> RenderContext<'a> {
    /// A context naming the section entry `entry` and the model of its
    /// first round, `model`. The Harness builds one per render.
    #[must_use]
    pub fn new(entry: &'a str, model: &'a ModelInfo) -> Self {
        Self { entry, model }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the contexts a Pack receives: a tool call's getters and
    //! its `Debug` form, and the facts a render learns.

    use std::num::NonZeroU32;

    use runvil_types::future::BoxFuture;
    use runvil_types::models::{ModelId, ModelInfo, ThinkingMode};
    use runvil_types::tools::{ToolCallOrigin, ToolCaller, ToolId};
    use runvil_vfs::{VfsError, VfsOp, VfsOutcome};

    use super::{RenderContext, ToolContext, ToolFiles};

    fn fetch() -> ToolId {
        ToolId::parse("web/fetch").expect("a two-segment tool id parses")
    }

    /// Files that answer every request with nothing, at once.
    struct NoFiles;

    impl ToolFiles for NoFiles {
        fn request(&self, _op: VfsOp) -> BoxFuture<'_, Result<VfsOutcome, VfsError>> {
            Box::pin(std::future::ready(Ok(VfsOutcome::Unit)))
        }
    }

    fn model_origin() -> ToolCallOrigin {
        ToolCallOrigin::new("Fetch", ToolCaller::Model)
    }

    #[test]
    fn each_getter_returns_the_value_the_context_was_built_from() {
        let (tool, origin) = (fetch(), model_origin());
        let files = NoFiles;
        let cx = ToolContext::new(&tool, &origin, &files);
        assert!(std::ptr::eq(cx.tool(), &raw const tool));
        assert!(std::ptr::eq(cx.origin(), &raw const origin));
        assert!(std::ptr::addr_eq(cx.files(), &raw const files));
        assert_eq!(cx.tool().name(), "fetch");
        assert_eq!(cx.origin().caller, ToolCaller::Model);
    }

    #[test]
    fn a_tool_context_shows_its_tool_and_origin_in_debug_and_not_its_files() {
        let (tool, origin) = (fetch(), model_origin());
        let shown = format!("{:?}", ToolContext::new(&tool, &origin, &NoFiles));
        assert!(shown.starts_with("ToolContext { tool: "), "{shown}");
        assert!(
            shown.contains("\"fetch\"") && shown.contains("section: \"Fetch\""),
            "{shown}"
        );
        assert!(shown.ends_with(".. }"), "{shown}");
    }

    #[test]
    fn a_render_context_holds_the_entry_and_the_model_it_was_built_with() {
        let model = ModelInfo::new(
            ModelId::gateway("analyst").expect("a valid model id"),
            "A careful analysis model",
            NonZeroU32::new(8_192).expect("a non-zero context window"),
            ThinkingMode::Switchable,
        );
        let entry = String::from("chat-1");
        let cx = RenderContext::new(&entry, &model);
        assert_eq!(cx.entry, "chat-1");
        assert!(std::ptr::eq(cx.model, &raw const model));
        assert_eq!(cx.model.id().name(), "analyst");
    }
}
