#!/bin/bash
apt update
apt install unzip msitools llvm -y

cargo install xwin --locked
cargo install --locked cargo-zigbuild

rustup target add x86_64-pc-windows-msvc
