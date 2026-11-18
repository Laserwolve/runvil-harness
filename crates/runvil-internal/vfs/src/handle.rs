//! The cloneable handle, the RAII capability, and the happens-before
//! claims ledger.
//!
//! [`VfsRef`] is the public handle: an `Arc`-shared volume pairing one
//! backend with the claims ledger, behind poison-safe locks. [`Access`]
//! is the RAII capability vended by [`VfsRef::acquire`]: it canonicalizes
//! paths at receipt, consults the handle's policy, admits the operation,
//! and locks the backend's access object per call. The admission refuses
//! an access whose scope or identity has ended. On a volume that checks
//! claims it also checks and records the operation's claim, after the
//! policy, so a denied operation never registers one. A handle with an
//! installed op sink fires it on every admitted operation - after the
//! policy and the admission pass, before the backend executes; see
//! [`crate::observe`].
//!
//! # Which volumes check claims
//!
//! A volume says whether its operations check and record claims. The
//! volume of a [`Store`] does. The volume of a handle a caller builds,
//! with [`VfsRef::new`] or a [`VfsRefBuilder`], does not: an operation on
//! it claims nothing and is checked against nothing, so two unordered
//! accesses to one path both run, and the later write is the content. An
//! overlay checks exactly when its base does. Identities, scopes, forks,
//! and joins are the same on every volume, because a store view takes
//! its order from the access it comes from, and a closed scope and an
//! ended identity refuse their operations on every volume.
//!
//! # Scopes, fork and join
//!
//! [`VfsRef::acquire`] starts a *scope*: the acquired [`Access`] is the
//! root identity, and every identity [`ScopeHandle::spawn`] forks joins
//! the scope. Each identity holds a vector clock, and every admitted access
//! records an *epoch* - its identity paired with its own clock entry at
//! that moment - on each region it touches. One epoch is ordered before
//! another identity's next step exactly when the other's clock has seen
//! the epoch's entry, so conflicts follow happens-before (FastTrack-style,
//! Flanagan and Freund, PLDI 2009) instead of liveness: a spawn forks the
//! parent's clock into the child, and a join - [`ScopeHandle::join`] -
//! merges the child's final clock back into the owner's. An identity's own
//! entry lives outside the shared snapshot, so a spawn reuses the parent's
//! map read-only and records the parent's entry as one frozen fork step:
//! a fanout shares one map instead of copying it per arm. Claims are never
//! released during a scope's life. An identity ends when its last
//! [`Access`] drops, its final clock stays in the scope for late joins,
//! and a scope ends with its last identity or when its run closes it
//! through [`ScopeHandle::end`], whichever comes first; its
//! claims are then ignored and purged lazily, and a closed scope's
//! accesses refuse every later operation. Two live scopes never order
//! each other, so where a volume checks claims, theirs always conflict.
//! The region model: a read claims the path, directory children, or
//! pattern it observes; a write claims its path, the ancestors it may
//! create, or the whole subtree it removes.

mod access;
mod case_fold;
mod claims;
mod claims_tree;
mod forward;
mod lines;
mod operations;
mod prune;
mod region;
mod scope;
mod scope_handle;
mod store_view;
#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::error::VfsError;
use crate::observe::{OpSink, Origin};
use crate::path::{VfsPath, canonicalize_absolute};
use crate::router::{Mounts, Router, VfsRefBuilder};
use crate::traits::{AcquireContext, AllowAll, ExecId, Policy, Vfs, VfsAccess};

pub(crate) use case_fold::CaseFolds;
use claims::Claims;
pub(crate) use scope::Scope;
pub use scope_handle::ScopeHandle;
pub use store_view::Store;

/// One mounted filesystem instance: its backend and the ledger of who is
/// touching what. The two are separately `Arc`-shareable so `overlay()`
/// can share the claims table while swapping the backend.
struct Volume {
    backend: Arc<Mutex<Box<dyn Vfs>>>,
    claims: Arc<Claims>,
    /// Whether an operation through this volume checks and records a
    /// claim in `claims`. A store's volume does. The volume of a handle
    /// a caller builds does not, and its ledger stays empty.
    checks_claims: bool,
    sink: Option<OpSink>,
    /// The mount prefixes whose claim keys fold case.
    folds: CaseFolds,
}

/// A cloneable handle to a virtual filesystem: one backend, and the
/// policy consulted on every operation.
///
/// Clones share the backend and the policy. A handle that a caller
/// builds, with [`VfsRef::new`] or a [`VfsRefBuilder`], checks no claims:
/// an operation through it runs whatever another access has read or
/// written. Two accesses that are unordered, such as those of two tasks
/// or of two scopes, may use one path, and the later write is the
/// content. Each access still has an identity in a scope, and an access
/// whose scope or identity has ended refuses every operation.
#[derive(Clone)]
pub struct VfsRef {
    volume: Arc<Volume>,
    policy: Arc<dyn Policy + Sync>,
}

impl fmt::Debug for VfsRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VfsRef").finish_non_exhaustive()
    }
}

impl VfsRef {
    /// Returns a handle over `backend` with the [`AllowAll`] policy.
    #[must_use]
    pub fn new(backend: impl Vfs + 'static) -> VfsRef {
        Self::with_policy(backend, AllowAll)
    }

    /// Returns a handle over `backend` that consults `policy` on every
    /// operation.
    ///
    /// The policy can share its state with the caller, for example
    /// through an `Arc`. When the caller changes that state during a run,
    /// the next operation sees the change.
    #[must_use]
    pub(crate) fn with_policy(
        backend: impl Vfs + 'static,
        policy: impl Policy + Sync + 'static,
    ) -> VfsRef {
        let folds = CaseFolds::of(&VfsPath::root(), &backend);
        VfsRef {
            volume: Arc::new(Volume {
                backend: Arc::new(Mutex::new(Box::new(backend))),
                claims: Arc::new(Claims::new()),
                checks_claims: false,
                sink: None,
                folds,
            }),
            policy: Arc::new(policy),
        }
    }

    /// Returns a builder for installing mounts.
    ///
    /// The mount table is fixed when [`VfsRefBuilder::build`] runs, so
    /// clones share it cheaply.
    #[must_use]
    pub fn builder() -> VfsRefBuilder {
        VfsRefBuilder::new()
    }

    /// Returns a new handle that mounts `backend` at `prefix` on top of
    /// this handle's namespace.
    ///
    /// The new handle shares this handle's policy and operation sink, and
    /// it treats claims as this handle does: a handle that a caller
    /// builds checks none.
    ///
    /// # Panics
    /// Panics when `prefix` is not an absolute virtual path or is the
    /// root: an overlay at `/` would replace the base entirely, so use
    /// [`VfsRef::new`] instead.
    #[must_use]
    #[track_caller]
    pub fn overlay(&self, prefix: &str, backend: impl Vfs + 'static) -> VfsRef {
        let canonical = match canonicalize_absolute(prefix) {
            Ok(canonical) => canonical,
            Err(err) => panic!("invalid overlay prefix {prefix:?}: {err}"),
        };
        assert!(
            canonical.as_str() != "/",
            "an overlay at / would replace the base entirely; use VfsRef::new instead"
        );
        // The base handle mounts at the root of the overlay's router:
        // operations outside the overlay prefix route through the base's
        // own policy and claims under the caller's identity.
        let folds = self.volume.folds.with(&CaseFolds::of(&canonical, &backend));
        let mut mounts = Mounts::new();
        mounts.insert(
            VfsPath::root().to_buf(),
            Arc::new(Mutex::new(Box::new(self.clone()))),
        );
        mounts.insert(canonical.to_buf(), Arc::new(Mutex::new(Box::new(backend))));
        VfsRef {
            volume: Arc::new(Volume {
                backend: Arc::new(Mutex::new(Box::new(Router::new(mounts)))),
                // The overlay shares the base's claims table and checks
                // claims exactly when the base does, so where they are
                // checked a conflict is found across both views of the
                // same storage.
                claims: Arc::clone(&self.volume.claims),
                checks_claims: self.volume.checks_claims,
                // One sink observes both views of the same storage, fired
                // by the outer capability with the caller's origin.
                sink: self.volume.sink.clone(),
                folds,
            }),
            policy: Arc::clone(&self.policy),
        }
    }

    /// Acquires an `Access` capability for a new serial thread of
    /// execution.
    ///
    /// Every acquire creates a fresh [`ExecId`] and starts a new *scope*.
    /// A scope is the acquired identity together with every identity
    /// later forked from it. The scope orders its identities, and it
    /// ends with its last identity or when its owner closes it. On a
    /// handle that a caller builds, no scope holds a file against
    /// another: two live scopes may read and write one path, and the
    /// later write is the content.
    ///
    /// `origin` only labels the operation events this capability fires.
    /// It never decides whether an operation may proceed.
    ///
    /// # Errors
    /// Returns an error when the backend refuses to acquire the identity.
    pub fn acquire(&self, origin: Origin) -> Result<Access, VfsError> {
        self.acquire_with(
            &AcquireContext::new(ExecId::vend(), Scope::start()),
            Some(origin),
        )
    }

    /// Acquires an `Access` capability for a new serial thread of
    /// execution, beside the [`ScopeHandle`] on the scope it starts.
    ///
    /// This is [`VfsRef::acquire`] plus the authority over the new scope:
    /// the scope handle forks, joins, and ends the scope's identities, and
    /// ends the scope. No other call returns a scope handle, so an
    /// `Access` alone never carries that authority.
    ///
    /// # Errors
    /// Returns an error when the backend refuses to acquire the identity.
    pub fn acquire_scope(&self, origin: Origin) -> Result<(Access, ScopeHandle), VfsError> {
        let access = self.acquire(origin)?;
        let scope = ScopeHandle::new(&access.scope);
        Ok((access, scope))
    }

    /// Acquires the capability for the identity and scope in `cx`: a
    /// fresh acquire passes a new scope, and a mounted handle receives
    /// the caller's context, so the forwarded capability joins the
    /// caller's scope and, where claims are checked, its claims conflict
    /// across both views of the same storage. A `None` origin is the mount forward: the outer
    /// handle already fired the caller's origin, so the forward fires
    /// nothing rather than double the event with a fabricated, less
    /// precise one.
    ///
    /// # Errors
    /// Returns an error when the backend refuses to acquire the identity;
    /// see [`VfsRef::acquire`]. Returns [`VfsError::ScopeEnded`], before
    /// any backend call, when the scope in `cx` belongs to a run that has
    /// ended.
    fn acquire_with(
        &self,
        cx: &AcquireContext,
        origin: Option<Origin>,
    ) -> Result<Access, VfsError> {
        let scope = self.join_scope(cx)?;
        let inner = match self.backend().acquire(cx) {
            Ok(inner) => inner,
            Err(err) => {
                scope.release(cx.id());
                return Err(err);
            }
        };
        Ok(Access {
            id: cx.id(),
            origin,
            store_view: false,
            volume: Arc::clone(&self.volume),
            policy: Arc::clone(&self.policy),
            scope,
            inner: Mutex::new(inner),
            released: false,
        })
    }

    /// Joins the scope in `cx`: attaches the identity, fresh or
    /// forwarded, and, on a handle that checks claims, registers the
    /// scope in its claims tables. A closed scope is refused before
    /// either. A handle that checks no claims registers nothing, so its
    /// tables never grow.
    fn join_scope(&self, cx: &AcquireContext) -> Result<Arc<Scope>, VfsError> {
        let scope = Arc::clone(cx.scope());
        scope.attach(cx.id(), &VfsPath::root())?;
        if self.volume.checks_claims {
            self.volume.claims.register_scope(&scope);
        }
        Ok(scope)
    }

    /// Builds a handle over a router with a fresh claims table and the
    /// installed policy, op sink, and the mounts' case folds, which
    /// checks claims when `checks_claims` is set: the builder's exit.
    pub(crate) fn from_router(
        router: Router,
        policy: Arc<dyn Policy + Sync>,
        sink: Option<OpSink>,
        folds: CaseFolds,
        checks_claims: bool,
    ) -> VfsRef {
        VfsRef {
            volume: Arc::new(Volume {
                backend: Arc::new(Mutex::new(Box::new(router))),
                claims: Arc::new(Claims::new()),
                checks_claims,
                sink,
                folds,
            }),
            policy,
        }
    }

    /// Poison-safe lock on the backend.
    fn backend(&self) -> MutexGuard<'_, Box<dyn Vfs>> {
        self.volume
            .backend
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// A capability that performs filesystem operations through a `VfsRef`
/// as one identity.
///
/// It holds an [`ExecId`] and the backend's access object. Every
/// operation canonicalizes the path, checks the policy, refuses when its
/// scope or its identity has ended, and reports the operation to the
/// observer installed with `VfsRefBuilder::on_op`, if any. It then locks
/// the backend for that one call. An access of a handle that a caller
/// builds checks no claims. A view of a store, an access from
/// `Store::view`, checks the store's claims between the policy and the
/// observer, and refuses an operation that conflicts with one that is
/// not ordered before it.
///
/// Dropping an `Access` drops one reference to its identity. The
/// identity ends when its last access drops, and the scope ends when
/// its last identity ends. [`Access::close`] does the same and reports
/// the backend release failures that dropping swallows.
#[must_use = "an acquire dropped immediately is a bug: the capability is its identity, which ends with it"]
pub struct Access {
    id: ExecId,
    /// The caller-supplied observability origin; `None` only on the
    /// crate-private mount forward, which never fires.
    origin: Option<Origin>,
    /// Set when this access is a view of a [`Store`]: the strict
    /// store-path rules gate every path, and errors come back in the
    /// caller's logical form.
    store_view: bool,
    volume: Arc<Volume>,
    policy: Arc<dyn Policy + Sync>,
    /// The scope this identity belongs to: its vector clock and its
    /// reference count, shared with every access of the scope.
    scope: Arc<Scope>,
    inner: Mutex<Box<dyn VfsAccess>>,
    /// Set once the identity's reference and its backend sessions are
    /// released, so a close followed by the drop releases once.
    released: bool,
}

impl fmt::Debug for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Access")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Access {
    /// Drops this access's reference to its identity, as dropping the
    /// access does, and reports what the release failed to do.
    ///
    /// The identity ends when its last access goes, and the scope ends
    /// with its last identity. Its claims are never released: an ended
    /// scope's claims are ignored.
    ///
    /// # Errors
    /// Returns the first failure among the backend sessions' releases,
    /// which dropping the access ignores. The identity's reference is
    /// released either way.
    pub fn close(mut self) -> Result<(), VfsError> {
        self.release()
    }

    /// The release behind both [`Access::close`] and the drop: the
    /// session's own releases, then the identity's reference, then the
    /// backend's release of the identity. Runs once.
    fn release(&mut self) -> Result<(), VfsError> {
        if std::mem::replace(&mut self.released, true) {
            return Ok(());
        }
        let session = self
            .inner
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .close();
        // At zero references the identity ends - and, when it was the
        // last, the scope ends with it.
        self.scope.release(self.id);
        let backend = self.backend().release(self.id);
        session.and(backend)
    }
}

impl Drop for Access {
    fn drop(&mut self) {
        // Releasing at the backend is best-effort here: the
        // happens-before state is already handled, so a backend failure
        // cannot leave a conflict behind. `Access::close` reports it.
        let _ = self.release();
    }
}
