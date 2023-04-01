use crate::EventQueue;
use std::future::Future;
use std::mem::{forget, swap};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{RawWaker, RawWakerVTable};

/// Contains the data required to wake a future.
pub(crate) struct WakerData {
    queue: &'static dyn EventQueue,
    state: FutureState,
}

impl WakerData {
    pub fn new(queue: &'static dyn EventQueue) -> Self {
        Self {
            queue,
            state: FutureState::Polling,
        }
    }

    pub fn into_raw_waker(this: Arc<Mutex<Self>>) -> RawWaker {
        RawWaker::new(Arc::into_raw(this) as _, &WAKER_VTABLE)
    }

    /// Returns [`Some`] if the task has been waked up during [`Future::poll()`].
    pub fn set_pending(
        &mut self,
        task: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Option<Pin<Box<dyn Future<Output = ()> + Send>>> {
        match self.state {
            FutureState::Polling => {
                self.state = FutureState::Pending(task);
                None
            }
            FutureState::Pending(_) => panic!("set_pending() cannot be called more than one"),
            FutureState::Waked => Some(task),
        }
    }

    pub fn wake(&mut self) {
        // Do nothing if the task is already waked.
        if let FutureState::Waked = self.state {
            return;
        }

        // Switch to waked and take out the previous value.
        let mut state = FutureState::Waked;

        swap(&mut state, &mut self.state);

        // Wake the task if we are not called from Future::poll.
        match state {
            FutureState::Polling => {}
            FutureState::Pending(t) => self.queue.push_ready(t),
            FutureState::Waked => unreachable!(),
        }
    }
}

/// Represents a state of the future on [`WakerData`].
enum FutureState {
    Polling,
    Pending(Pin<Box<dyn Future<Output = ()> + Send>>),
    Waked,
}

/// [`RawWakerVTable`] implementation for [`WakerData`].
static WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |data| {
        unsafe { Arc::increment_strong_count(data as *const Mutex<WakerData>) };
        RawWaker::new(data, &WAKER_VTABLE)
    },
    |data| {
        let data = unsafe { Arc::from_raw(data as *const Mutex<WakerData>) };
        data.lock().unwrap().wake();
    },
    |data| {
        let data = unsafe { Arc::from_raw(data as *const Mutex<WakerData>) };
        data.lock().unwrap().wake();
        forget(data);
    },
    |data| unsafe { Arc::decrement_strong_count(data as *const Mutex<WakerData>) },
);
