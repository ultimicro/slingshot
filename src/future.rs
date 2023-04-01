use std::task::Poll;

/// Create a [`Poll::Ready`] with a task canceled error.
pub(crate) fn io_cancel<T>() -> Poll<std::io::Result<T>> {
    Poll::Ready(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "the operation was canceled",
    )))
}
