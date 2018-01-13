# Portus (CCP-Rust)

Portus is an implementation of the congestion control plane (CCP) in Rust.
It is designed as a library that can be used to write new congestion control
algorithms in user-space. Congestion control algorithm code lives totally separate
from the library code and each algorithm is compiled into its own executable,
as opposed to be being baked into the library. 



### Setup

1. Install rust

```
curl https://sh.rustup.rs -sSf | sh

```


2. Install nightly toolchain

```
rustup toolchain install nightly
```


3. Build Portus library

In the root directory of the repository: 

```
make
```

Note: test_kp, which tests the kernel character device ipc (1) assumes the
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

This should create the executable `./ccp_reno/target/debug/reno`

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

TODO

