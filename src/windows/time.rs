use super::Iocp;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use crate::future::{io_cancel, watch_cancel};
use std::ffi::c_void;
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Weak};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use windows_sys::Win32::Foundation::{BOOLEAN, ERROR_IO_PENDING};
use windows_sys::Win32::System::Threading::{
    CreateTimerQueueTimer, DeleteTimerQueueTimer, WT_EXECUTEONLYONCE,
};

/// A future of [`Iocp::delay()`].
pub struct Delay<'a> {
    phantom: PhantomData<&'a Iocp>,
    dur: Duration,
    ct: Option<CancellationToken>,
    ch: Option<SubscriptionHandle>,
    timer: Option<Arc<Timer>>,
}

impl<'a> Delay<'a> {
    pub(crate) fn new(dur: Duration, ct: Option<CancellationToken>) -> Self {
        Self {
            phantom: PhantomData,
            dur,
            ct,
            ch: None,
            timer: None,
        }
    }

    unsafe extern "system" fn waker(param0: *mut c_void, _: BOOLEAN) {
        if let Some(t) = Weak::from_raw(param0 as *const Timer).upgrade() {
            t.elapsed.store(true, Ordering::Relaxed);
            t.waker.wake_by_ref();
        }
    }
}

impl<'a> Future for Delay<'a> {
    type Output = std::io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.ch = None;

        if let Some(ct) = &f.ct {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Check timer state.
        match &f.timer {
            Some(timer) => {
                if timer.elapsed.load(Ordering::Relaxed) {
                    return Poll::Ready(Ok(()));
                }
            }
            None => {
                // Get millisecond.
                let due: u32 = match f.dur.as_millis().try_into() {
                    Ok(v) => v,
                    Err(_) => {
                        return Poll::Ready(Err(Error::new(
                            ErrorKind::Other,
                            "the specified duration is too large",
                        )))
                    }
                };

                if due == 0 {
                    return Poll::Ready(Ok(()));
                }

                // Create a new timer.
                let timer = Arc::new(Timer {
                    handle: AtomicIsize::new(0),
                    waker: cx.waker().clone(),
                    elapsed: AtomicBool::new(false),
                });

                let mut handle = 0;
                let parameter = Weak::into_raw(Arc::downgrade(&timer));

                if unsafe {
                    CreateTimerQueueTimer(
                        &mut handle,
                        0,
                        Some(Self::waker),
                        parameter as _,
                        due,
                        0,
                        WT_EXECUTEONLYONCE,
                    )
                } == 0
                {
                    drop(unsafe { Weak::from_raw(parameter) });
                    return Poll::Ready(Err(Error::last_os_error()));
                } else {
                    timer.handle.store(handle, Ordering::Relaxed);
                }

                f.timer = Some(timer);
            }
        }

        // Watch for cancel.
        f.ch = watch_cancel(cx, &f.ct);

        Poll::Pending
    }
}

/// Encapsulate a Win32 timer.
struct Timer {
    handle: AtomicIsize,
    waker: Waker,
    elapsed: AtomicBool,
}

impl Drop for Timer {
    fn drop(&mut self) {
        let handle = self.handle.load(Ordering::Relaxed);

        // Not sure what if the invalid handle timer is NULL or INVALID_HANDLE_VALUE.
        if handle != 0 && unsafe { DeleteTimerQueueTimer(0, handle, 0) } == 0 {
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != ERROR_IO_PENDING as _ {
                panic!("cannot delete a timer: {e}");
            }
        }
    }
}
