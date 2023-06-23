use std::collections::HashMap;
use std::ops::DerefMut;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;

/// An object to signal a cancellation.
#[derive(Clone, Default)]
pub struct CancellationToken {
    data: Arc<TokenData>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            data: Arc::default(),
        }
    }

    pub fn is_canceled(&self) -> bool {
        self.data.canceled.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        // Toggle canceled flag.
        let mut subscribers = self.data.subscribers.lock().unwrap();

        if self
            .data
            .canceled
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        // Move out the subscribers and give up the lock.
        let wakers = std::mem::take(subscribers.deref_mut()).into_values();
        drop(subscribers);

        // Invoke all subscribers.
        for w in wakers {
            w.wake();
        }
    }

    /// The order of invocation for `waker` is unspecified.
    pub fn subscribe(&self, waker: Waker) -> Option<CancellationSubscription> {
        use std::collections::hash_map::Entry;

        // Invoke the handler immediately if we already canceled.
        let mut subscribers = self.data.subscribers.lock().unwrap();

        if self.data.canceled.load(Ordering::Relaxed) {
            drop(subscribers);
            waker.wake();
            return None;
        }

        // Store the handler.
        let id = loop {
            let k = self.data.next_id.fetch_add(1, Ordering::Relaxed);

            match subscribers.entry(k) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(e) => e.insert(waker),
            };

            break k;
        };

        drop(subscribers);

        // Construct the handle.
        Some(CancellationSubscription {
            data: self.data.clone(),
            id,
        })
    }
}

/// A subscription to [`CancellationToken`].
pub struct CancellationSubscription {
    data: Arc<TokenData>,
    id: usize,
}

impl CancellationSubscription {
    pub fn update(&mut self, waker: Waker) {
        let mut subscribers = self.data.subscribers.lock().unwrap();

        if self.data.canceled.load(Ordering::Relaxed) {
            drop(subscribers);
            waker.wake();
            return;
        }

        subscribers.insert(self.id, waker);
    }
}

impl Drop for CancellationSubscription {
    fn drop(&mut self) {
        self.data.subscribers.lock().unwrap().remove(&self.id);
    }
}

/// Contains a data for [`CancellationToken`].
#[derive(Default)]
struct TokenData {
    subscribers: Mutex<HashMap<usize, Waker>>,
    next_id: AtomicUsize,
    canceled: AtomicBool,
}
