# Portus [![Build Status](https://travis-ci.org/ccp-project/portus.svg?branch=master)](https://travis-ci.org/ccp-project/portus)

Portus is an implementation of a congestion control plane (CCP).
It is a library that can be used to write new congestion control
algorithms in user-space. 

Congestion control algorithm implementations live in independent crates
which use this library for common functionality. Each algorithm crate
provides a binary which runs a CCP with that algorithm activated.

Further documentation is available on [docs.rs](https://docs.rs/portus).

## Setup

1. Install rust, and use the nightly toolchain. See http://rust-lang.org/ for details.
2. `make`. This will build and lint the portus library and bundled algorithm libraries and binaries, and run unit tests.

### Notes

- The `ipc::netlink` and `ipc::kp` modules will only compile on Linux. If the CCP kernel module (github.mit.edu/nebula/ccp-kernel) is loaded, the test will refuse to run.

### Run

Once you've built `portus`, algorithms can be run as follows:

```
sudo ./ccp_generic_cong_avoid/target/debug/reno
```

All algorithms support the `--ipc` flag, allowing you to specify the method of
inter-process communication between the CCP and your datapath of choice. For
example, our current implementation of the linux kernel datapath uses netlink
sockets, while our implementation of the mtcp/dpdk datapath uses unix sockets.

Individual algorithms may also allow you to set algorithm-specific parameters
from the command line. Run with `--help` to view them.
