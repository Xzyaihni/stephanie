#!/bin/sh
set -e
cargo b
gdb --args ./target/debug/stephanie --debug
