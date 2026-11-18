//! A tool call's files at the effect loop: the desk through which one
//! call asks for file operations, and the run-wide queue of the calls
//! that asked.
//!
//! Each tool call gets a [`CallDesk`], which is the `ToolFiles` of the
//! call's `ToolContext`. A request joins the desk when its future is
//! first polled, and the call's id joins the run's [`Asked`] queue, which
//! wakes the effect loop. The loop takes the call's oldest request off
//! the desk and reports it to the run, and the run issues it as a `Vfs`
//! effect whose asker is the call. The loop performs that effect inline,
//! records its answer, and hands the whole outcome to the desk, which
//! wakes the request's future. The run issues the effects of one call in
//! the order the loop reported its requests, so the desk matches each
//! outcome to the oldest request it sent.
//!
//! A request whose future is dropped before the loop takes it is never
//! reported. One dropped later stays reported, so the loop still performs
//! it while its call is in flight, and the desk discards its outcome. A
//! desk lives as long as its call is in flight: the loop drops what a
//! call that has landed or was aborted still asks for.
//!
//! The queue also counts the requests the loop takes, and tells the loop
//! when it owes its executor a turn: once in every
//! [`REQUESTS_PER_TURN`] requests. The loop performs a request and wakes
//! its call without returning to the executor, so calls that keep asking
//! would otherwise hold the executor's thread for as long as they ask.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use runvil_engine::BoxFuture;
use runvil_engine::effect::EffectId;
use runvil_engine::vfs::{VfsError, VfsOp, VfsOutcome};
use runvil_plugin::ToolFiles;

/// How many file requests the effect loop takes before it gives its
/// executor a turn.
const REQUESTS_PER_TURN: u32 = 32;

/// The run-wide queue of the tool calls that asked for a file operation:
/// one entry for each request, oldest first.
#[derive(Default)]
pub(crate) struct Asked {
    queue: Mutex<AskedQueue>,
}

/// The calls that asked, the effect loop's waker, and the count of the
/// requests the loop took.
#[derive(Default)]
struct AskedQueue {
    /// The id of the call behind each request the loop has not taken.
    calls: VecDeque<EffectId>,
    /// The loop's waker, kept from the last poll that found no call.
    waker: Option<Waker>,
    /// The requests whose calls the loop took from the queue since it
    /// last gave its executor a turn.
    taken: u32,
}

impl Asked {
    /// Adds `call`, which made one more request, and wakes the loop.
    fn push(&self, call: EffectId) {
        let waker = {
            let mut queue = lock(&self.queue);
            queue.calls.push_back(call);
            queue.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Polls for the call behind the oldest request the loop has not
    /// taken. An empty queue keeps `cx`'s waker for the next request.
    pub(crate) fn poll_next(&self, cx: &mut Context<'_>) -> Poll<EffectId> {
        let mut queue = lock(&self.queue);
        if let Some(call) = queue.calls.pop_front() {
            return Poll::Ready(call);
        }
        queue.waker = Some(cx.waker().clone());
        Poll::Pending
    }

    /// Counts one request whose call the loop took from the queue, and
    /// says whether the loop owes its executor a turn before it reports
    /// that request: once in every [`REQUESTS_PER_TURN`] requests of the
    /// run.
    pub(crate) fn turn_due(&self) -> bool {
        let mut queue = lock(&self.queue);
        queue.taken += 1;
        if queue.taken < REQUESTS_PER_TURN {
            return false;
        }
        queue.taken = 0;
        true
    }
}

/// One tool call's desk: the requests the call made, from the first poll
/// of each one's future until that future takes its outcome or is
/// dropped.
pub(crate) struct CallDesk {
    /// The id of the call's `ToolCall` effect.
    call: EffectId,
    /// The run's queue of the calls that asked.
    asked: Arc<Asked>,
    requests: Mutex<Requests>,
}

/// The requests of one desk.
#[derive(Default)]
struct Requests {
    /// The ticket the next request takes.
    next: u64,
    /// The requests the loop has not taken, oldest first.
    queued: VecDeque<(u64, VfsOp)>,
    /// The requests the loop reported to the run, oldest first, each
    /// owed an outcome.
    sent: VecDeque<u64>,
    /// Each request a future still waits on: its waker until its outcome
    /// arrives, then the outcome.
    waiting: HashMap<u64, Waiting>,
}

/// What a request's future waits with.
enum Waiting {
    /// The waker of the future's last poll.
    For(Waker),
    /// The outcome, which the future's next poll takes.
    Answered(Result<VfsOutcome, VfsError>),
}

impl CallDesk {
    /// Opens the desk of tool call `call`, whose requests are announced
    /// on `asked`.
    pub(crate) fn open(call: EffectId, asked: &Arc<Asked>) -> Arc<CallDesk> {
        Arc::new(CallDesk {
            call,
            asked: Arc::clone(asked),
            requests: Mutex::default(),
        })
    }

    /// Takes the call's oldest request for the loop to report to the run,
    /// and notes that it is owed an outcome. `None` when every request
    /// was taken or dropped.
    pub(crate) fn next_request(&self) -> Option<VfsOp> {
        let mut requests = lock(&self.requests);
        let (ticket, op) = requests.queued.pop_front()?;
        requests.sent.push_back(ticket);
        Some(op)
    }

    /// Hands `outcome` to the oldest request the loop reported, and wakes
    /// the future that waits on it. The outcome of a request whose future
    /// was dropped is discarded.
    pub(crate) fn deliver(&self, outcome: Result<VfsOutcome, VfsError>) {
        let waker = {
            let mut requests = lock(&self.requests);
            let Some(ticket) = requests.sent.pop_front() else {
                return;
            };
            let Some(waiting) = requests.waiting.get_mut(&ticket) else {
                return;
            };
            match std::mem::replace(waiting, Waiting::Answered(outcome)) {
                Waiting::For(waker) => Some(waker),
                Waiting::Answered(_) => None,
            }
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Queues `op` as a new request, with `waker` to wake at its outcome,
    /// announces the call on the run's queue, and returns the request's
    /// ticket.
    fn ask(&self, op: VfsOp, waker: &Waker) -> u64 {
        let ticket = {
            let mut requests = lock(&self.requests);
            let ticket = requests.next;
            requests.next += 1;
            requests.queued.push_back((ticket, op));
            requests.waiting.insert(ticket, Waiting::For(waker.clone()));
            ticket
        };
        self.asked.push(self.call);
        ticket
    }

    /// Takes the outcome of request `ticket`, or keeps `waker` to wake at
    /// its arrival.
    fn take(&self, ticket: u64, waker: &Waker) -> Option<Result<VfsOutcome, VfsError>> {
        let mut requests = lock(&self.requests);
        match requests.waiting.remove(&ticket)? {
            Waiting::Answered(outcome) => Some(outcome),
            Waiting::For(_) => {
                requests.waiting.insert(ticket, Waiting::For(waker.clone()));
                None
            }
        }
    }

    /// Forgets request `ticket`, whose future was dropped: a request the
    /// loop has not taken leaves the queue, and the outcome of one it has
    /// taken has no one to reach.
    fn forget(&self, ticket: u64) {
        let mut requests = lock(&self.requests);
        requests.queued.retain(|(queued, _)| *queued != ticket);
        requests.waiting.remove(&ticket);
    }
}

impl ToolFiles for CallDesk {
    fn request(&self, op: VfsOp) -> BoxFuture<'_, Result<VfsOutcome, VfsError>> {
        Box::pin(Request {
            desk: self,
            stage: Stage::Unsent(op),
        })
    }
}

/// One request's future.
struct Request<'a> {
    desk: &'a CallDesk,
    stage: Stage,
}

/// How far one request has come.
enum Stage {
    /// Not polled yet: the operation to ask for.
    Unsent(VfsOp),
    /// At the desk under this ticket.
    Asked(u64),
    /// Resolved.
    Done,
}

impl Future for Request<'_> {
    type Output = Result<VfsOutcome, VfsError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let request = self.get_mut();
        match std::mem::replace(&mut request.stage, Stage::Done) {
            Stage::Unsent(op) => {
                request.stage = Stage::Asked(request.desk.ask(op, cx.waker()));
                Poll::Pending
            }
            Stage::Asked(ticket) => {
                if let Some(outcome) = request.desk.take(ticket, cx.waker()) {
                    return Poll::Ready(outcome);
                }
                request.stage = Stage::Asked(ticket);
                Poll::Pending
            }
            Stage::Done => Poll::Ready(Err(VfsError::Backend {
                message: "a file request was polled after it resolved".to_owned(),
            })),
        }
    }
}

impl Drop for Request<'_> {
    fn drop(&mut self) {
        if let Stage::Asked(ticket) = self.stage {
            self.desk.forget(ticket);
        }
    }
}

/// Poison-safe lock: a panic in a tool leaves the queues as they were,
/// and the loop still drains them.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "tool_files-tests.rs"]
mod tests;
