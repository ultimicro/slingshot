use self::tcp::{TcpAccept, TcpRead, TcpWrite};
use self::time::Delay;
use crate::cancel::CancellationToken;
use crate::fd::Fd;
use crate::{EventQueue, Runtime};
use libc::kqueue;
use std::future::Future;
use std::io::Error;
use std::net::{TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::time::Duration;

pub mod tcp;
pub mod time;

/// An implementation of [`Runtime`] backed by a kqueue.
pub struct Kqueue {
    fd: Fd,
}

impl Kqueue {
    pub fn new() -> &'static Self {
        // Create a kqueue.
        let fd = {
            let v = unsafe { kqueue() };

            if v < 0 {
                panic!("cannot create a kqueue: {}", Error::last_os_error());
            }

            Fd::new(v)
        };

        Box::leak(Box::new(Self { fd }))
    }
}

impl EventQueue for Kqueue {
    fn thread_count(&self) -> NonZeroUsize {
        todo!()
    }

    fn dequeue(
        &self,
        ready: &mut Vec<Pin<Box<dyn Future<Output = ()> + Send>>>,
    ) -> std::io::Result<bool> {
        todo!()
    }

    fn drop_task(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>) -> bool {
        todo!()
    }

    fn push_ready(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>) {
        todo!()
    }
}

impl Runtime for Kqueue {
    type TcpAccept<'a> = TcpAccept<'a>;
    type TcpRead<'a> = TcpRead<'a>;
    type TcpWrite<'a> = TcpWrite<'a>;
    type Delay<'a> = Delay<'a>;

    fn spawn<T: Future<Output = ()> + Send + 'static>(&self, task: T) -> Option<T> {
        todo!()
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
    ) -> TcpRead<'a> {
        TcpRead::new(self, tcp, buf, ct)
    }

    fn write_tcp<'a>(
        &'a self,
        tcp: &'a mut TcpStream,
        buf: &'a [u8],
        ct: Option<CancellationToken>,
    ) -> TcpWrite<'a> {
        TcpWrite::new(self, tcp, buf, ct)
    }

    fn delay(&self, dur: Duration, ct: Option<CancellationToken>) -> Delay<'_> {
        Delay::new(self, dur, ct)
    }
}
