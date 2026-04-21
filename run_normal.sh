#!/bin/sh
set -e
cargo b --bin stephanie
rm -f -r ./worlds
rm settings.json
RUST_BACKTRACE=1 STEPHANIE_LISPDISABLECHECKS=1 ./target/debug/stephanie --port 12345
