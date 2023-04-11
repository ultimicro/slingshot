use crate::cancel::{CancellationToken, SubscriptionHandle};
use std::task::{Context, Poll};

/// Create a [`Poll::Ready`] with a task canceled error.
pub(crate) fn io_cancel<T>() -> Poll<std::io::Result<T>> {
    Poll::Ready(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "the operation was canceled",
    )))
}

pub(crate) fn watch_cancel(
    cx: &mut Context,
    ct: &Option<CancellationToken>,
) -> Option<SubscriptionHandle> {
    if let Some(ct) = ct {
        let waker = cx.waker().clone();
        ct.subscribe(move || waker.wake())
    } else {
        None
    }
}
