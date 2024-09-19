#!/bin/sh
set -e
cargo b
RUST_BACKTRACE=1 gdb --args ./target/debug/stephanie --debug
