use self::tcp::{Accepting, Reading, Writing};
use self::time::Delay;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use crate::fd::Fd;
use crate::{EventQueue, Runtime};
use libc::{
    epoll_create1, epoll_ctl, epoll_event, epoll_wait, eventfd, read, write, EAGAIN, EFD_CLOEXEC,
    EFD_NONBLOCK, ENOENT, EPOLLET, EPOLLIN, EPOLLONESHOT, EPOLL_CLOEXEC, EPOLL_CTL_ADD,
    EPOLL_CTL_MOD,
};
use std::ffi::c_int;
use std::future::Future;
use std::io::Error;
use std::mem::{size_of, transmute};
use std::net::{TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};
use std::thread::available_parallelism;
use std::time::Duration;

pub mod tcp;
pub mod time;

/// An implementation of [`Runtime`] backed by an epoll.
pub struct Epoll {
    ready_pipes: Vec<(Fd, Fd)>, // Length is the number of worker thread.
    shutdown_event: Fd,         // An eventfd that is always readable when we want to shutdown.
    fd: Fd,                     // Drop as the last FD.
    active_tasks: AtomicIsize,  // Negative if being shutting down.
    next_ready: AtomicUsize,
}

impl Epoll {
    pub fn new() -> &'static Self {
        // Get the number of worker thread.
        let worker_count = match available_parallelism() {
            Ok(v) => match v.get() {
                1 => {
                    // We cannot run with a single thread because our push_ready() implementation
                    // will be block when the pipe buffer is full, which is a dead lock on a single
                    // thread due to no other thread pull the data from the pipe.
                    2
                }
                v => v,
            },
            Err(e) => panic!("cannot get the number of worker thread: {e}"),
        };

        // Create an epoll.
        let fd = {
            let v = unsafe { epoll_create1(EPOLL_CLOEXEC) };

            if v < 0 {
                panic!("cannot create an epoll: {}", Error::last_os_error());
            }

            Fd::new(v)
        };

        // Create a shutdown event.
        let shutdown_event = {
            let v = unsafe { eventfd(0, EFD_CLOEXEC | EFD_NONBLOCK) };

            if v < 0 {
                panic!("cannot create a shutdown event: {}", Error::last_os_error());
            }

            Fd::new(v)
        };

        // Add the shutdown event to the epoll.
        let mut event = epoll_event {
            events: EPOLLIN as _,
            u64: 0,
        };

        if unsafe { epoll_ctl(fd.get(), EPOLL_CTL_ADD, shutdown_event.get(), &mut event) } < 0 {
            let e = Error::last_os_error();
            panic!("cannot register the shutdown event to the epoll: {e}");
        }

        // Create pipes to transfer the ready tasks.
        let mut ready_pipes: Vec<(Fd, Fd)> = Vec::with_capacity(worker_count);

        for i in 0..worker_count {
            // Create a pipe.
            let pipe = match Fd::pipe() {
                Ok(v) => v,
                Err(e) => panic!("cannot create a pipe to transfer the ready tasks: {e}"),
            };

            // Enable non-blocking on on the read end.
            if let Err(e) = pipe.0.set_nonblocking() {
                panic!(
                    "cannot enable non-blocking mode on the pipe to transfer the ready tasks: {e}"
                );
            }

            // Add the read end to the epoll.
            let data = Box::into_raw(Box::new(Event::TaskReady(i)));

            event.events = (EPOLLIN | EPOLLET | EPOLLONESHOT) as _;
            event.u64 = unsafe { transmute(data) };

            if unsafe { epoll_ctl(fd.get(), EPOLL_CTL_ADD, pipe.0.get(), &mut event) } < 0 {
                let e = Error::last_os_error();
                panic!("cannot register the pipe to transfer the ready tasks to the epoll: {e}");
            }

            ready_pipes.push(pipe);
        }

        Box::leak(Box::new(Self {
            ready_pipes,
            shutdown_event,
            fd,
            active_tasks: AtomicIsize::new(0),
            next_ready: AtomicUsize::new(0),
        }))
    }

    fn shutdown(&self) {
        let v = 1u64.to_ne_bytes();

        if unsafe { write(self.shutdown_event.get(), v.as_ptr() as _, 8) } < 0 {
            let e = Error::last_os_error();
            panic!("cannot write to the shutdown event: {e}");
        }
    }

    fn watch_fd<T>(&self, cx: &mut Context, fd: c_int, ev: c_int) -> Poll<std::io::Result<T>> {
        // Setup an epoll_event.
        let waker = cx.waker().clone();
        let data = Box::into_raw(Box::new(Event::IoReady(waker)));
        let mut event = epoll_event {
            events: (ev | EPOLLET | EPOLLONESHOT) as _,
            u64: unsafe { transmute(data) },
        };

        // Enable the FD. When adding or re-enable the FD the epoll will check if the FD is
        // ready so no race condition is possible here. See
        // https://stackoverflow.com/q/12920243/1829232 for more information.
        if unsafe { epoll_ctl(self.fd.get(), EPOLL_CTL_MOD, fd, &mut event) } < 0 {
            // Check if we need to use EPOLL_CTL_ADD instead.
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != ENOENT {
                drop(unsafe { Box::from_raw(data) });
                return Poll::Ready(Err(e));
            }

            // Add the FD to the epoll.
            if unsafe { epoll_ctl(self.fd.get(), EPOLL_CTL_ADD, fd, &mut event) } < 0 {
                let e = Error::last_os_error();
                drop(unsafe { Box::from_raw(data) });
                return Poll::Ready(Err(e));
            }
        }

        Poll::Pending
    }

    fn watch_cancel(
        &self,
        cx: &mut Context,
        ct: &Option<CancellationToken>,
    ) -> Option<SubscriptionHandle> {
        if let Some(ct) = ct {
            let waker = cx.waker().clone();
            ct.subscribe(move || waker.wake())
        } else {
            None
        }
    }
}

impl EventQueue for Epoll {
    fn thread_count(&self) -> NonZeroUsize {
        NonZeroUsize::new(self.ready_pipes.len()).unwrap()
    }

    fn dequeue(
        &self,
        ready: &mut Vec<Pin<Box<dyn Future<Output = ()> + Send>>>,
    ) -> std::io::Result<bool> {
        // Wait for some events.
        let mut raws = [epoll_event { events: 0, u64: 0 }; 32];
        let count = loop {
            let r = unsafe { epoll_wait(self.fd.get(), raws.as_mut_ptr(), 32, -1) };

            if r < 0 {
                let e = Error::last_os_error();

                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                } else {
                    self.shutdown();
                    return Err(e);
                }
            } else {
                break r as usize;
            }
        };

        // Process the events. This loop MUST not break in the middle otherwise the memory may
        // be leak.
        for i in 0..count {
            // Get the event data.
            let event = match raws[i].u64 {
                0 => {
                    // It is okay to break the loop when we received a shutdown signal. The only
                    // possible cases is epoll_wait() was error on the other thread or all tasks has
                    // been completed.
                    //
                    // For the former case no point to process the other events because we are going
                    // to shutdown anyway. For the later case we are sure no other is available.
                    return Ok(false);
                }
                v => unsafe { *Box::from_raw(v as *mut Event) },
            };

            // Handle the event.
            match event {
                Event::TaskReady(i) => {
                    // Read the tasks from the pipe.
                    const SIZE: usize = size_of::<*mut (dyn Future<Output = ()> + Send)>();
                    let fd = &self.ready_pipes[i].0;
                    let mut ptr = [0u8; SIZE];

                    loop {
                        if unsafe { read(fd.get(), ptr.as_mut_ptr() as _, SIZE) } < 0 {
                            let e = Error::last_os_error();

                            if e.raw_os_error().unwrap() == EAGAIN {
                                break;
                            }

                            panic!("cannot read a task from {fd}: {e}");
                        }

                        ready.push(unsafe { Box::into_pin(Box::from_raw(transmute(ptr))) });
                    }

                    // Re-enable the FD.
                    let data = Box::into_raw(Box::new(Event::TaskReady(i)));
                    let mut event = epoll_event {
                        events: (EPOLLIN | EPOLLET | EPOLLONESHOT) as _,
                        u64: unsafe { transmute(data) },
                    };

                    if unsafe { epoll_ctl(self.fd.get(), EPOLL_CTL_MOD, fd.get(), &mut event) } < 0
                    {
                        let e = Error::last_os_error();
                        panic!("cannot re-enable {fd} on the epoll: {e}");
                    }
                }
                Event::IoReady(w) => w.wake(),
            }
        }

        Ok(true)
    }

    fn drop_task(&self, _: Pin<Box<dyn Future<Output = ()> + Send>>) -> bool {
        // Decrease the number of active task.
        if self.active_tasks.fetch_sub(1, Ordering::AcqRel) != 1 {
            return true;
        }

        // Try to shutdown.
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
        // Get the ready pipe.
        let fd = loop {
            // Get the next pipe.
            let i = self.next_ready.fetch_add(1, Ordering::AcqRel);

            if let Some(v) = self.ready_pipes.get(i) {
                break &v.1;
            }

            // Reset the next pipe to zero.
            let _ = self
                .next_ready
                .compare_exchange(i + 1, 0, Ordering::AcqRel, Ordering::Acquire);
        };

        // Send the task to the ready pipe.
        let task = unsafe { Pin::into_inner_unchecked(task) };
        let data = Box::into_raw(task);
        let size = size_of::<*mut (dyn Future<Output = ()> + Send)>();

        // POSIX required a write to the pipe to be atomic if the size of data is not exceed
        // _POSIX_PIPE_BUF, which is 512 bytes. That mean we don't need to check the number of bytes
        // written here.
        if unsafe { write(fd.get(), transmute(&data), size) } < 0 {
            let e = Error::last_os_error();
            panic!("cannot write to the ready pipe: {e}");
        }
    }
}

impl Runtime for Epoll {
    type TcpAccept<'a> = Accepting<'a>;
    type TcpRead<'a> = Reading<'a>;
    type TcpWrite<'a> = Writing<'a>;
    type Delay<'a> = Delay<'a>;

    fn spawn<T: Future<Output = ()> + Send + 'static>(&self, task: T) -> Option<T> {
        // Check if being shutting down.
        if self.active_tasks.fetch_add(1, Ordering::AcqRel) < 0 {
            return Some(task);
        }

        // Send the task.
        self.push_ready(Box::pin(task));

        None
    }

    fn accept_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpListener,
        ct: Option<CancellationToken>,
    ) -> Accepting<'a> {
        Accepting::new(self, tcp, ct)
    }

    fn read_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpStream,
        buf: &'a mut [u8],
        ct: Option<CancellationToken>,
    ) -> Reading<'a> {
        Reading::new(self, tcp, buf, ct)
    }

    fn write_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpStream,
        buf: &'a [u8],
        ct: Option<CancellationToken>,
    ) -> Writing<'a> {
        Writing::new(self, tcp, buf, ct)
    }

    fn delay<'a>(&'a self, dur: Duration, ct: Option<CancellationToken>) -> Delay<'a> {
        Delay::new(self, dur, ct)
    }
}

/// A data for each epoll event.
enum Event {
    TaskReady(usize),
    IoReady(Waker),
}
