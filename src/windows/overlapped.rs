use super::mem::Buffer;
use std::sync::Weak;
use std::task::Waker;
use windows_sys::Win32::System::IO::{OVERLAPPED, OVERLAPPED_0, OVERLAPPED_0_0};

/// Encapsulate an OVERLAPPED object.
#[repr(C)]
pub(super) struct Overlapped {
    raw: OVERLAPPED, // MUST be the first field.
    buf: Buffer,
    data: Weak<dyn OverlappedData>,
}

impl Overlapped {
    pub fn new<D>(offset: Option<u64>, buf: Buffer, data: Weak<D>) -> Box<Self>
    where
        D: OverlappedData + 'static,
    {
        let offset = match offset {
            Some(v) => OVERLAPPED_0_0 {
                Offset: (v & 0xffffffff) as u32,
                OffsetHigh: (v >> 32) as u32,
            },
            None => OVERLAPPED_0_0 {
                Offset: 0,
                OffsetHigh: 0,
            },
        };

        Box::new(Self {
            raw: OVERLAPPED {
                Internal: 0,
                InternalHigh: 0,
                Anonymous: OVERLAPPED_0 { Anonymous: offset },
                hEvent: 0,
            },
            buf,
            data,
        })
    }

    pub fn buf(&self) -> *mut u8 {
        self.buf.get()
    }

    pub fn complete(self, transferred: usize, error: Option<std::io::Error>) {
        // Get data.
        let data = match self.data.upgrade() {
            Some(v) => v,
            None => return,
        };

        // Set the result and wake the task.
        let waker = data.set_completed(match error {
            Some(v) => OverlappedResult::Error(v),
            None => OverlappedResult::Success(self.buf, transferred),
        });

        waker.wake();
    }
}

/// Additional data to attached with [`Overlapped`].
pub(super) trait OverlappedData: Send + Sync {
    fn set_completed(&self, r: OverlappedResult) -> Waker;
}

/// Result of an overlapped operation.
pub(super) enum OverlappedResult {
    Error(std::io::Error),
    Success(Buffer, usize),
}
