#!/bin/sh
set -e
cargo b
rm -r ./worlds
RUST_BACKTRACE=1 ./target/debug/stephanie --debug --port 12345 --name yandere
