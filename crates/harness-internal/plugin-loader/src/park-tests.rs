//! Tests for the slot behind a tool call's waker and for the slots of one
//! Plugin: what a wake does before a call parks, while one is parked,
//! and once a step has begun, that no wake is lost when it races the
//! park, that `wake` never unwinds, and that the pool hands a slot out
//! again once its call gave it back and keeps each one until it drops.

use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::task::{Wake, Waker};
use std::thread;

use super::{Parked, Slots, wake};
use crate::fakes::{Wakes, wake_through};

#[test]
fn a_wake_that_comes_before_the_park_is_reported_by_the_park_and_wakes_no_task() {
    let slot = Arc::new(Parked::default());
    let (wakes, task) = Wakes::new();
    // A step begins, and the Plugin wakes the call during it, or after
    // it returned and before the call parks.
    slot.clear();
    wake_through(slot.waker());
    assert!(
        slot.park(&task),
        "the park reports the wake that came first"
    );
    assert_eq!(
        wakes.count(),
        0,
        "the park leaves waking the task to its caller"
    );
    assert!(slot.park(&task), "the wake stays noted until a step begins");
    slot.clear();
    assert!(!slot.park(&task), "a step that begins forgets the wake");
}

#[test]
fn a_wake_while_a_call_is_parked_wakes_its_task_and_more_wakes_wake_it_no_more() {
    let slot = Arc::new(Parked::default());
    let (wakes, task) = Wakes::new();
    assert!(!slot.park(&task), "nothing woke the call yet");
    assert_eq!(wakes.count(), 0);
    wake_through(slot.waker());
    assert_eq!(wakes.count(), 1, "the wake wakes the task that parked");
    wake_through(slot.waker());
    wake_through(slot.waker());
    assert_eq!(wakes.count(), 1, "wakes do not add up");
    assert!(slot.park(&task), "the wake is noted until a step begins");
}

#[test]
fn a_wake_from_another_thread_wakes_the_task_of_a_parked_call() {
    let slot = Arc::new(Parked::default());
    let (wakes, task) = Wakes::new();
    assert!(!slot.park(&task));
    let shared = Arc::clone(&slot);
    thread::spawn(move || wake_through(shared.waker()))
        .join()
        .expect("the thread finishes");
    assert_eq!(wakes.count(), 1);
}

#[test]
fn a_step_that_begins_forgets_the_task_that_was_parked() {
    let slot = Arc::new(Parked::default());
    let (wakes, task) = Wakes::new();
    assert!(!slot.park(&task));
    slot.clear();
    wake_through(slot.waker());
    assert_eq!(
        wakes.count(),
        0,
        "a wake during a step wakes no task: the park after the step reports it"
    );
    assert!(slot.park(&task));
}

#[test]
fn a_park_keeps_the_task_of_its_latest_call() {
    let slot = Arc::new(Parked::default());
    let (first_wakes, first) = Wakes::new();
    let (second_wakes, second) = Wakes::new();
    assert!(!slot.park(&first));
    assert!(!slot.park(&second));
    wake_through(slot.waker());
    assert_eq!((first_wakes.count(), second_wakes.count()), (0, 1));
}

#[test]
fn the_waker_of_a_slot_is_the_same_each_time_and_its_ctx_is_the_slot() {
    let slot = Arc::new(Parked::default());
    let other = Arc::new(Parked::default());
    assert_eq!(slot.waker().ctx, slot.waker().ctx);
    assert_eq!(slot.waker().ctx.cast_const().cast(), Arc::as_ptr(&slot));
    assert_ne!(slot.waker().ctx, other.waker().ctx);
}

#[test]
fn a_wake_through_a_null_ctx_reaches_no_slot() {
    // SAFETY: `wake` takes NULL.
    unsafe { wake(ptr::null_mut()) };
}

/// A task waker that panics when it is woken.
struct Panics;

impl Wake for Panics {
    fn wake(self: Arc<Self>) {
        panic!("the task's waker panicked");
    }
}

#[test]
fn a_panic_in_the_tasks_waker_ends_in_the_wake_and_the_wake_is_still_noted() {
    let slot = Arc::new(Parked::default());
    assert!(!slot.park(&Waker::from(Arc::new(Panics))));
    // An unwind out of `wake` would abort this process.
    wake_through(slot.waker());
    let (wakes, task) = Wakes::new();
    assert!(
        slot.park(&task),
        "the wake was noted before the task was woken"
    );
    assert_eq!(wakes.count(), 0);
}

/// How many times the race below is run.
const ROUNDS: usize = if cfg!(miri) { 40 } else { 20_000 };

#[test]
fn no_wake_is_lost_when_it_races_the_park() {
    let slot = Arc::new(Parked::default());
    let (wakes, task) = Wakes::new();
    // Both threads wait at `start`, so the wake and the park of a round
    // run at the same time, and at `done` before the round is judged.
    let start = Arc::new(Barrier::new(2));
    let done = Arc::new(Barrier::new(2));
    let reported = Arc::new(AtomicUsize::new(0));
    let waking = {
        let (slot, start, done) = (Arc::clone(&slot), Arc::clone(&start), Arc::clone(&done));
        thread::spawn(move || {
            for _ in 0..ROUNDS {
                start.wait();
                wake_through(slot.waker());
                done.wait();
            }
        })
    };
    for round in 0..ROUNDS {
        // A step begins and returns, and the call parks while the Plugin
        // wakes it from its own thread.
        slot.clear();
        let before = wakes.count();
        start.wait();
        let woken = slot.park(&task);
        done.wait();
        if woken {
            reported.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                wakes.count(),
                before,
                "round {round}: a wake the park reported woke no task"
            );
        } else {
            assert_eq!(
                wakes.count(),
                before + 1,
                "round {round}: a wake the park did not report woke the parked task"
            );
        }
    }
    waking.join().expect("the waking thread finishes");
    assert_eq!(
        reported.load(Ordering::SeqCst) + wakes.count(),
        ROUNDS,
        "each wake was reported by the park or woke the task, and none did both"
    );
}

#[test]
fn a_slot_that_was_given_back_is_taken_again_so_calls_in_a_row_make_one_slot() {
    let slots = Slots::default();
    assert_eq!(slots.made(), 0, "no call took a slot yet");
    let first = slots.take();
    slots.give_back(&first);
    for _ in 0..100 {
        let again = slots.take();
        assert!(Arc::ptr_eq(&again, &first));
        slots.give_back(&again);
    }
    assert_eq!(slots.made(), 1);
}

#[test]
fn calls_in_flight_at_once_each_hold_a_slot_of_their_own() {
    let slots = Slots::default();
    let (first, second) = (slots.take(), slots.take());
    assert!(!Arc::ptr_eq(&first, &second));
    assert_ne!(first.waker().ctx, second.waker().ctx);
    assert_eq!(slots.made(), 2);
    slots.give_back(&first);
    let third = slots.take();
    assert!(
        Arc::ptr_eq(&third, &first),
        "the slot that is free is taken before a new one is made"
    );
    assert_eq!(slots.made(), 2);
}

#[test]
fn a_slot_starts_clean_and_a_late_wake_costs_the_call_that_holds_it_one_wake() {
    let slots = Slots::default();
    let (wakes, task) = Wakes::new();
    let slot = slots.take();
    let late = slot.waker();
    assert!(!slot.park(&task), "the first call parks");
    slots.give_back(&slot);
    wake_through(late);
    assert_eq!(
        wakes.count(),
        0,
        "the task of a call that ended is not woken"
    );
    let next = slots.take();
    assert!(Arc::ptr_eq(&next, &slot));
    assert!(
        !next.park(&task),
        "a wake that came while the slot was free is not the next call's"
    );
    wake_through(late);
    assert_eq!(
        wakes.count(),
        1,
        "a late wake wakes the call that holds the slot now, once"
    );
}

#[test]
fn the_slots_keep_every_slot_until_they_are_dropped() {
    let slots = Slots::default();
    // A call that is leaked or lost never gives its slot back.
    let lost = Arc::downgrade(&slots.take());
    let held = slots.take();
    let kept = Arc::downgrade(&held);
    slots.give_back(&held);
    drop(held);
    assert!(
        lost.upgrade().is_some() && kept.upgrade().is_some(),
        "a late wake through either slot reaches live memory"
    );
    wake_through(
        lost.upgrade()
            .expect("the slots keep the lost slot")
            .waker(),
    );
    drop(slots);
    assert!(
        lost.upgrade().is_none() && kept.upgrade().is_none(),
        "the slots go when their owner drops them"
    );
}
