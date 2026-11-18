//! One tool call of a Pack instance that a Plugin's entry point made: the
//! future that sends its RV_TOOL_CALL steps and asks the call's files
//! for each request a step makes, the function an `RvToolCallArgs`
//! carries to end the call, and the sender of RV_TOOL_CANCEL.

use std::ffi::c_void;
use std::future::Future;
use std::pin::Pin;
use std::ptr;
use std::task::{Context, Poll};

use runvil_plugin::{
    BoxFuture, Error, ErrorCode, ToolFiles, ToolOutput, VfsError, VfsOp, VfsOutcome,
    make_tool_error,
};
use runvil_plugin_sys::{
    RV_MAX_RESULT, RV_OK, RV_PENDING, RV_RESULT_BACKEND, RV_RESULT_INVALID_ARGUMENTS,
    RV_RESULT_OTHER, RV_RESULT_TRANSPORT, RV_RESULT_TRUSTED, RV_RESULT_UNTRUSTED, RV_TOOL_CALL,
    RV_TOOL_CANCEL, RvCall, RvStatus, RvToolCallArgs, RvToolResult,
};
use serde_json::Value;

use crate::file::{
    Answer, FINISH_AFTER_REQUEST, Wanted, file_append, file_copy, file_list, file_mkdir, file_read,
    file_remove, file_rename, file_stat, file_write,
};
use crate::pack::EntryPack;
use crate::session::{
    self, Field, Session, Shape, TOOL_CALL, Verb, answer, field, rv_str, shape, size_field,
};

#[cfg(test)]
#[path = "call-tests.rs"]
mod tests;

const RESULT: Shape = shape!("finish", "result", "RvToolResult");
const TEXT: Field = field!("the result's text", 16_777_216, RV_MAX_RESULT);

/// One tool call in flight. A poll sends at most one RV_TOOL_CALL step.
///
/// When a step made a file request, the future asks the call's files for
/// it and then waits for the outcome without waking itself: the files
/// wake its task when the outcome is there, and the poll after that
/// sends the step that is lent the answer. A step that returns
/// RV_PENDING with no request wakes the future's own task and leaves it
/// pending, so the Host's executor gives the run's other work a turn and
/// then polls again. Dropping the future while its last step returned
/// RV_PENDING sends RV_TOOL_CANCEL, so the Plugin frees the call's state.
///
/// It borrows the Pack instance, so the instance is deleted only after
/// every one of its calls has ended.
pub(crate) struct Steps<'a> {
    pack: &'a EntryPack,
    /// The call's files, which each request of a step is asked of.
    files: &'a dyn ToolFiles,
    /// The tool's name within its Pack.
    tool: String,
    /// The JSON text of the call's arguments.
    arguments: String,
    /// What the last step left in `call->tool_call`, which the loader
    /// passes back and never reads through.
    state: *mut c_void,
    /// Whether the last step returned RV_PENDING, so the Plugin still
    /// holds state for the call.
    pending: bool,
    /// Whether the call has ended.
    ended: bool,
    /// The request the last step made, while the call waits for its
    /// outcome.
    request: Option<Asked<'a>>,
    /// The answer to the request the last step made, which the next step
    /// is lent.
    answer: Option<Answer>,
}

/// A request the call's files were asked for: what it asks for, and the
/// future of its outcome.
struct Asked<'a> {
    wanted: Wanted,
    outcome: BoxFuture<'a, Result<VfsOutcome, VfsError>>,
}

/// What one step left of its call.
enum Stepped {
    /// The call ended, with this result.
    Ended(Result<ToolOutput, Error>),
    /// The step returned RV_PENDING after it made this file request.
    Asked(VfsOp),
    /// The step returned RV_PENDING and made no request.
    Waits,
}

// SAFETY: the `# Threads and lifetime` section of `runvil-plugin-sys` lets
// the steps of one tool call, and its RV_TOOL_CANCEL, come on different
// threads, and the loader never reads through `state`. Every other field
// is `Send`, which the assertion below checks for the two borrows, the
// request, and the answer.
unsafe impl Send for Steps<'_> {}

const _: () = {
    const fn is_send<T: Send>() {}
    is_send::<(&EntryPack, &dyn ToolFiles, Asked<'_>, Answer)>();
};

impl<'a> Steps<'a> {
    /// A call of the tool named `tool` on `pack` with `arguments`, whose
    /// file requests are asked of `files`. Nothing is sent until the
    /// first poll.
    pub(crate) fn new(
        pack: &'a EntryPack,
        tool: &str,
        arguments: &Value,
        files: &'a dyn ToolFiles,
    ) -> Steps<'a> {
        Steps {
            pack,
            files,
            tool: tool.to_owned(),
            arguments: arguments.to_string(),
            state: ptr::null_mut(),
            pending: false,
            ended: false,
            request: None,
            answer: None,
        }
    }

    /// An `RvCall` that sends `verb` for this call.
    fn new_call(&self, verb: u32) -> RvCall {
        RvCall {
            tool_call: self.state,
            ..self.pack.new_call(verb)
        }
    }

    /// Sends one RV_TOOL_CALL step, which is lent the answer to the
    /// request the last step made, and returns what the step left of the
    /// call. The request of a step that ended the call is not performed.
    fn step(&mut self) -> Stepped {
        let mut call = self.new_call(RV_TOOL_CALL);
        // The answer and what it points to stay valid until this function
        // returns, which is after the step has.
        let answer = self.answer.take();
        let lent = answer.as_ref().map(Answer::raw);
        let (status, mut session) = session::open(Verb::ToolCall, |ctx| {
            let args = RvToolCallArgs {
                size: size_field::<RvToolCallArgs>(),
                reserved: 0,
                ctx,
                tool: rv_str(&self.tool),
                arguments: rv_str(&self.arguments),
                finish,
                file_stat,
                file_list,
                file_read,
                answer: lent.as_ref().map_or(ptr::null(), ptr::from_ref),
                file_write,
                file_append,
                file_mkdir,
                file_remove,
                file_rename,
                file_copy,
            };
            call.args = (&raw const args).cast();
            // SAFETY: `call` carries this call's own pointers and a live
            // `RvToolCallArgs`, whose `answer` is NULL or a live
            // `RvFileAnswer`, and no other step of the call runs, because
            // a step needs `&mut self`.
            unsafe { self.pack.send(&mut call) }
        });
        self.state = call.tool_call;
        self.pending = status == RV_PENDING;
        let request = session.request.take();
        match (judge(status, &mut session), request) {
            (Some(result), _) => {
                self.end();
                Stepped::Ended(result)
            }
            (None, Some(op)) => Stepped::Asked(op),
            (None, None) => Stepped::Waits,
        }
    }

    /// Asks the call's files for `op`, the request the last step made,
    /// and polls the request at once, because a request is made at the
    /// first poll of its future. A request whose outcome is there
    /// already wakes the task for the step that is lent its answer.
    fn ask(&mut self, op: VfsOp, cx: &mut Context<'_>) {
        let files: &'a dyn ToolFiles = self.files;
        let wanted = Wanted::by(&op);
        let mut outcome = files.request(op);
        match outcome.as_mut().poll(cx) {
            Poll::Ready(outcome) => {
                self.answer = Some(Answer::to(wanted, outcome));
                cx.waker().wake_by_ref();
            }
            Poll::Pending => self.request = Some(Asked { wanted, outcome }),
        }
    }

    /// Ends the call: sends RV_TOOL_CANCEL when its last step returned
    /// RV_PENDING, so the Plugin frees the state it still holds.
    fn end(&mut self) {
        self.ended = true;
        if !std::mem::take(&mut self.pending) {
            return;
        }
        let mut call = self.new_call(RV_TOOL_CANCEL);
        // SAFETY: the call's last step returned RV_PENDING and no step
        // runs, so this is the one cancel the contract allows it.
        let _status = session::closed(|| unsafe { self.pack.send(&mut call) });
    }
}

impl Future for Steps<'_> {
    type Output = Result<ToolOutput, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.ended {
            return Poll::Ready(Err(failure(
                "the tool call was polled after it ended".to_owned(),
            )));
        }
        if let Some(asked) = this.request.as_mut() {
            // The contract sends a call no step while its request waits
            // for an answer. The call's files wake this task when the
            // outcome is there, so the future wakes nothing itself.
            let Poll::Ready(outcome) = asked.outcome.as_mut().poll(cx) else {
                return Poll::Pending;
            };
            this.answer = Some(Answer::to(asked.wanted, outcome));
            this.request = None;
        }
        match this.step() {
            Stepped::Ended(result) => Poll::Ready(result),
            Stepped::Asked(op) => {
                this.ask(op, cx);
                Poll::Pending
            }
            Stepped::Waits => {
                // The contract has no wake function: a pending call that
                // asked for nothing gets its next step as soon as the
                // executor polls again.
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

impl Drop for Steps<'_> {
    fn drop(&mut self) {
        self.end();
    }
}

/// A failed call that is the Plugin's fault or the loader's, with
/// `message` as the text a model reads.
fn failure(message: String) -> Error {
    make_tool_error(ErrorCode::ToolOther, message)
}

/// What a step that returned `status` means: `None` for a call that needs
/// another step, and otherwise how the call ended. The checks come in
/// this order: a misuse, then a result beside RV_PENDING, then a status
/// other than RV_OK, then no result.
fn judge(status: RvStatus, session: &mut Session) -> Option<Result<ToolOutput, Error>> {
    if let Some(misuse) = session.misuse.take() {
        return Some(Err(failure(format!(
            "the tool's Plugin misused the tool call: {misuse}"
        ))));
    }
    match (status, session.result.take()) {
        (RV_PENDING, None) => None,
        (RV_PENDING, Some(_)) => Some(Err(failure(
            "the tool's Plugin misused the tool call: a step called finish and then returned \
             RV_PENDING"
                .to_owned(),
        ))),
        (RV_OK, Some(result)) => Some(result),
        (RV_OK, None) => Some(Err(failure(
            "the tool's Plugin answered RV_TOOL_CALL with RV_OK without finishing the call"
                .to_owned(),
        ))),
        (status, _) => Some(Err(failure(format!(
            "the tool's Plugin answered RV_TOOL_CALL with status {status}"
        )))),
    }
}

/// Records the result a step ends its call with, validated; returns RV_OK
/// or RV_ERR_MISUSE.
pub(crate) unsafe extern "C" fn finish(ctx: *mut c_void, result: *const RvToolResult) -> RvStatus {
    answer(ctx, &TOOL_CALL, |session| {
        if session.result.is_some() {
            return Err("finish was called twice");
        }
        if session.request.is_some() {
            return Err(FINISH_AFTER_REQUEST);
        }
        // SAFETY: the contract makes `result` point at a readable
        // `RvToolResult` during this call.
        let result = unsafe { RESULT.read(result) }?;
        let code = match result.kind {
            RV_RESULT_UNTRUSTED | RV_RESULT_TRUSTED => None,
            RV_RESULT_INVALID_ARGUMENTS => Some(ErrorCode::ToolInvalidArguments),
            RV_RESULT_BACKEND => Some(ErrorCode::ToolBackend),
            RV_RESULT_TRANSPORT => Some(ErrorCode::ToolTransport),
            RV_RESULT_OTHER => Some(ErrorCode::ToolOther),
            _ => return Err("the result's kind is not one of the RV_RESULT constants"),
        };
        // SAFETY: the contract makes the text valid for its length.
        let text = unsafe { TEXT.read(result.text) }?;
        session.result = Some(match code {
            Some(code) => Err(make_tool_error(code, text)),
            None if result.kind == RV_RESULT_TRUSTED => Ok(ToolOutput::trusted(text)),
            None => Ok(ToolOutput::untrusted(text)),
        });
        Ok(RV_OK)
    })
}
