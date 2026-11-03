use alloc::sync::Arc;
use core::ops::Deref;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::PoisonError;

/// Shared pointer whose owner can block until it holds the only reference.
///
/// Cloning a [`Shared`] creates another reference to the same data, as with
/// [`Arc`]. Dropping a [`Shared`] notifies threads that are currently waiting
/// for exclusive access.
#[derive(Debug)]
pub(crate) struct Shared<T> {
    data: Arc<T>,
    /// This field must drop after `data` to prevent deadlocks: a waiting thread
    /// that is notified before the reference to `data` is released would go
    /// back to sleep and miss the wakeup.
    barrier: Barrier,
}

#[derive(Clone, Debug, Default)]
struct Barrier(Arc<BarrierInner>);

#[derive(Debug, Default)]
struct BarrierInner {
    waiter: Mutex<()>,
    notifier: Condvar,
}

impl<T> Shared<T> {
    /// Block the current thread until it holds the only reference to the shared
    /// data, then return exclusive access to it.
    ///
    /// Holding `&mut self` prevents new references from being created through
    /// `self` while the exclusive reference is alive.
    pub(crate) fn wait_exclusive(&mut self) -> &mut T {
        {
            // `waiter` guards no data, so poison can be safely ignored.
            let mut guard = self
                .barrier
                .0
                .waiter
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            while Arc::get_mut(&mut self.data).is_none() {
                // `waiter` guards no data, so poison can be safely ignored.
                guard = self
                    .barrier
                    .0
                    .notifier
                    .wait(guard)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }
        // The current borrow checker cannot return a mutable reference from
        // within the loop.
        Arc::get_mut(&mut self.data).expect("bug: `Shared` should be uniquely owned at this point")
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            barrier: self.barrier.clone(),
        }
    }
}

impl<T> Default for Shared<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            data: Arc::default(),
            barrier: Barrier::default(),
        }
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.data
    }
}

impl Drop for Barrier {
    fn drop(&mut self) {
        // `waiter` guards no data, so poison can be safely ignored.
        let _guard = self.0.waiter.lock().unwrap_or_else(PoisonError::into_inner);
        self.0.notifier.notify_all();
    }
}
