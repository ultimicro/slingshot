use super::mem::Buffer;
use super::overlapped::{Overlapped, OverlappedData, OverlappedResult};
use super::socket::Socket;
use super::Iocp;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use crate::future::{io_cancel, watch_cancel};
use std::cmp::min;
use std::future::Future;
use std::io::Error;
use std::mem::{size_of, transmute};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::ops::DerefMut;
use std::os::windows::prelude::{AsRawSocket, FromRawSocket};
use std::pin::Pin;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, ERROR_NOT_FOUND, FALSE};
use windows_sys::Win32::Networking::WinSock::{
    socket, AcceptEx, GetAcceptExSockaddrs, WSARecv, WSASend, AF_INET, AF_INET6, INVALID_SOCKET,
    IPPROTO_TCP, SOCKADDR_IN, SOCKADDR_IN6, SOCKET, SOCKET_ERROR, SOCK_STREAM, WSABUF,
    WSA_IO_PENDING,
};
use windows_sys::Win32::System::IO::CancelIoEx;

/// Represents a future for [`Iocp::accept_tcp()`].
pub struct TcpAccept<'a> {
    iocp: &'a Iocp,
    tcp: &'a mut TcpListener,
    cancellation_token: Option<CancellationToken>,
    cancellation_handle: Option<SubscriptionHandle>,
    pending: Option<Arc<Mutex<AcceptPending>>>,
}

impl<'a> TcpAccept<'a> {
    pub(crate) fn new(
        iocp: &'a Iocp,
        tcp: &'a mut TcpListener,
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            iocp,
            tcp,
            cancellation_token,
            cancellation_handle: None,
            pending: None,
        }
    }

    fn begin_accept(&mut self, cx: &mut Context) -> Result<Arc<Mutex<AcceptPending>>, Error> {
        // Get address info.
        let (af, addr_len) = match self.tcp.local_addr() {
            Ok(v) => {
                if v.is_ipv6() {
                    (AF_INET6, size_of::<SOCKADDR_IN6>() as u32)
                } else {
                    (AF_INET, size_of::<SOCKADDR_IN>() as u32)
                }
            }
            Err(e) => return Err(e),
        };

        // Create a client socket.
        let client = {
            let s = unsafe { socket(af as _, SOCK_STREAM, IPPROTO_TCP) };

            if s == INVALID_SOCKET {
                return Err(Error::last_os_error());
            }

            Socket::new(s)
        };

        // Construct an overlapped data.
        let client_sock = client.get();
        let addr_len = addr_len + 16;
        let pending = Arc::new(Mutex::new(AcceptPending {
            client: Some(client),
            addr_len,
            result: None,
            waker: Some(cx.waker().clone()),
        }));

        // Register the socket to IOCP.
        let server = self.tcp.as_raw_socket() as SOCKET;

        self.iocp.register_handle(server as _)?;

        // Do the accept.
        let buf = Buffer::new((addr_len * 2) as usize);
        let mut received = 0;
        let overlapped = Box::into_raw(Overlapped::new(None, buf, Arc::downgrade(&pending)));

        if unsafe {
            AcceptEx(
                server,
                client_sock,
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
                return Err(e);
            }
        }

        Ok(pending)
    }

    fn end_accept(pending: &mut AcceptPending) -> Result<Option<(TcpStream, SocketAddr)>, Error> {
        // Check if completed.
        let result = match pending.result.take() {
            Some(v) => v,
            None => return Ok(None),
        };

        // Get the result.
        let buf = match result {
            OverlappedResult::Error(e) => return Err(e),
            OverlappedResult::Success(buf, _) => buf,
        };

        // Get the addresses.
        let mut local = null_mut();
        let mut local_len = 0;
        let mut remote = null_mut();
        let mut remote_len = 0;

        unsafe {
            GetAcceptExSockaddrs(
                buf.get() as _,
                0,
                pending.addr_len,
                pending.addr_len,
                &mut local,
                &mut local_len,
                &mut remote,
                &mut remote_len,
            )
        };

        // Construct a remote address.
        let (ip, port) = match unsafe { (*remote).sa_family } {
            AF_INET => unsafe {
                let addr = remote as *const SOCKADDR_IN;
                let ip = IpAddr::from(transmute::<_, [u8; 4]>((*addr).sin_addr));
                let port = u16::from_be((*addr).sin_port);

                (ip, port)
            },
            AF_INET6 => unsafe {
                let addr = remote as *const SOCKADDR_IN6;
                let ip = IpAddr::from(transmute::<_, [u8; 16]>((*addr).sin6_addr));
                let port = u16::from_be((*addr).sin6_port);

                (ip, port)
            },
            _ => unreachable!("the remote address should always be AF_INET or AF_INET6"),
        };

        // Get cliet socket.
        let sock = pending.client.take().unwrap().into_raw();
        let stream = unsafe { TcpStream::from_raw_socket(sock as _) };
        let addr = SocketAddr::new(ip, port);

        Ok(Some((stream, addr)))
    }
}

impl<'a> Future for TcpAccept<'a> {
    type Output = std::io::Result<(TcpStream, SocketAddr)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.cancellation_handle = None;

        if let Some(ct) = &f.cancellation_token {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Check for pending.
        match &f.pending {
            Some(p) => {
                let mut p = p.lock().unwrap();

                match Self::end_accept(&mut p) {
                    Ok(v) => {
                        if let Some(v) = v {
                            return Poll::Ready(Ok(v));
                        }
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                }

                p.waker = Some(cx.waker().clone());
            }
            None => match f.begin_accept(cx) {
                Ok(v) => f.pending = Some(v),
                Err(e) => return Poll::Ready(Err(e)),
            },
        }

        // Watch for cancel.
        f.cancellation_handle = watch_cancel(cx, &f.cancellation_token);

        Poll::Pending
    }
}

/// Represents a future for [`Iocp::read_tcp()`].
pub struct TcpRead<'a> {
    iocp: &'a Iocp,
    tcp: &'a mut TcpStream,
    buf: &'a mut [u8],
    cancellation_token: Option<CancellationToken>,
    cancellation_handle: Option<SubscriptionHandle>,
    pending: Option<Arc<Mutex<ReadPending>>>,
}

impl<'a> TcpRead<'a> {
    pub(crate) fn new(
        iocp: &'a Iocp,
        tcp: &'a mut TcpStream,
        buf: &'a mut [u8],
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            iocp,
            tcp,
            buf,
            cancellation_token,
            cancellation_handle: None,
            pending: None,
        }
    }

    fn begin_read(&mut self, cx: &mut Context) -> Result<Arc<Mutex<ReadPending>>, Error> {
        // Allocate a buffer.
        let buf_len = min(self.buf.len(), u32::MAX as usize); // No plan to support Windows 16-bits.
        let buf = Buffer::new(buf_len);

        // Setup pending data.
        let socket = self.tcp.as_raw_socket() as SOCKET;
        let pending = Arc::new(Mutex::new(ReadPending {
            cancel: Some(socket),
            result: None,
            waker: Some(cx.waker().clone()),
        }));

        // Register the socket to IOCP.
        self.iocp.register_handle(socket as _)?;

        // Do the receive.
        let overlapped = Box::into_raw(Overlapped::new(None, buf, Arc::downgrade(&pending)));
        let mut flags = 0;

        if unsafe {
            WSARecv(
                socket,
                &WSABUF {
                    len: buf_len as _,
                    buf: (*overlapped).buf(),
                },
                1,
                null_mut(),
                &mut flags,
                overlapped as _,
                None,
            )
        } == SOCKET_ERROR
        {
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != WSA_IO_PENDING {
                drop(unsafe { Box::from_raw(overlapped) });
                return Err(e);
            }
        }

        Ok(pending)
    }

    fn end_read(pending: &mut ReadPending) -> Result<Option<Vec<u8>>, Error> {
        // Get the result.
        let result = match pending.result.take() {
            Some(v) => v,
            None => return Ok(None),
        };

        // Extract the result.
        match result {
            OverlappedResult::Error(e) => Err(e),
            OverlappedResult::Success(buf, transferred) => unsafe {
                Ok(Some(buf.into_vec(transferred)))
            },
        }
    }
}

impl<'a> Future for TcpRead<'a> {
    type Output = std::io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.cancellation_handle = None;

        if let Some(ct) = &f.cancellation_token {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Check if operation is pending.
        match &f.pending {
            Some(p) => {
                // Check if completed.
                let mut p = p.lock().unwrap();

                match Self::end_read(&mut p) {
                    Ok(v) => {
                        if let Some(v) = v {
                            f.buf[..v.len()].copy_from_slice(&v);
                            return Poll::Ready(Ok(v.len()));
                        }
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                }

                p.waker = Some(cx.waker().clone());
            }
            None => {
                // Check if we don't do the actual receive.
                if f.buf.is_empty() {
                    return Poll::Ready(Ok(0));
                }

                f.pending = match f.begin_read(cx) {
                    Ok(v) => Some(v),
                    Err(e) => return Poll::Ready(Err(e)),
                };
            }
        }

        // Watch for cancel.
        f.cancellation_handle = watch_cancel(cx, &f.cancellation_token);

        Poll::Pending
    }
}

/// Represents a future for [`Runtime::write_tcp()`].
pub struct TcpWrite<'a> {
    iocp: &'a Iocp,
    tcp: &'a mut TcpStream,
    buf: &'a [u8],
    cancellation_token: Option<CancellationToken>,
    cancellation_handle: Option<SubscriptionHandle>,
    pending: Option<Arc<Mutex<WritePending>>>,
}

impl<'a> TcpWrite<'a> {
    pub(crate) fn new(
        iocp: &'a Iocp,
        tcp: &'a mut TcpStream,
        buf: &'a [u8],
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            iocp,
            tcp,
            buf,
            cancellation_token,
            cancellation_handle: None,
            pending: None,
        }
    }

    fn begin_write(&mut self, cx: &mut Context) -> Result<Arc<Mutex<WritePending>>, Error> {
        // Copy data to send buffer.
        let buf_len = min(self.buf.len(), u32::MAX as usize); // No plan to support Windows 16-bits.
        let buf = self.buf[..buf_len].try_into().unwrap();

        // Setup pending data.
        let socket = self.tcp.as_raw_socket() as SOCKET;
        let pending = Arc::new(Mutex::new(WritePending {
            cancel: Some(socket),
            result: None,
            waker: Some(cx.waker().clone()),
        }));

        // Register the socket to IOCP.
        self.iocp.register_handle(socket as _)?;

        // Do the receive.
        let overlapped = Box::into_raw(Overlapped::new(None, buf, Arc::downgrade(&pending)));

        if unsafe {
            WSASend(
                socket,
                &WSABUF {
                    len: buf_len as _,
                    buf: (*overlapped).buf(),
                },
                1,
                null_mut(),
                0,
                overlapped as _,
                None,
            )
        } == SOCKET_ERROR
        {
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != WSA_IO_PENDING {
                drop(unsafe { Box::from_raw(overlapped) });
                return Err(e);
            }
        }

        Ok(pending)
    }

    fn end_write(pending: &mut WritePending) -> Result<Option<usize>, Error> {
        // Check if operation is completed.
        let result = match pending.result.take() {
            Some(v) => v,
            None => return Ok(None),
        };

        // Check the result.
        match result {
            OverlappedResult::Error(e) => Err(e),
            OverlappedResult::Success(_, transferred) => Ok(Some(transferred)),
        }
    }
}

impl<'a> Future for TcpWrite<'a> {
    type Output = std::io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.cancellation_handle = None;

        if let Some(ct) = &f.cancellation_token {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Check if operation is pending.
        match &f.pending {
            Some(p) => {
                let mut p = p.lock().unwrap();

                match Self::end_write(&mut p) {
                    Ok(v) => {
                        if let Some(v) = v {
                            return Poll::Ready(Ok(v));
                        }
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                }

                p.waker = Some(cx.waker().clone());
            }
            None => {
                // Check if we don't need to do the actual write.
                if f.buf.is_empty() {
                    return Poll::Ready(Ok(0));
                }

                // Start writing.
                f.pending = match f.begin_write(cx) {
                    Ok(v) => Some(v),
                    Err(e) => return Poll::Ready(Err(e)),
                };
            }
        }

        // Watch for cancel.
        f.cancellation_handle = watch_cancel(cx, &f.cancellation_token);

        Poll::Pending
    }
}

/// Implementation of [`OverlappedData`] for [`TcpAccept`].
struct AcceptPending {
    client: Option<Socket>,
    addr_len: u32,
    result: Option<OverlappedResult>,
    waker: Option<Waker>,
}

impl OverlappedData for Mutex<AcceptPending> {
    fn set_completed(&self, r: OverlappedResult) -> Waker {
        let mut p = self.lock().unwrap();

        p.result = Some(r);
        p.waker.take().unwrap()
    }
}

/// Implementation of [`OverlappedData`] for [`TcpRead`].
struct ReadPending {
    cancel: Option<SOCKET>,
    result: Option<OverlappedResult>,
    waker: Option<Waker>,
}

impl Drop for ReadPending {
    fn drop(&mut self) {
        // Check if we need to cancel the pending I/O.
        let sock = match self.cancel {
            Some(v) => v,
            None => return,
        };

        // Cancel the I/O.
        if unsafe { CancelIoEx(sock as _, null()) } == 0 {
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != ERROR_NOT_FOUND as _ {
                panic!("cannot cancel pending I/O: {e}");
            }
        }
    }
}

impl OverlappedData for Mutex<ReadPending> {
    fn set_completed(&self, r: OverlappedResult) -> Waker {
        let mut p = self.lock().unwrap();

        p.result = Some(r);
        p.cancel = None;
        p.waker.take().unwrap()
    }
}

/// Implementation of [`OverlappedData`] for [`TcpWrite`].
struct WritePending {
    cancel: Option<SOCKET>,
    result: Option<OverlappedResult>,
    waker: Option<Waker>,
}

impl Drop for WritePending {
    fn drop(&mut self) {
        // Check if we need to cancel the pending I/O.
        let sock = match self.cancel {
            Some(v) => v,
            None => return,
        };

        // Cancel the I/O.
        if unsafe { CancelIoEx(sock as _, null()) } == 0 {
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != ERROR_NOT_FOUND as _ {
                panic!("cannot cancel pending I/O: {e}");
            }
        }
    }
}

impl OverlappedData for Mutex<WritePending> {
    fn set_completed(&self, r: OverlappedResult) -> Waker {
        let mut p = self.lock().unwrap();

        p.result = Some(r);
        p.cancel = None;
        p.waker.take().unwrap()
    }
}
