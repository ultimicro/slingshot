use slingshot::cancel::CancellationToken;
use slingshot::linux::Epoll;
use slingshot::{run, Runtime};
use std::net::TcpListener;
use std::time::Duration;

#[test]
fn test_epoll_cancel() {
    let rt = Epoll::new();
    let ct = CancellationToken::new();

    run(rt, async move {
        // Listen for connection.
        let mut listener =
            TcpListener::bind("127.0.0.1:54337").expect("cannot listening on :54337");

        listener
            .set_nonblocking(true)
            .expect("cannot toggle non-blocking mode");

        // Spawn a task to cancel.
        let ctc = ct.clone();

        rt.spawn(async move {
            if let Err(e) = rt.delay(Duration::from_secs(5), None).await {
                panic!("cannot delay for 5 secs: {e}");
            }

            ctc.cancel();
        });

        // Accept.
        if let Err(e) = rt.accept_tcp(&mut listener, Some(ct)).await {
            if e.to_string() != "the operation was canceled" {
                panic!("cannot accept a connection: {e}");
            }
        }
    });
}
