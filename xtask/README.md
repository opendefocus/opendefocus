# OpenDefocus xtask

This package is a cli to handle repository tasks. It is not meant to be published. Just the central part for coordination.

Its just following the xtask convention: https://github.com/matklad/cargo-xtask.

That includes:
* Creating builds (in a single unified interface)
* Testing packages
* Building docs
* Simple cross platform scripts (no bash/sh/powershell stuff!! (pls) makes cross platform handling so much better)

It is fine to write untested code, etc, as its not meant to be production stable stuff. It is just for quick cli actions.