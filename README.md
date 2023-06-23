# Slingshot
[![CI](https://github.com/ultimicro/slingshot/actions/workflows/ci.yml/badge.svg)](https://github.com/ultimicro/slingshot/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/slingshot)](https://crates.io/crates/slingshot)

Slingshot is an async runtime for Rust similar to Tokio or async-std. What makes Slingshot different from those runtimes is:

- It is working directly with the data types from `std` like `std::net::TcpStream` instead of introducing a new data type.
- It is guarantee that all futures will run to completion if the process does not forced to exit.

This crate provide only the abstraction layer for the other crates to use. You will need one of the implementor if you are building an application.

## License

MIT
