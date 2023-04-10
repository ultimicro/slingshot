use self::cancel::CancellationToken;
use self::task::WakerData;
use std::future::Future;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

pub mod cancel;
#[cfg(unix)]
pub mod fd;
pub mod future;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod task;
#[cfg(target_os = "windows")]
pub mod windows;

/// Block the current thread until all futures has been completed. This function can be called only
/// once.
pub fn run<R, T>(rt: &'static R, task: T)
where
    R: Runtime,
    T: Future<Output = ()> + Send + 'static,
{
    // Toggle running flag.
    if RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        panic!("the runtime is already running");
    }

    // Spawn the main task.
    if rt.spawn(task).is_some() {
        panic!("shutdown() was invoked on the runtime before run()");
    }

    // Enter event loop.
    let worker_count = rt.thread_count().get();

    if worker_count > 1 {
        std::thread::scope(|s| {
            for _ in 0..worker_count {
                s.spawn(|| event_loop(rt));
            }
        });
    } else {
        event_loop(rt);
    }

    // Check if the workers was stopped because of an error.
    let mut error = ERROR.lock().unwrap();

    if let Some(e) = error.take() {
        panic!("cannot dequeue the I/O event: {e}");
    }
}

fn event_loop<R: Runtime>(runtime: &'static R) {
    let mut ready: Vec<Pin<Box<dyn Future<Output = ()> + Send>>> = Vec::new();

    'main: loop {
        // Wait for some tasks to ready.
        match runtime.dequeue(&mut ready) {
            Ok(v) => {
                if !v {
                    break;
                }
            }
            Err(e) => {
                // Set the error.
                let mut lock = ERROR.lock().unwrap();

                if lock.is_none() {
                    *lock = Some(e);
                }

                break;
            }
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
                        break 'main;
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

static RUNNING: AtomicBool = AtomicBool::new(false);
static ERROR: Mutex<Option<std::io::Error>> = Mutex::new(None);

/// Provides the methods to dequeue the I/O event. All methods in this trait is for internal use
/// only.
pub trait EventQueue: Sync {
    /// Returns the number of thread to invoke on [`dequeue()`].
    fn thread_count(&self) -> NonZeroUsize;

    /// Returns [`Ok(false)`] if all tasks has been completed or this method return the error on the
    /// other thread.
    fn dequeue(
        &self,
        ready: &mut Vec<Pin<Box<dyn Future<Output = ()> + Send>>>,
    ) -> std::io::Result<bool>;

    /// Returns `false` if no more tasks available.
    fn drop_task(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>) -> bool;

    fn push_ready(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>);
}

/// Provides methods to do asynchronous I/O on std types.
pub trait Runtime: EventQueue {
    type TcpAccept<'a>: Future<Output = std::io::Result<(TcpStream, SocketAddr)>> + Send
    where
        Self: 'a;

    type TcpRead<'a>: Future<Output = std::io::Result<usize>> + Send
    where
        Self: 'a;

    type TcpWrite<'a>: Future<Output = std::io::Result<usize>> + Send
    where
        Self: 'a;

    type Delay<'a>: Future<Output = std::io::Result<()>> + Send
    where
        Self: 'a;

    /// Return [`Some`] if the runtime being shutting down.
    fn spawn<T: Future<Output = ()> + Send + 'static>(&self, task: T) -> Option<T>;

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
