//! Where a tool call waits between two steps: the [`Parked`] slot that
//! the `ctx` of the call's `RvWaker` points at, the [`Slots`] of one
//! Plugin's calls, which the Plugin owns until it is unloaded, and
//! [`wake`], the function in every `RvWaker` the loader hands out.

use std::ffi::c_void;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::Waker;
use std::{mem, panic};

use runvil_plugin_sys::RvWaker;

#[cfg(test)]
#[path = "park-tests.rs"]
mod tests;

/// What the `ctx` of one tool call's waker points at: whether the Plugin
/// woke the call since its last step began, and otherwise the task to
/// wake when it does.
///
/// A Plugin may call `wake` on any thread and at any time until it is
/// unloaded: on the loader's own thread inside a step or inside
/// RV_TOOL_CANCEL, on a thread of its own, and after the call has ended.
/// So no method here holds the slot's lock across a call out of this
/// module. To clone, wake, or drop a `Waker` runs the Host's code, so a
/// method clones one before it takes the lock, moves plain data under
/// the lock, and lets go of the lock before it wakes or drops one.
#[derive(Default)]
pub(crate) struct Parked {
    state: Mutex<State>,
}

/// What one slot holds.
#[derive(Default)]
enum State {
    /// No wake has come since the call's last step began, and no task
    /// waits for one.
    #[default]
    Quiet,
    /// No wake has come since the call's last step began, and the call is
    /// parked: this wakes its task.
    Waiting(Waker),
    /// The Plugin woke the call since its last step began.
    Woken,
}

impl Parked {
    /// The contract's waker over this slot, the same one each time. Its
    /// `ctx` points at the slot, so it is valid for as long as the
    /// [`Slots`] that made the slot live.
    pub(crate) fn waker(self: &Arc<Self>) -> RvWaker {
        RvWaker {
            ctx: Arc::as_ptr(self).cast_mut().cast(),
            wake,
        }
    }

    /// The state, locked. It is plain data that no panic leaves half
    /// written, so a poisoned lock is opened all the same.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Forgets every wake so far, and the task that waited for one. A
    /// step that begins calls it before the entry point is called: that
    /// step is the one the wakes so far asked for, and a wake from here
    /// on asks for the step after it.
    pub(crate) fn clear(&self) {
        let before = {
            let mut state = self.lock();
            mem::take(&mut *state)
        };
        // The lock is let go by now.
        drop(before);
    }

    /// Parks the call for `task`: keeps `task` to wake at the next wake,
    /// unless a wake came since the call's last step began. Returns
    /// whether one came, and the caller then wakes `task` itself.
    ///
    /// The check and the keeping are one act under the slot's lock. So a
    /// wake that lands between a step's return and this call is not
    /// lost: it came first and is reported here, or it comes after and
    /// wakes `task`.
    pub(crate) fn park(&self, task: &Waker) -> bool {
        let fresh = State::Waiting(task.clone());
        let (woken, spare) = {
            let mut state = self.lock();
            if matches!(*state, State::Woken) {
                (true, fresh)
            } else {
                (false, mem::replace(&mut *state, fresh))
            }
        };
        // The lock is let go by now.
        drop(spare);
        woken
    }

    /// Notes a wake, and wakes the task of a call that is parked here.
    fn note(&self) {
        let before = {
            let mut state = self.lock();
            mem::replace(&mut *state, State::Woken)
        };
        // The lock is let go by now.
        if let State::Waiting(task) = before {
            task.wake();
        }
    }
}

/// The `wake` of every `RvWaker` the loader hands a Plugin. The Plugin
/// may call it on any thread, any number of times, from the first step
/// of the waker's call until its RV_PLUGIN_UNLOAD returns.
///
/// It notes the wake in the slot behind `ctx` and wakes the task of a
/// call that is parked there. It calls nothing of the Plugin's, and it
/// holds no lock while it wakes that task, so it does not block. It
/// never unwinds into the Plugin: the slot's lock is opened even when it
/// is poisoned, and a panic in the task's waker ends here. A NULL `ctx`
/// names no slot and wakes nothing.
///
/// # Safety
/// `ctx` is NULL, or the `ctx` of an `RvWaker` that [`Parked::waker`]
/// made for a Plugin whose RV_PLUGIN_UNLOAD has not returned.
pub(crate) unsafe extern "C" fn wake(ctx: *mut c_void) {
    // SAFETY: the caller passes NULL or the `ctx` of a waker that
    // `Parked::waker` made, which points at a slot inside an `Arc` that
    // the `Slots` of the waker's Plugin hold. The `Plugin` frees its
    // `Slots` only after its RV_PLUGIN_UNLOAD has returned, and the
    // caller makes no wake after that, so the slot is alive. Every use
    // of a slot is through a shared reference, so this one may be made
    // on any thread.
    let Some(slot) = (unsafe { ctx.cast::<Parked>().as_ref() }) else {
        return;
    };
    // The task's waker is the Host's own code. A panic in it ends here,
    // because an unwind must not cross into the Plugin.
    let _panicked = panic::catch_unwind(|| slot.note());
}

/// The slots of one Plugin's tool calls. It keeps every slot it ever
/// made for as long as it lives, and the `Plugin` that owns it drops it
/// only after RV_PLUGIN_UNLOAD has returned. So the `ctx` of every waker
/// the Plugin was handed points at live memory until then, however long
/// ago the waker's call ended.
///
/// A call takes a slot when it starts and gives it back when it ends, so
/// there are as many slots as calls were ever in flight at once. A wake
/// meant for a call that ended reaches the slot that call held, which is
/// free or held by a later call. It costs that later call one step it
/// did not need, and no more.
#[derive(Default)]
pub(crate) struct Slots {
    pool: Mutex<Pool>,
}

/// The slots of one [`Slots`].
#[derive(Default)]
struct Pool {
    /// Every slot made so far. No slot ever leaves it.
    made: Vec<Arc<Parked>>,
    /// The slots that no call holds now.
    free: Vec<Arc<Parked>>,
}

impl Slots {
    /// The pool, locked. It is plain data that no panic leaves half
    /// written, so a poisoned lock is opened all the same.
    fn lock(&self) -> MutexGuard<'_, Pool> {
        self.pool.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A slot for a call that starts: one that no call holds, or a new
    /// one when every slot is held. It starts clean, whatever wake came
    /// for the call that held it last.
    pub(crate) fn take(&self) -> Arc<Parked> {
        let slot = {
            let mut pool = self.lock();
            if let Some(free) = pool.free.pop() {
                free
            } else {
                let made = Arc::new(Parked::default());
                pool.made.push(Arc::clone(&made));
                made
            }
        };
        // The pool's lock is let go by now.
        slot.clear();
        slot
    }

    /// Takes `slot` back from the call that held it, once for each
    /// [`take`](Slots::take), when that call has ended. The task of that
    /// call is forgotten, so a late wake wakes no task of a call that
    /// ended.
    pub(crate) fn give_back(&self, slot: &Arc<Parked>) {
        slot.clear();
        let free = Arc::clone(slot);
        self.lock().free.push(free);
    }

    /// How many slots were made so far, held or free.
    #[cfg(test)]
    pub(crate) fn made(&self) -> usize {
        self.lock().made.len()
    }
}
