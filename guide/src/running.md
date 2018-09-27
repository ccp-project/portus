# Running CCP Algorithms

At this point, you should have an algorithm that builds successfully, either by following 
the instructions in [Setup](./setup/index.md) to build an existing algorithm or by writing and 
building your own. 

First, you need to start the CCP user agent:

<details><summary><b>Rust</b></summary><p>

When you run `cargo build` in a CCP algorithm repository, `cargo` puts all of the build files
and final binaries in the local `./target` directory. You can use the following command 
to start the CCP userspace agent: 

    sudo ./target/[MODE]/[ALG_NAME] --ipc [IPC]

* `MODE` is either debug or release. You should run `release` unless you're running into problems.
* Check the top of `Cargo.toml` to find the exact algorithm name. It is usually what you'd expect.
* `IPC` specifies what IPC mechanism should be used to contact the datapath component. 
It can either "netlink" (only for Kernel), "chardev" for character device, or "unix" for unix 
sockets.
    - **IMPORTANT** -- this must match the parameters you provided when setting up the
datapath component.
    - `sudo` is only necessary if the IPC mechanism requires it. It is currently required for
    netlink sockets and the character device since they communicate with the Linux kernel,
    but not required if you are using unix sockets. 


For example, a typical run of BBR with the Linux Kernel would look like this:

    sudo ./target/release/bbr --ipc netlink


</p></details>
<br/>
<details><summary><b>Python</b></summary><p>

If you haven't already, install `portus` via pip: `pip install --user portus`. 

Simply run `sudo python [ALG].py`. If you need to change the ipc mechanism, see the 
`connect` method in the python source file. We have not provided command line arguments
by default, but you can always add them for convenience by using e.g. `argparse`. 

</p></details>

<br/>
<hr/>
<br/>

Now that CCP is running, any sockets that set the `TCP_CONGESTION` sockopt to `"ccp"` will
use this algorithm implementation. Some applications such as `iperf` conveniently allow
this to be set directly from the command line.
Add `-Z ccp` for `iperf`(v2) or `-C ccp` for `iperf3`.
