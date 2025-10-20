#!/bin/sh
set -e
cargo b --bin stephanie
rm -f -r ./worlds
RUST_BACKTRACE=1 ./target/debug/stephanie --port 12345
