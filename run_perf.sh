#!/bin/sh
set -e
cargo rustc --bin stephanie --release -- --cfg stimings
RUST_BACKTRACE=1 ./target/release/stephanie
