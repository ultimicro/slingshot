use super::Iocp;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// A future of [`Runtime::delay()`].
pub struct Delay<'a> {
    iocp: &'a Iocp,
    dur: Duration,
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
}

impl<'a> Delay<'a> {
    pub(crate) fn new(iocp: &'a Iocp, dur: Duration, ct: Option<CancellationToken>) -> Self {
        Self {
            iocp,
            dur,
            ct,
            ch: None,
        }
    }
}

impl<'a> Future for Delay<'a> {
    type Output = std::io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!();
    }
}
