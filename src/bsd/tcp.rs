use super::Kqueue;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use std::future::Future;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::pin::Pin;
use std::task::{Context, Poll};

/// A future for [`Kqueue::accept_tcp()`].
pub struct TcpAccept<'a> {
    kq: &'a Kqueue,
    tcp: &'a mut TcpListener,
    cancellation_token: Option<CancellationToken>,
    cancellation_handle: Option<SubscriptionHandle>,
}

impl<'a> TcpAccept<'a> {
    pub(super) fn new(
        kq: &'a Kqueue,
        tcp: &'a mut TcpListener,
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            kq,
            tcp,
            cancellation_token,
            cancellation_handle: None,
        }
    }
}

impl<'a> Future for TcpAccept<'a> {
    type Output = std::io::Result<(TcpStream, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}

/// A future for [`Kqueue::read_tcp()`].
pub struct TcpRead<'a> {
    kq: &'a Kqueue,
    tcp: &'a mut TcpStream,
    buf: &'a mut [u8],
    cancellation_token: Option<CancellationToken>,
    cancellation_handle: Option<SubscriptionHandle>,
}

impl<'a> TcpRead<'a> {
    pub(super) fn new(
        kq: &'a Kqueue,
        tcp: &'a mut TcpStream,
        buf: &'a mut [u8],
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            kq,
            tcp,
            buf,
            cancellation_token,
            cancellation_handle: None,
        }
    }
}

impl<'a> Future for TcpRead<'a> {
    type Output = std::io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}

/// A future for [`Kqueue::write_tcp()`].
pub struct TcpWrite<'a> {
    kq: &'a Kqueue,
    tcp: &'a mut TcpStream,
    buf: &'a [u8],
    cancellation_token: Option<CancellationToken>,
    cancellation_handle: Option<SubscriptionHandle>,
}

impl<'a> TcpWrite<'a> {
    pub(super) fn new(
        kq: &'a Kqueue,
        tcp: &'a mut TcpStream,
        buf: &'a [u8],
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            kq,
            tcp,
            buf,
            cancellation_token,
            cancellation_handle: None,
        }
    }
}

impl<'a> Future for TcpWrite<'a> {
    type Output = std::io::Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}
