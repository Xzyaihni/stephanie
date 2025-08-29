#!/bin/sh
set -e
cargo rustc -r --target=x86_64-pc-windows-gnu --bin stephanie -- -L ~/fromsource/libs -l lzma-5

mkdir -p target/winbuild/stephanie

mv target/x86_64-pc-windows-gnu/release/stephanie.exe target/winbuild/stephanie/

cp ~/fromsource/libs/liblzma-5.dll target/winbuild/stephanie

deps=(lisp shaders fonts textures info items world_generation 'icon.png')
for f in ${deps[@]}; do
    cp -r $f target/winbuild/stephanie/
done

cd target/winbuild/
zip -r stephanie.zip stephanie
cd ../../
