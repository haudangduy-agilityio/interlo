//! A result that arrives once and the threads that wait for it: one setup
//! check, one engine load and the launch check of the model files.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

pub(crate) struct Flight<T> {
    result: Mutex<Option<T>>,
    arrived: Condvar,
}

impl<T> Default for Flight<T> {
    fn default() -> Self {
        Self {
            result: Mutex::new(None),
            arrived: Condvar::new(),
        }
    }
}

impl<T: Clone> Flight<T> {
    /// Every change is one assignment, so a poisoned lock is still safe to use.
    fn result(&self) -> MutexGuard<'_, Option<T>> {
        self.result.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Keep `result` and wake the threads that wait for it.
    pub(crate) fn finish(&self, result: T) {
        *self.result() = Some(result);
        self.arrived.notify_all();
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.result().is_some()
    }

    /// The result, once it is there.
    pub(crate) fn wait(&self) -> T {
        let mut result = self.result();
        loop {
            if let Some(found) = result.as_ref() {
                return found.clone();
            }
            result = self
                .arrived
                .wait(result)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Change a result that is there.
    pub(crate) fn change(&self, change: impl FnOnce(&mut T)) {
        if let Some(result) = self.result().as_mut() {
            change(result);
        }
    }
}
