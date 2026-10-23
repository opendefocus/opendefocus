# Developer documentation
Thank you for considering to contribute to OpenDefocus! In this documentation you will find (hopefully) all you need to get started.

---

## Structure
The project has multiple crates defined at the crates directory, each of them serve their own purpose:

| Crate Name                                                       | Description                                                                                                                   |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| [opendefocus](https://github.com/opendefocus/opendefocus/tree/main/crates/opendefocus)                              | The actual public library itself. The `main` crate.                                                                           |


### Supporting libraries

These independently versioned libraries are maintained in `crates/` and serve
applications beyond convolution:
| Crate Name                                                                 | Description                                                                                                                                             |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [circle-of-confusion](https://github.com/opendefocus/opendefocus/tree/main/crates/circle-of-confusion) | Camera and depth calculations |
| [bokeh-creator](https://github.com/opendefocus/opendefocus/tree/main/crates/bokeh-creator) | Aperture generation |
| [image-ndarray](https://github.com/opendefocus/opendefocus/tree/main/crates/image-ndarray) | ImageBuffer and ndarray conversions |


