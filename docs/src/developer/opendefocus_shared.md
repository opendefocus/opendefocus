# opendefocus-shared

The shared crate contains code which both the kernel and core crate require, without making redudant code. For example definitions of the CPU image. It is fine to write std code, but make sure it is also no-std compatible for the spirv source code.

## Structure
- [lib.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-shared/src/lib.rs): contains the exports to inner functionality, constants and macros to some branchless operations.
- [internal_settings.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-shared/src/internal_settings.rs): conversion logic for exposing the settings into a simplified struct which can be send over GPU buffers.
- [math.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-shared/src/math.rs): some math that both core and kernel require, but also simple switches to the correct math library (`std` and `libm`)
- [cpu_image.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-shared/src/cpu_image.rs): wrapper for the images send by the CPU to match functionality according to the spirv library. To make it easy to work with different image types.
