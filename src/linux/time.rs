use super::Epoll;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use crate::fd::Fd;
use crate::future::io_cancel;
use libc::{
    itimerspec, read, timerfd_create, timerfd_settime, timespec, CLOCK_MONOTONIC, EPOLLIN,
    TFD_CLOEXEC, TFD_NONBLOCK,
};
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::ops::DerefMut;
use std::pin::Pin;
use std::ptr::null_mut;
use std::task::{Context, Poll};
use std::time::Duration;

/// A future of [`Runtime::delay()`].
pub struct Delay<'a> {
    ep: &'a Epoll,
    dur: Duration,
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
    fd: Option<Fd>,
}

impl<'a> Delay<'a> {
    pub(crate) fn new(ep: &'a Epoll, dur: Duration, ct: Option<CancellationToken>) -> Self {
        Self {
            ep,
            dur,
            ct,
            ch: None,
            fd: None,
        }
    }
}

impl<'a> Future for Delay<'a> {
    type Output = std::io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.ch = None;

        if let Some(ct) = &f.ct {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Get the timerfd.
        let fd = match &f.fd {
            Some(v) => v.get(),
            None => {
                // Check if duration is zero.
                if f.dur.is_zero() {
                    return Poll::Ready(Ok(()));
                }

                // Setup itimerspec.
                let spec = itimerspec {
                    it_value: timespec {
                        tv_sec: match f.dur.as_secs().try_into() {
                            Ok(v) => v,
                            Err(_) => {
                                return Poll::Ready(Err(Error::new(
                                    ErrorKind::Other,
                                    "the specified duration is too large",
                                )))
                            }
                        },
                        tv_nsec: match f.dur.subsec_nanos().try_into() {
                            Ok(v) => v,
                            Err(_) => {
                                return Poll::Ready(Err(Error::new(
                                    ErrorKind::Other,
                                    "the specified duration is too large",
                                )))
                            }
                        },
                    },
                    it_interval: timespec {
                        tv_sec: 0,
                        tv_nsec: 0,
                    },
                };

                // Create the timerfd.
                let fd = unsafe { timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK | TFD_CLOEXEC) };

                if fd < 0 {
                    return Poll::Ready(Err(Error::last_os_error()));
                }

                let fd = Fd::new(fd);

                // Start the timerfd.
                if unsafe { timerfd_settime(fd.get(), 0, &spec, null_mut()) } < 0 {
                    return Poll::Ready(Err(Error::last_os_error()));
                }

                f.fd = Some(fd);
                f.fd.as_ref().unwrap().get()
            }
        };

        // Read the timerfd.
        let mut buf = 0u64.to_ne_bytes();

        if unsafe { read(fd, buf.as_mut_ptr() as _, 8) } < 0 {
            // Check for expiration.
            let e = Error::last_os_error();

            if e.kind() != ErrorKind::WouldBlock {
                return Poll::Ready(Err(e));
            }

            // Watch for ready.
            if let Poll::Ready(v) = f.ep.watch_fd(cx, fd, EPOLLIN) {
                return Poll::Ready(v);
            }

            f.ch = f.ep.watch_cancel(cx, &f.ct);

            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }
}
