use std::alloc::{alloc, dealloc, Layout};
use std::ptr::null_mut;

/// Encapsulate a raw memory buffer.
pub(super) struct Buffer {
    ptr: *mut u8,
    len: usize,
}

impl Buffer {
    pub fn new(len: usize) -> Self {
        debug_assert_ne!(len, 0);

        Self {
            ptr: unsafe { alloc(Layout::array::<u8>(len).unwrap()) },
            len,
        }
    }

    pub fn get(&self) -> *mut u8 {
        self.ptr
    }

    /// # Safety
    /// `len` must be less than or equal to the size of the buffer.
    pub unsafe fn into_vec(mut self, len: usize) -> Vec<u8> {
        debug_assert!(len <= self.len);

        let vec = Vec::from_raw_parts(self.ptr, len, self.len);
        self.ptr = null_mut();

        vec
    }
}

unsafe impl Send for Buffer {}

impl Drop for Buffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { dealloc(self.ptr, Layout::array::<u8>(self.len).unwrap()) };
        }
    }
}

impl TryFrom<&[u8]> for Buffer {
    type Error = ();

    fn try_from(v: &[u8]) -> Result<Self, Self::Error> {
        if v.is_empty() {
            return Err(());
        }

        let b = Buffer::new(v.len());
        unsafe { b.get().copy_from_nonoverlapping(v.as_ptr(), v.len()) };

        Ok(b)
    }
}
