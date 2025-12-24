#!/bin/bash

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"

cd ~
curl -O https://ziglang.org/download/0.13.0/zig-linux-$(uname -m)-0.13.0.tar.xz

tar xf zig-linux-$(uname -m)-0.13.0.tar.xz
echo "export PATH=\"\$HOME/zig-linux-$(uname -m)-0.13.0:\$PATH\"" >> ~/.bashrc

dnf update
dnf install protobuf-compiler openssl-devel -y
curl -LsSf https://astral.sh/uv/install.sh | sh

cargo install --locked cargo-zigbuild
