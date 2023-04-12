use super::Epoll;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use crate::future::{io_cancel, watch_cancel};
use libc::{EPOLLIN, EPOLLOUT};
use std::future::Future;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::ops::DerefMut;
use std::os::fd::AsRawFd;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Represents a future for [`Tcp::accept()`].
pub struct Accepting<'a> {
    ep: &'a Epoll,
    tcp: &'a mut TcpListener,
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
}

impl<'a> Accepting<'a> {
    pub(crate) fn new(
        ep: &'a Epoll,
        tcp: &'a mut TcpListener,
        ct: Option<CancellationToken>,
    ) -> Self {
        Self {
            ep,
            tcp,
            ct,
            ch: None,
        }
    }
}

impl<'a> Future for Accepting<'a> {
    type Output = std::io::Result<(TcpStream, SocketAddr)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.ch = None;

        if let Some(ct) = &f.ct {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Accept the connection.
        match f.tcp.accept() {
            Ok(v) => return Poll::Ready(Ok(v)),
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    return Poll::Ready(Err(e));
                }
            }
        }

        // Watch for ready.
        if let Poll::Ready(v) = f.ep.watch_fd(cx, f.tcp.as_raw_fd(), EPOLLIN) {
            return Poll::Ready(v);
        }

        f.ch = watch_cancel(cx, &f.ct);

        Poll::Pending
    }
}

/// Represents a future for [`Tcp::read()`].
pub struct Reading<'a> {
    ep: &'a Epoll,
    tcp: &'a mut TcpStream,
    buf: &'a mut [u8],
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
}

impl<'a> Reading<'a> {
    pub(crate) fn new(
        ep: &'a Epoll,
        tcp: &'a mut TcpStream,
        buf: &'a mut [u8],
        ct: Option<CancellationToken>,
    ) -> Self {
        Self {
            ep,
            tcp,
            buf,
            ct,
            ch: None,
        }
    }
}

impl<'a> Future for Reading<'a> {
    type Output = std::io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.ch = None;

        if let Some(ct) = &f.ct {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Read the socket.
        match f.tcp.read(f.buf) {
            Ok(v) => return Poll::Ready(Ok(v)),
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    return Poll::Ready(Err(e));
                }
            }
        }

        // Watch for ready.
        if let Poll::Ready(v) = f.ep.watch_fd(cx, f.tcp.as_raw_fd(), EPOLLIN) {
            return Poll::Ready(v);
        }

        f.ch = watch_cancel(cx, &f.ct);

        Poll::Pending
    }
}

/// Represents a future for [`Tcp::write()`].
pub struct Writing<'a> {
    ep: &'a Epoll,
    tcp: &'a mut TcpStream,
    buf: &'a [u8],
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
}

impl<'a> Writing<'a> {
    pub(crate) fn new(
        ep: &'a Epoll,
        tcp: &'a mut TcpStream,
        buf: &'a [u8],
        ct: Option<CancellationToken>,
    ) -> Self {
        Self {
            ep,
            tcp,
            buf,
            ct,
            ch: None,
        }
    }
}

impl<'a> Future for Writing<'a> {
    type Output = std::io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.ch = None;

        if let Some(ct) = &f.ct {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Read the socket.
        match f.tcp.write(f.buf) {
            Ok(v) => return Poll::Ready(Ok(v)),
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    return Poll::Ready(Err(e));
                }
            }
        }

        // Watch for ready.
        if let Poll::Ready(v) = f.ep.watch_fd(cx, f.tcp.as_raw_fd(), EPOLLOUT) {
            return Poll::Ready(v);
        }

        f.ch = watch_cancel(cx, &f.ct);

        Poll::Pending
    }
}
