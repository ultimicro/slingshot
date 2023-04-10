# Slingshot
[![CI](https://github.com/ultimicro/slingshot/actions/workflows/ci.yml/badge.svg)](https://github.com/ultimicro/slingshot/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/slingshot)](https://crates.io/crates/slingshot)

Slingshot is a bare metal performance async runtime for Rust similar to Tokio or async-std. What makes Slingshot different from those runtimes is:

- It is working directly with the data types from `std` like `std::net::TcpStream` instead of introducing a new data type.
- It is a bare metal runtime with (almost) zero cost the same as C that directly using a platform-specific I/O notification system.

Currently it is supports only Linux but the runtime was designed as a cross platform so any PR to add supports for the other platforms is welcome!

## Examples

```rust
use slingshot::linux::Epoll;
use slingshot::{run, Runtime};
use std::net::{TcpListener, TcpStream};

fn main() {
    let rt = Epoll::new();

    let client = async {
        // Connect to the server.
        let mut client = TcpStream::connect("127.0.0.1:54336").expect("cannot connect to :54336");

        client
            .set_nonblocking(true)
            .expect("cannot enable non-blocking mode");

        // Write the data.
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

        client
            .set_nonblocking(true)
            .expect("cannot enable non-blocking mode");

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
```

## License

MIT
