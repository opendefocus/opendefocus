#!/bin/bash

apt update
apt install 7zip -y

rustup target add aarch64-apple-darwin
cargo install --locked cargo-zigbuild
curl -L "https://github.com/phracker/MacOSX-SDKs/releases/download/11.3/MacOSX11.3.sdk.tar.xz" | tar -J -x -C /opt

cd ~
curl -O https://ziglang.org/download/0.13.0/zig-linux-$(uname -m)-0.13.0.tar.xz

tar xf zig-linux-$(uname -m)-0.13.0.tar.xz

echo "export PATH=\"\$HOME/zig-linux-$(uname -m)-0.13.0:\$PATH\"" >> ~/.bashrc
echo "export SDKROOT=/opt/MacOSX11.3.sdk" >> ~/.bashrc