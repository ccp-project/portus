Portus is an implementation of a congestion control plane (CCP).
It is a library that can be used to write new congestion control
algorithms in user-space. 

Congestion control algorithm implementations live in independent crates
which use this library for common functionality. Each algorithm crate
provides a binary which runs a CCP with that algorithm activated.

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

## Writing a new congestion control algorithm

Congestion control algorithms should implement the `CongAlg` trait, specifying:

### Config

Algorithms specify `Config`, a configuration struct, to define parameters to pass to new instantiations. This allows algorithms to customize their parameters.

### `create(Datapath, Config, DatapathInfo)`

`create()` is called to initialize state for a new flow. `portus` will pass in `Datapath`, a handle to send messages to the datapath (i.e., to set the congestion window via a send pattern), `Config`, an instance of the custom configuration struct created by the CCP binary and passed into `portus::start()`, and `DatapathInfo`, which contains the flow's 5-tuple and the MSS the datapath will use.

Note that almost all algorithm implementations will install a fold function in the `create()` handler; otherwise, no further measurements will be reported.

### `measure(Measurement)`

`measure()` is called when the CCP gets a message as specified by the installed send pattern and/or fold function. The Measurement contains fields specified in the fold function; these can be accessed with `.get_field(field_name, scope)`.
An example follows.

```rust
let scope = Datapath.install_measurement("(def (Report.foo 0))");
...
let zero = Measurement.get_field("Report.foo", scope);
```

### `close()`

Algorithms may optionally specify a handler when about to be deallocated.
