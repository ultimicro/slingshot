use std::io::Error;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};

/// Encapsulate a Win32 handle.
pub(super) struct Handle(HANDLE);

impl Handle {
    pub fn new(h: HANDLE) -> Self {
        Self(h)
    }

    pub fn get(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if unsafe { CloseHandle(self.0) } == 0 {
            panic!(
                "cannot close handle #{}: {}",
                self.0,
                Error::last_os_error()
            );
        }
    }
}
