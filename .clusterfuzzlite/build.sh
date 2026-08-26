#!/bin/bash
set -euo pipefail

cd "$SRC/pdf-core"
cargo fuzz build -O --debug-assertions

target_dir="fuzz/target/x86_64-unknown-linux-gnu/release"
for target in fuzz/fuzz_targets/*.rs; do
    name="$(basename "${target%.rs}")"
    cp "$target_dir/$name" "$OUT/$name"
done
