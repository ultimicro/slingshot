use super::Epoll;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use crate::future::io_cancel;
use libc::{read, signalfd_siginfo, EPOLLIN};
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::mem::{size_of, MaybeUninit};
use std::ops::DerefMut;
use std::os::fd::AsRawFd;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future for [`Epoll::read_signal()`].
pub struct SignalRead<'a, S: AsRawFd> {
    ep: &'a Epoll,
    sig: &'a mut S,
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
}

impl<'a, S: AsRawFd> SignalRead<'a, S> {
    pub(super) fn new(ep: &'a Epoll, sig: &'a mut S, ct: Option<CancellationToken>) -> Self {
        Self {
            ep,
            sig,
            ct,
            ch: None,
        }
    }
}

impl<'a, S: AsRawFd> Future for SignalRead<'a, S> {
    type Output = std::io::Result<signalfd_siginfo>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.ch = None;

        if let Some(ct) = &f.ct {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Read the FD.
        let fd = f.sig.as_raw_fd();
        let mut info: MaybeUninit<signalfd_siginfo> = MaybeUninit::uninit();
        let size = size_of::<signalfd_siginfo>();

        if unsafe { read(fd, info.as_mut_ptr() as _, size) } < 0 {
            // Check if signal not available.
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
            Poll::Ready(Ok(unsafe { info.assume_init() }))
        }
    }
}
