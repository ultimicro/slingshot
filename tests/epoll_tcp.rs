use slingshot::linux::Epoll;
use slingshot::{run, Runtime};
use std::net::{TcpListener, TcpStream};

#[test]
fn test_epoll_tcp() {
    let rt = Epoll::new();

    let client = async {
        let mut client = TcpStream::connect("127.0.0.1:54336").expect("cannot connect to :54336");
        let data = [7u8; 8];

        match rt.write_tcp(&mut client, &data, None).await {
            Ok(v) => assert_eq!(v, 8),
            Err(e) => panic!("cannot write the data: {e}"),
        };
    };

    run(rt, async move {
        // Listen for connection.
        let mut listener =
            TcpListener::bind("127.0.0.1:54336").expect("cannot listening on :54336");

        listener
            .set_nonblocking(true)
            .expect("cannot toggle non-blocking mode");

        // Spawn a connector.
        rt.spawn(client);

        // Accept.
        let (mut client, addr) = match rt.accept_tcp(&mut listener, None).await {
            Ok(v) => v,
            Err(e) => panic!("cannot accept a connection: {e}"),
        };

        assert!(addr.ip().is_loopback());

        // Read.
        let mut buf = [0u8; 8];

        match rt.read_tcp(&mut client, &mut buf, None).await {
            Ok(v) => {
                assert_eq!(v, 8);
                assert_eq!(buf, [7u8; 8]);
            }
            Err(e) => panic!("cannot read the data: {e}"),
        }
    });
}
