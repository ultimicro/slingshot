use super::Iocp;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use std::future::Future;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Represents a future for [`Runtime::accept_tcp()`].
pub struct TcpAccept<'a> {
    iocp: &'a Iocp,
    tcp: &'a mut TcpListener,
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
}

impl<'a> TcpAccept<'a> {
    pub(crate) fn new(
        iocp: &'a Iocp,
        tcp: &'a mut TcpListener,
        ct: Option<CancellationToken>,
    ) -> Self {
        Self {
            iocp,
            tcp,
            ct,
            ch: None,
        }
    }
}

impl<'a> Future for TcpAccept<'a> {
    type Output = std::io::Result<(TcpStream, SocketAddr)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!();
    }
}

/// Represents a future for [`Runtime::read_tcp()`].
pub struct TcpRead<'a> {
    iocp: &'a Iocp,
    tcp: &'a mut TcpStream,
    buf: &'a mut [u8],
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
}

impl<'a> TcpRead<'a> {
    pub(crate) fn new(
        iocp: &'a Iocp,
        tcp: &'a mut TcpStream,
        buf: &'a mut [u8],
        ct: Option<CancellationToken>,
    ) -> Self {
        Self {
            iocp,
            tcp,
            buf,
            ct,
            ch: None,
        }
    }
}

impl<'a> Future for TcpRead<'a> {
    type Output = std::io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!();
    }
}

/// Represents a future for [`Runtime::write_tcp()`].
pub struct TcpWrite<'a> {
    iocp: &'a Iocp,
    tcp: &'a mut TcpStream,
    buf: &'a [u8],
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
}

impl<'a> TcpWrite<'a> {
    pub(crate) fn new(
        iocp: &'a Iocp,
        tcp: &'a mut TcpStream,
        buf: &'a [u8],
        ct: Option<CancellationToken>,
    ) -> Self {
        Self {
            iocp,
            tcp,
            buf,
            ct,
            ch: None,
        }
    }
}

impl<'a> Future for TcpWrite<'a> {
    type Output = std::io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!();
    }
}
