# <i class="fa-solid fa-wrench"></i> Installation

This page will give an explanation on how to install OpenDefocus.

Before you start, please make sure all your drivers are up-to-date.


## Nuke
Installation is the same on each operating system, as each download will contain all supported Nuke versions + all operating systems. This is all included in one package.

It is similar to any gizmo install that you've probably done previously.

### Extracting the download
Extract the downloaded zip file to a location you'd like to install the plugin. This can be anything, but it is common to use the `.nuke` folder.

This folder can be found here:

* <i class="fa-brands fa-windows"></i> `C:\Users\Your Username\.nuke`
* <i class="fa-brands fa-linux"></i> `~/.nuke`
* <i class="fa-brands fa-apple"></i> `/Users/YourUsername/.nuke`

For more information from the Foundry visit their documentation: [https://support.foundry.com/hc/en-us/articles/207271649-Q100048-Locating-the-default-nuke-directory](https://support.foundry.com/hc/en-us/articles/207271649-Q100048-Locating-the-default-nuke-directory)


### `init.py` installation
If there is no `init.py` file in your `.nuke` folder, simply copy the one provided in the downloaded zip file and you're done!

Else, copy the following contents into your `init.py` file:
```python
nuke.pluginAddPath("./opendefocus_plugin")
```

If you've chosen to install OpenDefocus into a different location than the `.nuke` folder, adjust the path to the absolute path of the installation directory:

```python
nuke.pluginAddPath("the/path/to/the/installation/here")
```

### Environment variable installation (alternative)
This is an alternative approach, without modifying the `init.py`, but by setting the `NUKE_PATH` environment variable.

1. Add a `NUKE_PATH` environment variable to your system.
2. Set the value to the absolute path to the inside of the `opendefocus_plugin` folder. E.g. `my/path/to/my_installation/opendefocus_plugin/`

