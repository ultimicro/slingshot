use std::io::Error;
use std::mem::forget;
use windows_sys::Win32::Networking::WinSock::{closesocket, SOCKET, SOCKET_ERROR};

/// Encapsulate a raw socket.
pub(super) struct Socket(SOCKET);

impl Socket {
    pub fn new(raw: SOCKET) -> Self {
        Self(raw)
    }

    pub fn get(&self) -> SOCKET {
        self.0
    }

    pub fn into_raw(self) -> SOCKET {
        let r = self.0;
        forget(self);
        r
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        if unsafe { closesocket(self.0) } == SOCKET_ERROR {
            panic!("cannot close a socket: {}", Error::last_os_error());
        }
    }
}
