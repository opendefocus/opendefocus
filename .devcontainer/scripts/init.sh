#!/bin/bash

apt update
curl -LsSf https://astral.sh/uv/install.sh | sh

cargo xtask
