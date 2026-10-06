#!/usr/bin/env bash
set -euo pipefail
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
export RUNNER_TEMP="${RUNNER_TEMP:-$(mktemp -d)}"
export GITHUB_PATH="${GITHUB_PATH:-$RUNNER_TEMP/github_path}"
touch "$GITHUB_PATH"

python3 .github/check-first-party.py
rustc --version
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo check --locked --target wasm32-unknown-unknown

version=$(python3 -c 'import tomllib; print(next(p["version"] for p in tomllib.load(open("Cargo.lock", "rb"))["package"] if p["name"] == "wasm-bindgen"))')
archive="wasm-bindgen-${version}-x86_64-unknown-linux-musl"
curl --user-agent cvld-security-review --fail --location --retry 3 "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${version}/${archive}.tar.gz" -o "$RUNNER_TEMP/wasm-bindgen.tar.gz"
curl --user-agent cvld-security-review --fail --location --retry 3 "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${version}/${archive}.tar.gz.sha256sum" -o "$RUNNER_TEMP/wasm-bindgen.tar.gz.sha256sum"
checksum=$(cut -d ' ' -f1 "$RUNNER_TEMP/wasm-bindgen.tar.gz.sha256sum")
printf '%s  %s\n' "$checksum" "$RUNNER_TEMP/wasm-bindgen.tar.gz" | sha256sum --check
tar -xzf "$RUNNER_TEMP/wasm-bindgen.tar.gz" -C "$RUNNER_TEMP"
echo "$RUNNER_TEMP/$archive" >> "$GITHUB_PATH"
export PATH="$RUNNER_TEMP/$archive:$PATH"
cargo test --locked --target wasm32-unknown-unknown --tests

rustup run nightly rustc --version
NIGHTLY_RUSTC="$(rustup which --toolchain nightly rustc)"
NIGHTLY_CARGO="$(rustup which --toolchain nightly cargo)"
NIGHTLY_BIN="$(dirname "$NIGHTLY_RUSTC")"
export RUSTC="$NIGHTLY_RUSTC"
export CARGO="$NIGHTLY_CARGO"
export PATH="$NIGHTLY_BIN:$PATH"
export RUSTUP_TOOLCHAIN=nightly
"$NIGHTLY_CARGO" llvm-cov --version
"$NIGHTLY_CARGO" llvm-cov --locked --workspace --all-features --branch --no-cfg-coverage --no-cfg-coverage-nightly --ignore-filename-regex '/tests/' --json --output-path coverage.json
"$NIGHTLY_CARGO" llvm-cov report --locked --branch --ignore-filename-regex '/tests/' --lcov --output-path coverage.lcov
"$NIGHTLY_CARGO" llvm-cov report --locked --branch --ignore-filename-regex '/tests/' --text --output-path coverage.txt
python3 .github/test-source-coverage.py
python3 .github/check-source-coverage.py coverage.lcov coverage.json coverage.txt
