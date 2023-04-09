use self::tcp::{TcpAccept, TcpRead, TcpWrite};
use self::time::Delay;
use crate::cancel::CancellationToken;
use crate::{EventQueue, Runtime};
use std::future::Future;
use std::net::{TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::time::Duration;

pub mod tcp;
pub mod time;

/// An implementation of [`Runtime`] backed by an I/O completion port.
pub struct Iocp {}

impl Iocp {
    pub fn new() -> &'static Self {
        Box::leak(Box::new(Self {}))
    }
}

impl EventQueue for Iocp {
    fn thread_count(&self) -> NonZeroUsize {
        todo!();
    }

    fn dequeue(
        &self,
        ready: &mut Vec<Pin<Box<dyn Future<Output = ()> + Send>>>,
    ) -> std::io::Result<bool> {
        todo!();
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