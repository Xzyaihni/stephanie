#!/bin/sh
set -e
cargo b
RUST_BACKTRACE=1 ./target/debug/stephanie --debug --port 12345
