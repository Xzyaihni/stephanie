#!/bin/sh
set -e
g++ ./src/float_helper.cpp -shared -fPIC -o libfloathelper.so
cargo rustc -- -L ./ -l floathelper
LD_LIBRARY_PATH="./" gdb --args ./target/debug/stephanie --debug
