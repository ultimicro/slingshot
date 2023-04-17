use super::Iocp;
use crate::cancel::{CancellationToken, SubscriptionHandle};
use crate::future::{io_cancel, watch_cancel};
use std::ffi::c_void;
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use windows_sys::Win32::Foundation::{BOOLEAN, ERROR_IO_PENDING, HANDLE};
use windows_sys::Win32::System::Threading::{
    CreateTimerQueueTimer, DeleteTimerQueueTimer, WT_EXECUTEONLYONCE,
};

/// A future of [`Iocp::delay()`].
pub struct Delay<'a> {
    phantom: PhantomData<&'a Iocp>,
    duration: Duration,
    cancellation_token: Option<CancellationToken>,
    cancellation_handle: Option<SubscriptionHandle>,
    timer: Option<Arc<Mutex<Timer>>>,
}

impl<'a> Delay<'a> {
    pub(crate) fn new(duration: Duration, cancellation_token: Option<CancellationToken>) -> Self {
        Self {
            phantom: PhantomData,
            duration,
            cancellation_token,
            cancellation_handle: None,
            timer: None,
        }
    }

    fn begin_timer(&mut self, cx: &mut Context, due: u32) -> Result<Arc<Mutex<Timer>>, Error> {
        // Construct timer data.
        let timer = Arc::new(Mutex::new(Timer {
            handle: None,
            waker: cx.waker().clone(),
            elapsed: false,
        }));

        // Start timer.
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
            let e = Error::last_os_error();
            drop(unsafe { Weak::from_raw(parameter) });
            return Err(e);
        }

        // Store the handle.
        timer.lock().unwrap().handle = Some(handle);

        Ok(timer)
    }

    unsafe extern "system" fn waker(param0: *mut c_void, _: BOOLEAN) {
        // Get timer.
        let timer = match Weak::from_raw(param0 as *const Mutex<Timer>).upgrade() {
            Some(v) => v,
            None => return,
        };

        // Toggle elapsed and wake the task.
        let mut t = timer.lock().unwrap();

        t.elapsed = true;
        t.waker.wake_by_ref();
    }
}

impl<'a> Future for Delay<'a> {
    type Output = std::io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.deref_mut();

        // Check if canceled.
        f.cancellation_handle = None;

        if let Some(ct) = &f.cancellation_token {
            if ct.is_canceled() {
                return io_cancel();
            }
        }

        // Check timer state.
        match &f.timer {
            Some(t) => {
                // Check if elapsed.
                let mut t = t.lock().unwrap();

                if t.elapsed {
                    return Poll::Ready(Ok(()));
                } else {
                    t.waker = cx.waker().clone();
                }
            }
            None => {
                // Get millisecond.
                let due: u32 = match f.duration.as_millis().try_into() {
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
                f.timer = match f.begin_timer(cx, due) {
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

/// Encapsulate a Win32 timer.
struct Timer {
    handle: Option<HANDLE>,
    waker: Waker,
    elapsed: bool,
}

impl Drop for Timer {
    fn drop(&mut self) {
        // Check if we need to delete the timer.
        let handle = match self.handle {
            Some(v) => v,
            None => return,
        };

        // Delete the timer.
        if unsafe { DeleteTimerQueueTimer(0, handle, 0) } == 0 {
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != ERROR_IO_PENDING as _ {
                panic!("cannot delete a timer: {e}");
            }
        }
    }
}
