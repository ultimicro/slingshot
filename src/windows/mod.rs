use self::handle::Handle;
use self::overlapped::Overlapped;
use self::tcp::{TcpAccept, TcpRead, TcpWrite};
use self::time::Delay;
use crate::cancel::CancellationToken;
use crate::{EventQueue, Runtime};
use std::future::Future;
use std::io::Error;
use std::net::{TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::thread::available_parallelism;
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ABANDONED_WAIT_0, FALSE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::Win32::System::IO::{
    CreateIoCompletionPort, GetQueuedCompletionStatus, PostQueuedCompletionStatus,
};

pub mod handle;
pub mod overlapped;
pub mod socket;
pub mod tcp;
pub mod time;

/// An implementation of [`Runtime`] backed by an I/O completion port.
pub struct Iocp {
    iocp: HANDLE,
    active_tasks: AtomicIsize,
    closed: AtomicBool,
}

impl Iocp {
    pub fn new() -> &'static Self {
        // Create an I/O completion port.
        let iocp = {
            let handle = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, 0, 0, 0) };

            if handle == 0 {
                panic!(
                    "cannot create an I/O completion port: {}",
                    Error::last_os_error()
                );
            }

            Handle::new(handle)
        };

        Box::leak(Box::new(Self {
            iocp: iocp.into_raw(),
            active_tasks: AtomicIsize::new(0),
            closed: AtomicBool::new(false),
        }))
    }

    fn register_handle(&self, handle: HANDLE) -> Result<(), Error> {
        if unsafe { CreateIoCompletionPort(handle, self.iocp, 0, 0) } == 0 {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn shutdown(&self) {
        if self
            .closed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        if unsafe { CloseHandle(self.iocp) } == 0 {
            panic!("cannot close the IOCP handle: {}", Error::last_os_error());
        }
    }
}

impl Drop for Iocp {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::Acquire) {
            if unsafe { CloseHandle(self.iocp) } == 0 {
                panic!("cannot close the IOCP handle: {}", Error::last_os_error());
            }
        }
    }
}

impl EventQueue for Iocp {
    fn thread_count(&self) -> NonZeroUsize {
        available_parallelism().expect("cannot determine a number of worker thread")
    }

    fn dequeue(
        &self,
        ready: &mut Vec<Pin<Box<dyn Future<Output = ()> + Send>>>,
    ) -> std::io::Result<bool> {
        // Wait for the event.
        let mut transferred = 0;
        let mut key = 0;
        let mut overlapped = null_mut();
        let error = if unsafe {
            GetQueuedCompletionStatus(
                self.iocp,
                &mut transferred,
                &mut key,
                &mut overlapped,
                INFINITE,
            )
        } == FALSE
        {
            let e = Error::last_os_error();

            // Check if I/O failed.
            if overlapped.is_null() {
                // Check if a shutdown.
                return if e.raw_os_error().unwrap() == ERROR_ABANDONED_WAIT_0 as _ {
                    Ok(false)
                } else {
                    self.shutdown();
                    Err(e)
                };
            }

            Some(e)
        } else {
            None
        };

        // Check event type.
        if overlapped.is_null() {
            match unsafe { *Box::from_raw(key as *mut Event) } {
                Event::TaskReady(task) => ready.push(task.into()),
            }
        } else {
            unsafe { Box::from_raw(overlapped as *mut Overlapped) }.complete(error);
        }

        Ok(true)
    }

    fn drop_task(&self, _: Pin<Box<dyn Future<Output = ()> + Send>>) -> bool {
        // Decrease the number of active tasks.
        if self.active_tasks.fetch_sub(1, Ordering::Relaxed) != 1 {
            return true;
        }

        // Toggle shutdown if all tasks has been completed.
        if self
            .active_tasks
            .compare_exchange(0, isize::MIN, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return true;
        }

        self.shutdown();
        false
    }

    fn push_ready(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>) {
        let task = unsafe { Pin::into_inner_unchecked(task) };
        let event = Box::new(Event::TaskReady(task));
        let key = Box::into_raw(event);

        if unsafe { PostQueuedCompletionStatus(self.iocp, 0, key as _, null()) } == 0 {
            panic!(
                "cannot send the task to execute: {}",
                Error::last_os_error()
            );
        }
    }
}

impl Runtime for Iocp {
    type TcpAccept<'a> = TcpAccept<'a>;
    type TcpRead<'a> = TcpRead<'a>;
    type TcpWrite<'a> = TcpWrite<'a>;
    type Delay<'a> = Delay<'a>;

    fn spawn<T: Future<Output = ()> + Send + 'static>(&self, task: T) -> Option<T> {
        // Increase the number of active tasks.
        if self.active_tasks.fetch_add(1, Ordering::AcqRel) < 0 {
            return Some(task);
        }

        // Send the task to execute.
        self.push_ready(Box::pin(task));

        None
    }

    fn accept_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpListener,
        ct: Option<CancellationToken>,
    ) -> TcpAccept<'a> {
        TcpAccept::new(self, tcp, ct)
    }

    fn read_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpStream,
        buf: &'a mut [u8],
        ct: Option<CancellationToken>,
    ) -> Self::TcpRead<'a> {
        todo!();
    }

    fn write_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpStream,
        buf: &'a [u8],
        ct: Option<CancellationToken>,
    ) -> Self::TcpWrite<'a> {
        todo!();
    }

    fn delay(&self, dur: Duration, ct: Option<CancellationToken>) -> Delay<'_> {
        Delay::new(dur, ct)
    }
}

/// An object to be using as a completion key for non I/O.
enum Event {
    TaskReady(Box<dyn Future<Output = ()> + Send>),
}
