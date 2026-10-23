# bokeh-creator reference images

These 11 JPEG files are byte-identical to the references in [olivebowl/bokeh-creator](https://github.com/olivebowl/bokeh-creator/tree/180778c2f4ad6161e3a76ac7c48a4d2b1e022816/test/images), release 0.1.17, commit `180778c2f4ad6161e3a76ac7c48a4d2b1e022816`. They retain upstream's EUPL-1.2 license, reproduced at `crates/bokeh-creator/LICENSE`. No images were recompressed or regenerated during import.

The source tests encode the generator settings: disc/blade, blade count/angle/curvature, aspect ratio, ring/blur, seeded noise and rectangular resolutions. They are perceptual regression references (MSSIM >0.9 on luma), not HDR or numerical ground truth. SHA256SUMS records the exact upstream bytes.

Run `cargo xtask fixtures sync` at the repository root to verify canonical files and tracked package copies. After an intentional reference update, review the new images and hashes, then run `cargo xtask fixtures sync --stage` and commit both sets. Test execution never modifies fixtures. Packaged copies allow standalone published-crate tests without access to this repository.
