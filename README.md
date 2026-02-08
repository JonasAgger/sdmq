# sdmq

[![Crates.io](https://img.shields.io/crates/v/sdmq.svg)](https://crates.io/crates/sdmq)
[![Documentation](https://docs.rs/sdmq/badge.svg)](https://docs.rs/sdmq)
[![License](https://img.shields.io/crates/l/sdmq.svg)](https://github.com/JonasAgger/sdmq#license)

A wip simple dumb MQ for home use and ease of embedded.
Just make a small packet, throw it into UDP and forget about it.

## Installation

Add this to your Cargo.toml:

    [dependencies]
    sdmq = "0.1"

## Usage

    use hello_world_lib::greet;

    fn main() {
        let message = greet("World");
        println!("{}", message);
    }

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.