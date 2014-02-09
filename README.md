# Pyrite (super pre alpha)
Pyrite is an experimental render engine, written in Rust. It uses path
tracing and colors based on wavelengths.

## Getting started
Pyrite is currently only tested on Linux, but it may work on other systems too.
It will ususaly be buildable using the [rust-nightly](http://www.rust-ci.org/) Ubuntu package.

To download and build Pyrite to the `bin/` folder:


    git clone --recursive https://github.com/Ogeon/pyrite.git
    cd pyrite
    make deps
    make

To run Pyrite:


    cd bin/
    ./pyrite --render path/to/project.json

To run Pyrite in CLI mode:


    cd bin/
    ./pyrite path/to/project.json
    > render
    ...
    > exit

This will result in an image called `render.png` in `path/to/`. Example
projects can be found in `test/`.

## Dependencies
Pyrite requires the following libraries:

* [nalgebra](https://github.com/sebcrozet/nalgebra) for linear algebra.
* [Ogeon/rust-png](https://github.com/Ogeon/rust-png) for saving and loading PNG images. A fork of [mozilla-servo/rust-png](https://github.com/mozilla-servo/rust-png), but it will be compatible with the same version of Rust as Pyrite (usually master).
