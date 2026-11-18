//! Tests for a tool call's desk: a request joins its desk at its future's
//! first poll and announces its call, an outcome wakes and resolves the
//! oldest request the loop took, a dropped future's request is never
//! reported or has its outcome discarded, and the run's queue wakes the
//! loop.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Wake;

use runvil_engine::{Prompt, Run, RunParams, RunSeed, Step, Timestamp};

use super::*;

/// Counts the times it is woken.
#[derive(Default)]
struct Woken(AtomicUsize);

impl Wake for Woken {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl Woken {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// A waker that counts on `woken`.
fn waker_of(woken: &Arc<Woken>) -> Waker {
    Waker::from(Arc::clone(woken))
}

/// An effect id to open a desk under. Only a run makes an `EffectId`, so
/// this is the id of the one effect a small run issues.
fn call_id() -> EffectId {
    let source = "---\nname: t\ndescription: d\nrunvil: 0\n---\n\n\
        ## Only\n\n```lua\nreturn store.exists('x')\n```\n";
    let prompt = Prompt::parse(&|_: &str| Ok(source.to_owned()), "prompt")
        .expect("the fixture prompt parses");
    let params = RunParams::new(RunSeed::from_bytes([7; 32]), Timestamp::UNIX_EPOCH);
    let mut run = Run::new(Arc::new(prompt), "{}", params);
    let Step::Pending { effects, .. } = run.step() else {
        panic!("the fixture run issues one effect");
    };
    let [issued] = effects.as_slice() else {
        panic!("the fixture run issues one effect: {effects:?}");
    };
    issued.id
}

/// A write of `text` to `path`.
fn write(path: &str, text: &str) -> VfsOp {
    VfsOp::Write {
        path: path.to_owned(),
        contents: text.as_bytes().to_vec(),
    }
}

/// Polls `request` once with a waker that counts on `woken`.
fn poll(
    request: &mut BoxFuture<'_, Result<VfsOutcome, VfsError>>,
    woken: &Arc<Woken>,
) -> Poll<Result<VfsOutcome, VfsError>> {
    request
        .as_mut()
        .poll(&mut Context::from_waker(&waker_of(woken)))
}

/// Whether the run's queue holds a call now.
fn announced(asked: &Asked) -> Option<EffectId> {
    match asked.poll_next(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(call) => Some(call),
        Poll::Pending => None,
    }
}

#[test]
fn a_request_joins_its_desk_at_its_first_poll_and_resolves_with_the_outcome_the_loop_delivers() {
    let asked = Arc::new(Asked::default());
    let call = call_id();
    let desk = CallDesk::open(call, &asked);
    let woken = Arc::new(Woken::default());

    let mut request = desk.request(write("/a.md", "a"));
    assert!(
        desk.next_request().is_none(),
        "an unpolled request is not at the desk"
    );
    assert_eq!(announced(&asked), None);

    assert!(poll(&mut request, &woken).is_pending());
    assert_eq!(announced(&asked), Some(call), "the call is announced once");
    assert_eq!(announced(&asked), None);
    assert_eq!(desk.next_request(), Some(write("/a.md", "a")));
    assert!(desk.next_request().is_none());
    assert!(poll(&mut request, &woken).is_pending(), "no outcome yet");
    assert_eq!(woken.count(), 0);

    desk.deliver(Ok(VfsOutcome::Unit));
    assert_eq!(woken.count(), 1, "the outcome wakes the request's future");
    assert_eq!(
        poll(&mut request, &woken),
        Poll::Ready(Ok(VfsOutcome::Unit))
    );
    assert!(
        matches!(
            poll(&mut request, &woken),
            Poll::Ready(Err(VfsError::Backend { .. }))
        ),
        "a resolved request has no second outcome"
    );
}

#[test]
fn the_requests_of_one_call_resolve_in_the_order_they_went_out() {
    let asked = Arc::new(Asked::default());
    let call = call_id();
    let desk = CallDesk::open(call, &asked);
    let woken = Arc::new(Woken::default());
    let mut first = desk.request(write("/first.md", "1"));
    let mut second = desk.request(write("/second.md", "2"));
    // The second future is polled first, so its request is the older.
    assert!(poll(&mut second, &woken).is_pending());
    assert!(poll(&mut first, &woken).is_pending());
    assert_eq!(announced(&asked), Some(call));
    assert_eq!(announced(&asked), Some(call), "one entry for each request");

    assert_eq!(desk.next_request(), Some(write("/second.md", "2")));
    assert_eq!(desk.next_request(), Some(write("/first.md", "1")));
    let missing = VfsError::NotFound {
        path: "/second.md".to_owned(),
    };
    desk.deliver(Err(missing.clone()));
    assert!(poll(&mut first, &woken).is_pending(), "its outcome is next");
    assert_eq!(poll(&mut second, &woken), Poll::Ready(Err(missing)));
    desk.deliver(Ok(VfsOutcome::Bool(true)));
    assert_eq!(
        poll(&mut first, &woken),
        Poll::Ready(Ok(VfsOutcome::Bool(true)))
    );
}

#[test]
fn a_request_whose_future_is_dropped_before_the_loop_takes_it_is_never_reported() {
    let asked = Arc::new(Asked::default());
    let call = call_id();
    let desk = CallDesk::open(call, &asked);
    let woken = Arc::new(Woken::default());
    let mut dropped = desk.request(write("/dropped.md", "x"));
    let mut kept = desk.request(write("/kept.md", "y"));
    assert!(poll(&mut dropped, &woken).is_pending());
    assert!(poll(&mut kept, &woken).is_pending());
    drop(dropped);

    // The run's queue still holds the dropped request's entry, and the
    // desk hands the loop the request that is left.
    assert_eq!(announced(&asked), Some(call));
    assert_eq!(desk.next_request(), Some(write("/kept.md", "y")));
    assert_eq!(announced(&asked), Some(call));
    assert!(desk.next_request().is_none(), "the dropped request is gone");
    desk.deliver(Ok(VfsOutcome::Unit));
    assert_eq!(poll(&mut kept, &woken), Poll::Ready(Ok(VfsOutcome::Unit)));
}

#[test]
fn the_outcome_of_a_request_whose_future_is_dropped_after_the_loop_took_it_is_discarded() {
    let asked = Arc::new(Asked::default());
    let desk = CallDesk::open(call_id(), &asked);
    let woken = Arc::new(Woken::default());
    let mut dropped = desk.request(write("/dropped.md", "x"));
    let mut kept = desk.request(write("/kept.md", "y"));
    assert!(poll(&mut dropped, &woken).is_pending());
    assert!(poll(&mut kept, &woken).is_pending());
    assert_eq!(desk.next_request(), Some(write("/dropped.md", "x")));
    assert_eq!(desk.next_request(), Some(write("/kept.md", "y")));
    drop(dropped);

    // The operation still happened. Its outcome reaches no one, and the
    // next outcome is the kept request's.
    desk.deliver(Ok(VfsOutcome::Bool(false)));
    assert_eq!(woken.count(), 0, "no future waits on the first outcome");
    assert!(poll(&mut kept, &woken).is_pending());
    desk.deliver(Ok(VfsOutcome::Unit));
    assert_eq!(woken.count(), 1);
    assert_eq!(poll(&mut kept, &woken), Poll::Ready(Ok(VfsOutcome::Unit)));
    desk.deliver(Ok(VfsOutcome::Unit));
    assert_eq!(
        woken.count(),
        1,
        "an outcome no request is owed wakes no one"
    );
}

#[test]
fn a_request_wakes_a_loop_that_waits_on_the_empty_queue() {
    let asked = Arc::new(Asked::default());
    let call = call_id();
    let desk = CallDesk::open(call, &asked);
    let loop_woken = Arc::new(Woken::default());
    assert!(
        asked
            .poll_next(&mut Context::from_waker(&waker_of(&loop_woken)))
            .is_pending()
    );

    let mut request = desk.request(write("/a.md", "a"));
    assert_eq!(loop_woken.count(), 0, "an unpolled request wakes no one");
    assert!(poll(&mut request, &Arc::new(Woken::default())).is_pending());
    assert_eq!(loop_woken.count(), 1);
    assert_eq!(announced(&asked), Some(call));
}
