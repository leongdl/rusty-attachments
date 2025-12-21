//! Generic progress callback trait and implementations.

use std::marker::PhantomData;

/// Generic progress callback trait.
///
/// Type parameter `T` is the progress data type, allowing different
/// operations to report different progress information while sharing
/// the same callback pattern.
pub trait ProgressCallback<T>: Send + Sync {
    /// Called with progress updates.
    ///
    /// # Arguments
    /// * `progress` - Progress data for the current operation
    ///
    /// # Returns
    /// - `true` to continue the operation
    /// - `false` to cancel the operation
    fn on_progress(&self, progress: &T) -> bool;
}

/// A no-op progress callback that always continues.
pub struct NoOpProgress;

impl<T> ProgressCallback<T> for NoOpProgress {
    fn on_progress(&self, _progress: &T) -> bool {
        true
    }
}

/// A progress callback that wraps a closure.
pub struct FnProgress<F, T> {
    callback: F,
    _marker: PhantomData<T>,
}

impl<F, T> FnProgress<F, T>
where
    F: Fn(&T) -> bool + Send + Sync,
{
    /// Create a new closure-based progress callback.
    ///
    /// # Arguments
    /// * `callback` - Closure that receives progress and returns whether to continue
    pub fn new(callback: F) -> Self {
        Self {
            callback,
            _marker: PhantomData,
        }
    }
}

impl<F, T> ProgressCallback<T> for FnProgress<F, T>
where
    F: Fn(&T) -> bool + Send + Sync,
    T: Send + Sync,
{
    fn on_progress(&self, progress: &T) -> bool {
        (self.callback)(progress)
    }
}

/// Create a progress callback from a closure.
///
/// # Arguments
/// * `f` - Closure that receives progress and returns whether to continue
///
/// # Returns
/// A `FnProgress` wrapper implementing `ProgressCallback<T>`.
pub fn progress_fn<F, T>(f: F) -> FnProgress<F, T>
where
    F: Fn(&T) -> bool + Send + Sync,
{
    FnProgress::new(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    struct TestProgress {
        count: u64,
    }

    #[test]
    fn test_noop_progress() {
        let progress: NoOpProgress = NoOpProgress;
        let data: TestProgress = TestProgress { count: 42 };
        assert!(progress.on_progress(&data));
    }

    #[test]
    fn test_fn_progress_continue() {
        let callback = progress_fn(|p: &TestProgress| p.count < 100);
        assert!(callback.on_progress(&TestProgress { count: 50 }));
    }

    #[test]
    fn test_fn_progress_cancel() {
        let callback = progress_fn(|p: &TestProgress| p.count < 100);
        assert!(!callback.on_progress(&TestProgress { count: 150 }));
    }

    #[test]
    fn test_fn_progress_captures_state() {
        let counter: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
        let counter_clone: Arc<AtomicU64> = counter.clone();

        let callback = progress_fn(move |_: &TestProgress| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            true
        });

        callback.on_progress(&TestProgress { count: 1 });
        callback.on_progress(&TestProgress { count: 2 });

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
