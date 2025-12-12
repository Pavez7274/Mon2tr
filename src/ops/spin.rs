use std::{sync::atomic::{AtomicU32, Ordering::*}};

#[repr(transparent)]
pub struct Spin(pub AtomicU32); // 1 = lock; 0 = unlock
pub struct Guard<'a>(&'a Spin);

impl Spin {
    #[inline(always)]
    pub fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    /// Try to acquire without spinning.
    #[inline(always)]
    pub fn try_lock(&self) -> Option<Guard<'_>> {
        return match self.0.compare_exchange(0, 1, Acquire, Relaxed) {
            Ok (_) => Some(Guard(&self)),
            Err(_) => None,
        }
    }

    /// Spin until acquire.
    #[inline(always)]
    pub fn lock(&self) -> Guard<'_> {
        // optimistic attempt (best case).
        if let Some(guard) = self.try_lock() {
            return guard;
        }

        loop {
            if self.0.load(Relaxed) == 0 {
                if self.0.compare_exchange(0, 1, Acquire, Relaxed).is_ok() {
                    return Guard(&self);
                }
            }

            core::hint::spin_loop();
        }
    }

    #[inline(always)]
    pub fn unlock(&self) {
        self.0.store(0, Release);
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.unlock();
    }
}
