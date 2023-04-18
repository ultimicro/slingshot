use super::Kqueue;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// A future for [`Kqueue::delay()`].
pub struct Delay<'a> {
    kq: &'a Kqueue,
    duration: Duration,
    cancellation_token: Option<CancellationToken>,
    cancellation_handle: Option<SubscriptionHandle>,
}

impl<'a> Delay<'a> {
    pub(super) fn new(
        kq: &'a Kqueue,
        duration: Duration,
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            kq,
            duration,
            cancellation_token,
            cancellation_handle: None,
        }
    }
}

impl<'a> Future for Delay<'a> {
    type Output = std::io::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}
