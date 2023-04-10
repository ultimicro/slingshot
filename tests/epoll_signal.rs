use libc::{
    raise, sigaddset, sigemptyset, sigfillset, signalfd, sigprocmask, SFD_CLOEXEC, SFD_NONBLOCK,
    SIGUSR1, SIG_BLOCK,
};
use slingshot::fd::Fd;
use slingshot::linux::{Epoll, LinuxRuntime};
use slingshot::run;
use std::io::Error;
use std::mem::MaybeUninit;
use std::ptr::null_mut;

#[test]
fn test_epoll_signal() {
    // Block default signal handler.
    let mut blocks = MaybeUninit::uninit();

    if unsafe { sigfillset(blocks.as_mut_ptr()) } < 0 {
        panic!("sigfillset() failed: {}", Error::last_os_error());
    }

    if unsafe { sigprocmask(SIG_BLOCK, blocks.as_ptr(), null_mut()) } < 0 {
        panic!("sigprocmask() failed: {}", Error::last_os_error());
    }

    // Execute the runtime.
    let rt = Epoll::new();

    run(rt, async move {
        // Create a signalfd.
        let mut fd = {
            let mut set = MaybeUninit::uninit();

            if unsafe { sigemptyset(set.as_mut_ptr()) } < 0 {
                panic!("sigemptyset() failed: {}", Error::last_os_error());
            } else if unsafe { sigaddset(set.as_mut_ptr(), SIGUSR1) } < 0 {
                panic!("sigaddset() failed: {}", Error::last_os_error());
            }

            let fd = unsafe { signalfd(-1, set.as_ptr(), SFD_NONBLOCK | SFD_CLOEXEC) };

            if fd < 0 {
                panic!("signalfd() failed: {}", Error::last_os_error());
            }

            Fd::new(fd)
        };

        fd.set_nonblocking().expect("set_nonblocking() failed");

        // Raise the signal. We cannot do this on another task because it will send the signal to
        // different thread.
        if unsafe { raise(SIGUSR1) } != 0 {
            panic!("raise(SIGUSR1) failed");
        }

        // Read the signal.
        let i = match unsafe { rt.read_signal(&mut fd, None).await } {
            Ok(v) => v,
            Err(e) => panic!("cannot wait for a signal: {e}"),
        };

        assert_eq!(i.ssi_signo, SIGUSR1 as _);
    });
}
