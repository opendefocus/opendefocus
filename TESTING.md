# Testing and verification

What a change must prove before it merges, how to measure performance, and what
is not yet covered. Commands are in [AGENTS.md](AGENTS.md#commands).

## What each check proves

| Check | Proves | Does not prove |
| --- | --- | --- |
| `cargo xtask --test-crates`, unit tests | Host-side logic, settings, layouts | GPU behaviour |
| `cargo xtask --compile --gpu` (Naga) | WGSL syntax and binding layout | Driver behaviour or image quality |
| Hardware tests (`#[ignore]`, run with `-- --ignored --test-threads=1`) | GPU output on this adapter | Other adapters |
| `cargo xtask parity` | Plugin matches CLI on this machine (`test/parity/commands.tsv`, output in `target/parity/exr`) | Correctness of the CLI itself |
| `cargo xtask --test-natron` | Plugin renders in Natron and matches the CLI at 1e-6 | Other hosts |

Image tests need `--release`. Missing hardware is a missing gate, never
evidence of no impact. Never claim older tests cover a changed shader or
runtime path.

## Correctness

- Compare base and final float outputs and contact sheets, and play motion
  where it applies. Pass counts, hashes and timings do not replace looking.
- Use independent checks: PSF shape, energy, alpha, clipping, focus and motion.
- Historical renders and the committed gallery are comparisons, not truth.
- Add a focused regression next to the affected code and reuse fixtures in
  `test/images/`.

## Performance

Required for rendering and allocation changes.

1. Measure the unchanged base and the final change on identical hardware,
   features, inputs, settings, dimensions, regions and stripe height. Check the
   recorded adapter and restore pristine inputs.
2. Measure cold initialization and first frame separately, then three warm-ups
   and ten measured frames: `cargo xtask bench before -- <input> [options]` and
   `cargo xtask bench after` (results in gitignored `benchmark-results/`), or
   CLI `--repeat 13 --report <path>` with the path outside the checkout.
3. Report medians, maxima, percentage change, output correctness and, where
   relevant, memory. Investigate any repeatable median regression of 5% or more.

Serialize GPU work with `/tmp/opendefocus-validation.lock` on shared hosts.

## OFX plugin

- Build with `cargo xtask --bundle-ofx`; load without installing via
  `OFX_PLUGIN_PATH=<dir containing the bundle>`.
- Natron scripts live in `test/natron/`. Node id `org.opendefocus.OpenDefocus`,
  input 0 Source, input 1 Depth; Write `bitDepth` index 7 is 32f.
- Baseline EXRs are GPU-specific. On other hardware, compare the plugin with the
  CLI on the same machine.
- Do not install or launch a host only to close a coverage gap; record the gap.

## Coverage and known gaps

This section lists coverage only. Bugs, open questions and other actionable
gaps are [GitHub issues](https://github.com/opendefocus/opendefocus/issues);
file new ones there, not here.

**Verified only on** Apple M1 / Metal with Natron 2.6 and DaVinci Resolve Free.

**Unverified:** Linux and Windows hosts, installers and signing, the
`test-natron` and release workflows (never run), Resolve Studio, Nuke
Studio/Indie (Nuke Non-Commercial does not load OFX), the Nuke camera input
(`NukeOfxCameraSuite`), animated parameters, save/load, abort, 1x1 and odd
sizes, non-zero RoD origins, pixel aspect, 8K, multi-plane layers.

**Releases** are Linux and Windows (OFX installers for Linux x86_64 and aarch64
and Windows x86_64, Python wheels). The Windows installer is unsigned. macOS
follows once it is tested in CI.
