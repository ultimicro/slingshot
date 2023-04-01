use libc::{close, fcntl, pipe, F_GETFL, F_SETFL, O_NONBLOCK, STDERR_FILENO};
use std::ffi::c_int;
use std::fmt::{Display, Formatter};
use std::io::Error;

/// Encapsulate a file descriptor.
pub(crate) struct Fd(c_int);

impl Fd {
    /// A shorthand for libc [`pipe()`].
    pub fn pipe() -> std::io::Result<(Self, Self)> {
        let mut ends = [0 as c_int; 2];

        if unsafe { pipe(ends.as_mut_ptr()) } < 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok((Self(ends[0]), Self(ends[1])))
    }

    pub fn new(v: c_int) -> Self {
        if v <= STDERR_FILENO {
            panic!("the value cannot be stdin, stdout or stderr");
        }

        Self(v)
    }

    pub fn get(&self) -> c_int {
        self.0
    }

    pub fn set_nonblocking(&self) -> std::io::Result<()> {
        let flags = unsafe { fcntl(self.0, F_GETFL) };

        if flags < 0 {
            return Err(Error::last_os_error());
        }

        if unsafe { fcntl(self.0, F_SETFL, flags | O_NONBLOCK) } < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        if unsafe { close(self.0) } < 0 {
            let e = std::io::Error::last_os_error();
            panic!("cannot close file descriptor #{}: {e}", self.0);
        }
    }
}

impl Display for Fd {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}
