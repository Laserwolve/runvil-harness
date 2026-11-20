//! Tests for when a tool call parks and what each wake of its Plugin
//! does: a wake during a step, a wake while the call is parked, on the
//! loader's thread and on another, a wake while a file request waits, a
//! wake inside RV_TOOL_CANCEL and inside RV_PLUGIN_UNLOAD, and a wake
//! after the call ended. They also show that one call holds a slot at a
//! time, that a slot is taken again once its call gave it back, and that
//! a call nothing wakes fails its test and does not hang it.

use std::ffi::c_void;
use std::future::Future;
use std::pin::pin;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use runvil_plugin::VfsOutcome;
use serde_json::json;

use crate::fakes::*;
use crate::park::Parked;
use crate::resources::HostResources;

/// A weak handle on the slot behind `ctx`, the `ctx` of a waker a step
/// was lent. It shows whether the slot is alive and keeps it alive no
/// longer.
fn slot_behind(ctx: *mut c_void) -> Weak<Parked> {
    let slot = ctx.cast::<Parked>().cast_const();
    // SAFETY: the `ctx` of a waker the loader lends a step is the
    // `Arc::as_ptr` of a slot that the step's Plugin holds, and the step
    // is running, so that `Arc` is alive. The count is raised by one for
    // the `Arc` made here, which drops when this function returns.
    let held = unsafe {
        Arc::increment_strong_count(slot);
        Arc::from_raw(slot)
    };
    Arc::downgrade(&held)
}

#[test]
fn a_wake_during_a_step_is_seen_as_the_call_parks_and_wakes_its_task_once() {
    pack_fake!(
        plug,
        {
            pub(super) static STEPS: AtomicUsize = AtomicUsize::new(0);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            if STEPS.fetch_add(1, Ordering::SeqCst) < 2 {
                wake_call(call);
                // Wakes do not add up, however many come in one step.
                wake_call(call);
                return RV_PENDING;
            }
            finish(call, RV_RESULT_TRUSTED, "done")
        }
    );
    unblocked("a step that wakes its own call", || {
        let pack = made(plug::entry);
        let call = test_call("tool");
        let mut future = pin!(pack.call(call.context(), json!({})));
        let (woken, waker) = Wakes::new();
        let mut cx = Context::from_waker(&waker);

        assert!(future.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            woken.count(),
            1,
            "the wakes of the first step wake the task once, as the call parks"
        );
        assert!(future.as_mut().poll(&mut cx).is_pending());
        assert_eq!(woken.count(), 2);
        let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
            panic!("the third step finishes the call");
        };
        assert_eq!(result.expect("the call succeeds").text(), "done");
        assert_eq!(
            woken.count(),
            2,
            "the step that ends the call wakes nothing"
        );
        assert_eq!(plug::STEPS.load(Ordering::SeqCst), 3);
        assert_eq!(plug::CANCELS.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn a_step_that_returns_pending_and_wakes_nothing_parks_its_call() {
    pack_fake!(
        plug,
        {
            pub(super) static STEPS: AtomicUsize = AtomicUsize::new(0);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |_call| {
            STEPS.fetch_add(1, Ordering::SeqCst);
            RV_PENDING
        }
    );
    let pack = made(plug::entry);
    let call = test_call("tool");
    let mut future = pin!(pack.call(call.context(), json!({})));
    let (woken, waker) = Wakes::new();
    let mut cx = Context::from_waker(&waker);

    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(
        woken.count(),
        0,
        "a call that parks does not wake its own task"
    );
    assert_eq!(plug::STEPS.load(Ordering::SeqCst), 1);

    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(
        plug::STEPS.load(Ordering::SeqCst),
        2,
        "a poll that no wake asked for still sends a step"
    );
    assert_eq!(woken.count(), 0);
}

#[test]
#[should_panic(expected = "nothing woke its task within 50ms")]
fn a_call_that_parks_and_is_never_woken_fails_its_test_at_the_limit() {
    pack_fake!(
        plug,
        {
            pub(super) static STEPS: AtomicUsize = AtomicUsize::new(0);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            // A call that is stepped with no wake ends at its hundredth
            // step, so the wait below then returns, and this test fails
            // for the panic it did not get.
            if STEPS.fetch_add(1, Ordering::SeqCst) < 99 {
                return RV_PENDING;
            }
            finish(call, RV_RESULT_TRUSTED, "stepped though nothing woke it")
        }
    );
    let pack = made(plug::entry);
    let call = test_call("tool");
    let _never = block_on_within(
        pack.call(call.context(), json!({})),
        Duration::from_millis(50),
    );
}

#[test]
fn a_wake_while_a_call_is_parked_wakes_its_task_and_the_step_it_asked_for_forgets_it() {
    pack_fake!(
        plug,
        {
            pub(super) static KEPT: Kept = Kept::new();
            pub(super) static STEPS: AtomicUsize = AtomicUsize::new(0);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            KEPT.keep(call);
            // The third step finishes, and no step wakes its call.
            if STEPS.fetch_add(1, Ordering::SeqCst) < 2 {
                return RV_PENDING;
            }
            finish(call, RV_RESULT_TRUSTED, "done")
        }
    );
    let pack = made(plug::entry);
    let call = test_call("tool");
    let mut future = pin!(pack.call(call.context(), json!({})));
    let (woken, waker) = Wakes::new();
    let mut cx = Context::from_waker(&waker);

    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(woken.count(), 0, "the call is parked");
    assert!(plug::KEPT.wake());
    assert_eq!(
        woken.count(),
        1,
        "the wake wakes the task of the parked call"
    );
    assert!(plug::KEPT.wake());
    assert_eq!(woken.count(), 1, "wakes do not add up");
    assert_eq!(
        plug::STEPS.load(Ordering::SeqCst),
        1,
        "a wake sends no step by itself"
    );

    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(plug::STEPS.load(Ordering::SeqCst), 2);
    assert_eq!(
        woken.count(),
        1,
        "the second step answered both wakes, so the call parks again and is not woken"
    );

    assert!(plug::KEPT.wake());
    assert_eq!(woken.count(), 2);
    let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
        panic!("the third step finishes the call");
    };
    assert_eq!(result.expect("the call succeeds").text(), "done");
}

#[test]
fn a_parked_call_gets_its_next_step_when_another_thread_wakes_it() {
    pack_fake!(
        plug,
        {
            pub(super) static KEPT: Kept = Kept::new();
            pub(super) static STEPS: AtomicUsize = AtomicUsize::new(0);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            if STEPS.fetch_add(1, Ordering::SeqCst) == 0 {
                KEPT.keep(call);
                return RV_PENDING;
            }
            finish(call, RV_RESULT_TRUSTED, "done")
        }
    );
    let pack = made(plug::entry);
    let call = test_call("tool");
    let mut future = pin!(pack.call(call.context(), json!({})));
    let (woken, waker) = Wakes::new();
    let mut cx = Context::from_waker(&waker);

    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(woken.count(), 0, "the call is parked");
    let woke = thread::spawn(|| plug::KEPT.wake())
        .join()
        .expect("the waking thread finishes");
    assert!(woke, "the thread woke through the waker the step kept");
    assert_eq!(
        woken.count(),
        1,
        "a wake on another thread wakes the task of the parked call"
    );
    assert_eq!(plug::STEPS.load(Ordering::SeqCst), 1);
    let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
        panic!("the second step finishes the call");
    };
    assert_eq!(result.expect("the call succeeds").text(), "done");
    assert_eq!(plug::STEPS.load(Ordering::SeqCst), 2);
}

#[test]
fn a_call_stays_parked_until_a_thread_of_its_plugin_wakes_it() {
    pack_fake!(
        plug,
        {
            pub(super) static KEPT: Kept = Kept::new();
            /// 1 once the waking thread has come to its wake.
            pub(super) static RELEASED: AtomicUsize = AtomicUsize::new(0);
            /// The thread that wakes the call, which the test joins.
            pub(super) static WAKING: Mutex<Option<std::thread::JoinHandle<()>>> =
                Mutex::new(None);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            if tool_call(call).is_null() {
                set_tool_call(call, instance(0x300));
                KEPT.keep(call);
                let waking = std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    RELEASED.store(1, Ordering::SeqCst);
                    KEPT.wake();
                });
                if let Ok(mut handle) = WAKING.lock() {
                    *handle = Some(waking);
                }
                return RV_PENDING;
            }
            let said = if RELEASED.load(Ordering::SeqCst) == 1 {
                "woken"
            } else {
                "stepped before the wake"
            };
            finish(call, RV_RESULT_TRUSTED, said)
        }
    );
    let pack = made(plug::entry);
    // `call_tool` waits for a wake after the first step, and the tool
    // says whether its wake had come when the second step was sent.
    let output = call_tool(&*pack, "tool", json!({})).expect("the call succeeds");
    assert_eq!(output.text(), "woken");
    // A Plugin stops each of its threads before it is unloaded.
    let waking = plug::WAKING.lock().expect("the lock holds").take();
    waking
        .expect("the first step started the thread")
        .join()
        .expect("the waking thread finishes");
}

#[test]
fn a_wake_while_a_file_request_waits_sends_no_step_and_the_answer_brings_the_next_one() {
    pack_fake!(
        plug,
        {
            pub(super) static KEPT: Kept = Kept::new();
            pub(super) static STEPS: AtomicUsize = AtomicUsize::new(0);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            if STEPS.fetch_add(1, Ordering::SeqCst) == 0 {
                KEPT.keep(call);
                // A wake and a request in one step.
                wake_call(call);
                ask_read(call, "/notes/a.md", (2, 3));
                return RV_PENDING;
            }
            finish(call, RV_RESULT_UNTRUSTED, &format!("{:?}", read_answer(call)))
        }
    );
    let pack = made(plug::entry);
    let call = test_call("tool");
    let files = HeldFiles::default();
    let mut future = pin!(pack.call(context_over(&call, &files), json!({})));
    let (woken, waker) = Wakes::new();
    let mut cx = Context::from_waker(&waker);

    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(files.asked().len(), 1, "the step's request was made");
    assert_eq!(
        woken.count(),
        0,
        "the wake of a step that made a request wakes no task"
    );

    assert!(plug::KEPT.wake());
    let woke = thread::spawn(|| plug::KEPT.wake())
        .join()
        .expect("the waking thread finishes");
    assert!(woke);
    assert_eq!(
        woken.count(),
        0,
        "a wake while the request waits wakes no task either"
    );
    // The task is polled all the same, as anything else may wake it.
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(
        plug::STEPS.load(Ordering::SeqCst),
        1,
        "no step is sent while the request waits for its answer, whatever woke the task"
    );

    files.answer(0, Ok(VfsOutcome::Bytes(b"llo".to_vec())));
    assert_eq!(woken.count(), 1, "the answer wakes the task");
    let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
        panic!("the step that is lent the answer finishes the call");
    };
    assert_eq!(
        result.expect("the call succeeds").text(),
        format!("Some(Ok({:?}))", b"llo")
    );
    assert_eq!(plug::STEPS.load(Ordering::SeqCst), 2);
    assert_eq!(files.asked().len(), 1, "the call asked once");
}

#[test]
fn a_wake_inside_the_cancel_returns_and_wakes_no_task() {
    pack_fake!(
        plug,
        {
            pub(super) static KEPT: Kept = Kept::new();
            /// 1 once the cancel woke through the kept waker.
            pub(super) static WOKE_IN_CANCEL: AtomicUsize = AtomicUsize::new(0);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            KEPT.keep(call);
            set_tool_call(call, instance(0x300));
            RV_PENDING
        },
        // A Plugin wakes a call as it frees the call's state, from a
        // destructor, on the thread that sent the cancel.
        cancel: |_call| WOKE_IN_CANCEL.store(usize::from(KEPT.wake()), Ordering::SeqCst)
    );
    unblocked("the cancel of a call whose Plugin wakes it", || {
        let pack = made(plug::entry);
        let call = test_call("tool");
        let mut future = Box::pin(pack.call(call.context(), json!({})));
        let (woken, waker) = Wakes::new();
        let mut cx = Context::from_waker(&waker);
        assert!(future.as_mut().poll(&mut cx).is_pending());
        assert_eq!(woken.count(), 0, "the call is parked");

        drop(future);
        assert_eq!(plug::CANCELS.load(Ordering::SeqCst), 1);
        assert_eq!(
            plug::WOKE_IN_CANCEL.load(Ordering::SeqCst),
            1,
            "the cancel woke through the call's waker"
        );
        assert_eq!(
            woken.count(),
            0,
            "a wake inside the cancel wakes no task: the call has ended"
        );

        // The slot went back clean, so the wake is not the next call's.
        let next = test_call("tool");
        let mut future = Box::pin(pack.call(next.context(), json!({})));
        assert!(future.as_mut().poll(&mut cx).is_pending());
        assert_eq!(woken.count(), 0, "the next call parks and is not woken");
    });
}

#[test]
fn the_slots_outlive_the_unload_so_a_wake_inside_it_reaches_live_memory() {
    pack_fake!(
        plug,
        {
            pub(super) static KEPT: Kept = Kept::new();
            /// The slot behind the waker the step was lent.
            pub(super) static SLOT: Mutex<Option<std::sync::Weak<crate::park::Parked>>> =
                Mutex::new(None);
            /// Inside RV_PLUGIN_UNLOAD: 1 when that slot was alive, and 2
            /// when it was freed already.
            pub(super) static ALIVE_IN_UNLOAD: AtomicUsize = AtomicUsize::new(0);
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "tool")
        },
        step: |call| {
            KEPT.keep(call);
            if let Ok(mut slot) = SLOT.lock() {
                *slot = Some(super::slot_behind(step_args(call).waker.ctx));
            }
            finish(call, RV_RESULT_TRUSTED, "done")
        },
        // A Plugin may wake until its unload returns. A slot that was
        // freed by then is not woken through, so a failure of this test
        // reads no freed memory.
        unload: |_call| {
            let alive = SLOT
                .lock()
                .is_ok_and(|slot| slot.as_ref().is_some_and(|slot| slot.strong_count() > 0));
            ALIVE_IN_UNLOAD.store(if alive { 1 } else { 2 }, Ordering::SeqCst);
            if alive {
                KEPT.wake();
            }
        }
    );
    unblocked("the unload of a Plugin that wakes a call", || {
        let (plugin, pack) = made_with(plug::entry, &HostResources::new());
        call_tool(&*pack, "tool", json!({})).expect("the call succeeds");
        let slot = plug::SLOT
            .lock()
            .expect("the lock holds")
            .clone()
            .expect("the step kept a handle on its slot");

        // The call ended and its Pack instance is deleted, and the Plugin
        // is still loaded.
        drop(pack);
        assert_eq!(plug::DELETES.load(Ordering::SeqCst), 1);
        assert!(slot.strong_count() > 0, "the Plugin keeps the slot");
        assert!(plug::KEPT.wake(), "a late wake reaches live memory");

        drop(plugin);
        assert_eq!(plug::UNLOADS.load(Ordering::SeqCst), 1);
        assert_eq!(
            plug::ALIVE_IN_UNLOAD.load(Ordering::SeqCst),
            1,
            "the slot was alive while RV_PLUGIN_UNLOAD ran"
        );
        assert_eq!(
            slot.strong_count(),
            0,
            "the slot was freed once the unload had returned"
        );
    });
}

#[test]
fn a_wake_after_its_call_ended_is_harmless_and_costs_the_next_call_in_the_slot_one_step() {
    pack_fake!(
        plug,
        {
            pub(super) static KEPT: Kept = Kept::new();
            /// How many steps the `parks` tool got.
            pub(super) static STEPS: AtomicUsize = AtomicUsize::new(0);
            /// The `ctx` of the waker the `parks` tool was lent.
            pub(super) static PARKS_CTX: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "keeps");
            add_tool(call, "parks")
        },
        step: |call| {
            if text(step_args(call).tool) == "keeps" {
                // Keeps its waker past its end, as a timer of a Plugin
                // fires after the call it was armed for.
                KEPT.keep(call);
                return finish(call, RV_RESULT_TRUSTED, "kept");
            }
            PARKS_CTX.store(step_args(call).waker.ctx, Ordering::SeqCst);
            // The second step finishes, and no step wakes its call.
            if STEPS.fetch_add(1, Ordering::SeqCst) == 0 {
                return RV_PENDING;
            }
            finish(call, RV_RESULT_TRUSTED, "done")
        }
    );
    let (plugin, pack) = made_with(plug::entry, &HostResources::new());
    let output = call_tool(&*pack, "keeps", json!({})).expect("the first call succeeds");
    assert_eq!(output.text(), "kept");
    assert!(
        plug::KEPT.wake(),
        "a wake after the call ended does nothing"
    );

    let call = test_call("parks");
    let mut future = pin!(pack.call(call.context(), json!({})));
    let (woken, waker) = Wakes::new();
    let mut cx = Context::from_waker(&waker);
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(
        plug::PARKS_CTX.load(Ordering::SeqCst),
        plug::KEPT.ctx(),
        "the later call holds the slot the first call held"
    );
    assert_eq!(
        woken.count(),
        0,
        "a late wake that came before the later call started is not that call's"
    );

    assert!(plug::KEPT.wake());
    assert_eq!(
        woken.count(),
        1,
        "a late wake reaches the call that holds the slot now"
    );
    let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
        panic!("the step the late wake cost finishes the later call");
    };
    assert_eq!(result.expect("the later call succeeds").text(), "done");
    assert_eq!(plug::STEPS.load(Ordering::SeqCst), 2);
    assert_eq!(plugin.slots().made(), 1);
}

#[test]
fn calls_in_a_row_take_one_slot_and_calls_in_flight_at_once_hold_one_each() {
    pack_fake!(
        plug,
        {
            /// The `ctx` of the waker each step was lent, in order.
            pub(super) static CTXS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
        },
        new: |call| {
            set_pack_run(call, instance(PACK_TAG));
            add_tool(call, "quick");
            add_tool(call, "parks")
        },
        step: |call| {
            if let Ok(mut ctxs) = CTXS.lock() {
                ctxs.push(step_args(call).waker.ctx.addr());
            }
            if text(step_args(call).tool) == "parks" {
                return RV_PENDING;
            }
            finish(call, RV_RESULT_TRUSTED, "done")
        }
    );
    /// The `ctx` of each of the last `count` steps.
    fn last(count: usize) -> Vec<usize> {
        let ctxs = plug::CTXS.lock().expect("the lock holds");
        ctxs[ctxs.len() - count..].to_vec()
    }
    let (plugin, pack) = made_with(plug::entry, &HostResources::new());
    let mut cx = Context::from_waker(std::task::Waker::noop());

    for _ in 0..100 {
        call_tool(&*pack, "quick", json!({})).expect("the call succeeds");
    }
    assert_eq!(
        plugin.slots().made(),
        1,
        "a hundred calls in a row made one slot"
    );
    let one = last(100);
    assert!(one.iter().all(|ctx| *ctx == one[0]), "{one:?}");

    for round in 0..2 {
        let (first, second) = (test_call("parks"), test_call("parks"));
        let mut one_call = Box::pin(pack.call(first.context(), json!({})));
        let mut other_call = Box::pin(pack.call(second.context(), json!({})));
        assert!(one_call.as_mut().poll(&mut cx).is_pending());
        assert!(other_call.as_mut().poll(&mut cx).is_pending());
        let two = last(2);
        assert_ne!(
            two[0], two[1],
            "round {round}: two calls in flight at once hold a slot each"
        );
        assert_eq!(plugin.slots().made(), 2, "round {round}");
        drop((one_call, other_call));
        // Each of these calls ends in its step and is dropped after that,
        // and gives its slot back once.
        for _ in 0..10 {
            call_tool(&*pack, "quick", json!({})).expect("the call succeeds");
        }
        assert_eq!(plugin.slots().made(), 2, "round {round}");
    }
}
