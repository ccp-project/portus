# Portus [![Build Status](https://travis-ci.org/ccp-project/portus.svg?branch=master)](https://travis-ci.org/ccp-project/portus)

Portus is an implementation of a congestion control plane (CCP).
It is a library that can be used to write new congestion control
algorithms in user-space. 

Congestion control algorithm implementations live in independent crates
which use this library for common functionality. Each algorithm crate
provides a binary which runs a CCP with that algorithm activated.

Further documentation is available on [docs.rs](https://docs.rs/portus).

## Setup

1. Install rust. See http://rust-lang.org/ for details.
2. `make`. This will build and lint the portus library and bundled algorithm libraries and binaries, and run unit tests.

### Notes

- The `ipc::netlink` and `ipc::kp` modules will only compile on Linux. If the CCP kernel module (github.mit.edu/nebula/ccp-kernel) is loaded, the test will refuse to run.

### Run

There are no algorithm binaries in this repository: it is just a library and runtime for CCP algorithms. You may be interested in https://github.com/ccp-project/generic-cong-avoid, which provides implementations of Reno and Cubic, or https://github.com/ccp-project/bbr, a BBR implementation.
