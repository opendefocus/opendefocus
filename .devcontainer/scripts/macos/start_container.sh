#!/bin/bash

container run --name opendefocus \
  -d \
  --workdir /workspace/opendefocus\
  --volume "$PWD":/workspace/opendefocus\
  docker.io/rust:1.92-trixie \
  sleep infinity