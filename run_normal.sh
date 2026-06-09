#!/bin/sh
set -e
cargo b --profile=dev-debug --bin stephanie
rm -f -r ./worlds
rm -f settings.json
RUST_BACKTRACE=1 STEPHANIE_LISPDISABLECHECKS=1 ./target/dev-debug/stephanie --port 12345
