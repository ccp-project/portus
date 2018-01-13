# Portus (CCP-Rust)

Portus is an implementation of a congestion control plane (CCP).
It is a library that can be used to write new congestion control
algorithms in user-space. 

Congestion control algorithm implementations live in independent crates
which use this library for common functionality. Each algorithm crate
provides a binary which runs a CCP with that algorithm activated.

### Setup

1. Install rust
2. Use nightly rust

```
curl https://sh.rustup.rs -sSf | sh
rustup toolchain install nightly
```

3. Build Portus library

In the root directory of the repository: 

```
make
```

#Notes

1. netlink will only compile on linux. If the CCP kernel module (github.mit.edu/nebula/ccp-kernel) is loaded, the test will hang.

2. test_kp, which tests the kernel character device ipc (1) assumes the
kernel module has already been loaded and (2) must be run as root,
otherwise it will fail (this is fine if you're not using the char device).

Once the module is loaded, to run the tests as root:
```
sudo cargo +nightly test --features bench --all
```
If sudo cannot find cargo (`sudo: cargo: command not found`) its likely a PATH
issue. A quick fix is to find the absolute path of the cargo executable (`which
cargo`) and then supply it manually (`sudo /path/to/cargo +nightly ...`)


4. Build an individual algorithm, e.g. Reno

```
cd ccp_reno
cargo +nightly build
```

This create the executable `./ccp_reno/target/debug/reno`

### Run

Once you've built an algorithm (you need to build each one separately),
it can be run as follows:

```
sudo ./ccp_reno/target/debug/reno
```

All algorithms support the `--ipc` flag, allowing you to specify the method of
inter-process communication between the CCP and your datapath of choice. For
example, our current implementation of the linux kernel datapath uses netlink
sockets, while our implementation of the mtcp/dpdk datapath uses unix sockets.

Individual algorithms may also allow you to set algorithm-specific parameters
from the command line. Run with `--help` to view them.

### Writing a new congestion control algorithm

Congestion control algorithms should implement the CongAlg trait, specifying:
- a configuration struct with parameters to pass to new instantiations
- a create() constructor, called upon a new flow
- a measure() handle, called upon the receipt of a measurement
