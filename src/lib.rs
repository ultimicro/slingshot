pub use cancellation::*;

use self::task::WakerData;
use std::future::Future;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

mod cancellation;
mod task;

pub fn event_loop<Q: EventQueue>(runtime: &'static Q) -> std::io::Result<()> {
    let mut ready: Vec<Pin<Box<dyn Future<Output = Status<()>> + Send>>> = Vec::new();

    'main: loop {
        // Wait for some tasks to ready.
        if !runtime.dequeue(&mut ready)? {
            break Ok(());
        }

        // Advance the tasks that are ready.
        for mut task in ready.drain(..) {
            // Advance the task.
            let data = Arc::new(Mutex::new(WakerData::new(runtime)));
            let raw_waker = WakerData::into_raw_waker(data.clone());
            let waker = unsafe { Waker::from_raw(raw_waker) };
            let mut context = Context::from_waker(&waker);

            match task.as_mut().poll(&mut context) {
                Poll::Ready(_) => {
                    // Decrease the number of active tasks.
                    if !runtime.drop_task(task) {
                        break 'main Ok(());
                    }
                }
                Poll::Pending => {
                    // Store the future in the waker data.
                    let mut data = data.lock().unwrap();

                    if let Some(task) = data.set_pending(task) {
                        runtime.push_ready(task);
                    }
                }
            }
        }
    }
}

/// Provides the methods to dequeue the I/O event. All methods in this trait is for internal use
/// only.
pub trait EventQueue: Sync {
    /// Returns [`Ok(false)`] if all tasks has been completed or this method return the error on the
    /// other thread.
    fn dequeue(
        &self,
        ready: &mut Vec<Pin<Box<dyn Future<Output = Status<()>> + Send>>>,
    ) -> std::io::Result<bool>;

    /// Returns `false` if no more tasks available.
    fn drop_task(&self, task: Pin<Box<dyn Future<Output = Status<()>> + Send>>) -> bool;

    fn push_ready(&self, task: Pin<Box<dyn Future<Output = Status<()>> + Send>>);
}

/// Provides methods to do asynchronous I/O on std types.
pub trait Runtime {
    type TcpAccept<'a>: Future<Output = Status<std::io::Result<(TcpStream, SocketAddr)>>> + Send
    where
        Self: 'a;

    type TcpRead<'a>: Future<Output = Status<std::io::Result<usize>>> + Send
    where
        Self: 'a;

    type TcpWrite<'a>: Future<Output = Status<std::io::Result<usize>>> + Send
    where
        Self: 'a;

    type Delay<'a>: Future<Output = Status<std::io::Result<()>>> + Send
    where
        Self: 'a;

    /// Return [`Some`] if the runtime being shutting down.
    fn spawn<T: Future<Output = Status<()>> + Send + 'static>(&self, task: T) -> Option<T>;

    /// An asynchronous version of [`TcpListener::accept()`]. `tcp` should be in non-blocking mode
    /// otherwise the call may be block.
    fn accept_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpListener,
        ct: Option<CancellationToken>,
    ) -> Self::TcpAccept<'a>;

    /// An asynchronous version of [`std::io::Read::read()`] for [`TcpStream`]. `tcp` should be in
    /// non-blocking mode otherwise the call may be block.
    fn read_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpStream,
        buf: &'a mut [u8],
        ct: Option<CancellationToken>,
    ) -> Self::TcpRead<'a>;

    /// An asynchronous version of [`std::io::Write::write()`] for [`TcpStream`]. `tcp` should be in
    /// non-blocking mode otherwise the call may be block.
    fn write_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpStream,
        buf: &'a [u8],
        ct: Option<CancellationToken>,
    ) -> Self::TcpWrite<'a>;

    /// Create a future that will complete after `dur`.
    fn delay(&self, dur: Duration, ct: Option<CancellationToken>) -> Self::Delay<'_>;
}

/// A type for [`Future::Output`] for cancelable future.
pub enum Status<T> {
    Completed(T),
    Canceled,
}
