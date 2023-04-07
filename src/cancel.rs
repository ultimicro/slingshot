use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// An object to signal a cancellation.
#[derive(Clone)]
pub struct CancellationToken {
    data: Arc<TokenData>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            data: Arc::new(TokenData {
                subscribers: Mutex::new(HashMap::new()),
                next_id: AtomicUsize::new(0),
                canceled: AtomicBool::new(false),
            }),
        }
    }

    pub fn is_canceled(&self) -> bool {
        self.data.canceled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        // Toggle canceled flag.
        let mut subscribers = self.data.subscribers.lock().unwrap();

        if self
            .data
            .canceled
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        // Invoke all subscribers.
        for (_, h) in subscribers.drain() {
            h();
        }
    }

    /// The order of invocation for `h` is unspecified.
    pub fn subscribe<H>(&self, h: H) -> Option<SubscriptionHandle>
    where
        H: FnOnce() + Send + 'static,
    {
        use std::collections::hash_map::Entry;

        // Invoke the handler immediately if we already canceled.
        let mut subscribers = self.data.subscribers.lock().unwrap();

        if self.is_canceled() {
            h();
            return None;
        }

        // Store the handler.
        let id = loop {
            let k = self.data.next_id.fetch_add(1, Ordering::Relaxed);

            match subscribers.entry(k) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(e) => {
                    e.insert(Box::new(h));
                    break k;
                }
            }
        };

        drop(subscribers);

        // Construct the handle.
        Some(SubscriptionHandle {
            data: self.data.clone(),
            id,
        })
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// An object to unsubscribe from [`CancellationToken`] when dropped.
pub struct SubscriptionHandle {
    data: Arc<TokenData>,
    id: usize,
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        self.data.subscribers.lock().unwrap().remove(&self.id);
    }
}

/// Contains a data for [`CancellationToken`].
struct TokenData {
    subscribers: Mutex<HashMap<usize, Box<dyn FnOnce() + Send>>>,
    next_id: AtomicUsize,
    canceled: AtomicBool,
}
