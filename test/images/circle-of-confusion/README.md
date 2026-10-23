# Circle of confusion numeric fixtures

`cases.json` is unchanged from olivebowl/circle-of-confusion v0.2.7,
commit f127ae05463d9b8f5bc58d287c5b35d645ee6600 (EUPL-1.2).
There are no upstream image assets or generation scripts. These are manually
specified numeric CoC regression inputs/expected values, not renderer outputs.
SHA256: `a07b26d49750f7770241a1f2cf119c05098ead0976b3b9376db801bf38ac7aaa`.

This is the canonical copy. Run `cargo xtask fixtures sync --stage`
after editing it, then commit the staged `crates/circle-of-confusion/test/cases.json`
copy too. CI checks their byte equality. The package copy is deliberate: Cargo
and Python source distributions include it, so published tests need no monorepo
or external asset paths. Rust embeds it using `include_str!`; Python reads it
relative to its packaged tests. Crate license: `crates/circle-of-confusion/LICENSE`.
