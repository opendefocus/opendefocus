#!/bin/bash

apt update
apt install protobuf-compiler -y
curl -LsSf https://astral.sh/uv/install.sh | sh

cargo xtask
cd crates/spirv-cli-build && rustup toolchain install
