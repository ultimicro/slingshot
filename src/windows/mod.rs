use self::handle::Handle;
use self::tcp::{TcpAccept, TcpRead, TcpWrite};
use self::time::Delay;
use crate::cancel::CancellationToken;
use crate::{EventQueue, Runtime};
use std::future::Future;
use std::io::Error;
use std::mem::MaybeUninit;
use std::net::{TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::thread::available_parallelism;
use std::time::Duration;
use windows_sys::Win32::Foundation::{FALSE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::IO::{
    CreateIoCompletionPort, GetQueuedCompletionStatusEx, OVERLAPPED_ENTRY,
};

pub mod handle;
pub mod tcp;
pub mod time;

/// An implementation of [`Runtime`] backed by an I/O completion port.
pub struct Iocp {
    iocp: Handle,
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

        Box::leak(Box::new(Self { iocp }))
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
        // Wait for the events.
        let mut events: [MaybeUninit<OVERLAPPED_ENTRY>; 64] =
            unsafe { MaybeUninit::uninit().assume_init() };
        let mut count = 0;

        if unsafe {
            GetQueuedCompletionStatusEx(
                self.iocp.get(),
                events.as_mut_ptr() as _,
                64,
                &mut count,
                0xffffffff,
                FALSE,
            )
        } == FALSE
        {
            return Err(Error::last_os_error());
        }

        todo!()
    }

    fn drop_task(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>) -> bool {
        todo!();
    }

    fn push_ready(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>) {
        todo!();
    }
}

impl Runtime for Iocp {
    type TcpAccept<'a> = TcpAccept<'a>;
    type TcpRead<'a> = TcpRead<'a>;
    type TcpWrite<'a> = TcpWrite<'a>;
    type Delay<'a> = Delay<'a>;

    fn spawn<T: Future<Output = ()> + Send + 'static>(&self, task: T) -> Option<T> {
        todo!();
    }

    fn accept_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpListener,
        ct: Option<CancellationToken>,
    ) -> Self::TcpAccept<'a> {
        todo!();
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

    fn delay(&self, dur: Duration, ct: Option<CancellationToken>) -> Self::Delay<'_> {
        todo!();
    }
}
