use super::overlapped::{Overlapped, OverlappedData, OverlappedResult};
use super::socket::Socket;
use super::Iocp;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use crate::future::{io_cancel, watch_cancel};
use std::future::Future;
use std::io::Error;
use std::mem::{size_of, transmute};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::ops::DerefMut;
use std::os::windows::prelude::{AsRawSocket, FromRawSocket};
use std::pin::Pin;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, FALSE};
use windows_sys::Win32::Networking::WinSock::{
    socket, AcceptEx, GetAcceptExSockaddrs, AF_INET, AF_INET6, INVALID_SOCKET, IPPROTO_TCP,
    SOCKADDR_IN, SOCKADDR_IN6, SOCKET, SOCK_STREAM,
};

/// Represents a future for [`Iocp::accept_tcp()`].
pub struct TcpAccept<'a> {
    iocp: &'a Iocp,
    tcp: &'a mut TcpListener,
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
    pending: Option<Arc<Mutex<AcceptPending>>>,
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
            pending: None,
        }
    }
}

impl<'a> Future for TcpAccept<'a> {
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

        // Check for pending.
        match &f.pending {
            Some(p) => {
                // Check if completed.
                let mut p = p.lock().unwrap();

                if let Some(r) = p.result.take() {
                    let r = match r {
                        OverlappedResult::Error(e) => Err(e),
                        OverlappedResult::Success(buf) => unsafe {
                            // Get addresses.
                            let mut local = null_mut();
                            let mut local_len = 0;
                            let mut remote = null_mut();
                            let mut remote_len = 0;

                            GetAcceptExSockaddrs(
                                buf.as_ptr() as _,
                                0,
                                p.addr_len,
                                p.addr_len,
                                &mut local,
                                &mut local_len,
                                &mut remote,
                                &mut remote_len,
                            );

                            // Construct a remote address.
                            let (ip, port) = match (*remote).sa_family {
                                AF_INET => {
                                    let a = remote as *mut SOCKADDR_IN;

                                    (
                                        IpAddr::from(transmute::<_, [u8; 4]>((*a).sin_addr)),
                                        u16::from_be((*a).sin_port),
                                    )
                                }
                                AF_INET6 => {
                                    let a = remote as *mut SOCKADDR_IN6;

                                    (
                                        IpAddr::from(transmute::<_, [u8; 16]>((*a).sin6_addr)),
                                        u16::from_be((*a).sin6_port),
                                    )
                                }
                                _ => {
                                    unreachable!("the remote address should be AF_INET or AF_INET6")
                                }
                            };

                            // Get cliet socket.
                            let client = p.client.take().unwrap().into_raw();

                            Ok((
                                TcpStream::from_raw_socket(client as _),
                                SocketAddr::new(ip, port),
                            ))
                        },
                    };

                    return Poll::Ready(r);
                }
            }
            None => {
                // Get address info.
                let (af, addr_len) = match f.tcp.local_addr() {
                    Ok(v) => {
                        if v.is_ipv4() {
                            (AF_INET, size_of::<SOCKADDR_IN>() as u32)
                        } else {
                            (AF_INET6, size_of::<SOCKADDR_IN6>() as u32)
                        }
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                };

                // Create a client socket.
                let client = {
                    let s = unsafe { socket(af as _, SOCK_STREAM, IPPROTO_TCP) };

                    if s == INVALID_SOCKET {
                        return Poll::Ready(Err(Error::last_os_error()));
                    }

                    Socket::new(s)
                };

                // Construct an overlapped data.
                let cfd = client.get();
                let addr_len = addr_len + 16;
                let p = Arc::new(Mutex::new(AcceptPending {
                    client: Some(client),
                    addr_len,
                    result: None,
                }));

                // Register the socket to IOCP.
                let sfd = f.tcp.as_raw_socket() as SOCKET;

                if let Err(e) = f.iocp.register_handle(sfd as _) {
                    return Poll::Ready(Err(e));
                }

                // Do the accept.
                let waker = cx.waker().clone();
                let buf = vec![0u8; (addr_len * 2) as usize];
                let mut received = 0;
                let overlapped =
                    Box::into_raw(Overlapped::new(None, buf, waker, Arc::downgrade(&p)));

                if unsafe {
                    AcceptEx(
                        sfd,
                        cfd,
                        (*overlapped).buf() as _,
                        0,
                        addr_len,
                        addr_len,
                        &mut received,
                        overlapped as _,
                    )
                } == FALSE
                {
                    let e = Error::last_os_error();

                    if e.raw_os_error().unwrap() != ERROR_IO_PENDING as _ {
                        drop(unsafe { Box::from_raw(overlapped) });
                        return Poll::Ready(Err(e));
                    }
                }

                // Keep pending data.
                f.pending = Some(p);
            }
        }

        // Watch for cancel.
        f.ch = watch_cancel(cx, &f.ct);

        Poll::Pending
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

/// Implementation of [`OverlappedData`] for [`TcpAccept`].
struct AcceptPending {
    client: Option<Socket>,
    addr_len: u32,
    result: Option<OverlappedResult>,
}

impl OverlappedData for Mutex<AcceptPending> {
    fn set_completed(&self, r: OverlappedResult) {
        self.lock().unwrap().result = Some(r);
    }
}
