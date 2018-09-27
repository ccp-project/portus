# Setup

1. Regardless of whether you'd like to run or build an algorithm in Rust or Python, you'll need to install Rust (nightly version) and Cargo (Rust's package manager). Any method of installing Rust should work, but we recommend the following:

        curl https://sh.rustup.rs -sSf | sh -- -y -v --default-toolchain nightly

2. Next, you'll need to pick a datapath and install our datapath integration. We currently support the following three datapaths. If you're not sure, you probably want the Linux Kernel. This integration has received the most use and is the easiest to set up, but we have successfully run all of the same experiments and tests on the other datapaths as well. If you would like to use a datapath that is not listed below, you'll need to add support yourself (see [New Datapaths](../libccp/index.md)).

    <details><summary><b>Linux (kernel module)</b></summary><p>

    Clone our kernel module:

    `git clone https://github.com/ccp-project/ccp-kernel.git`

    Fetch submodules:

    `git submodule update --init --recursive`

    Build:

    `cd ccp-kernel && make`

    Install: (provide `ipc=0` to use netlink sockets):

    `sudo ./ccp_kernel_load ipc=0`

    </p></details>

    <details><summary><b>mTCP / DPDK (fork)</b></summary><p>

    Clone our fork:

    `git clone https://github.com/ccp-project/ccp-mtcp.git`

    Follow the instructions in `REAMDE.md` for building mTCP as normal (and for building DPDK first, if you haven't done so already).

    More detailed instructions coming soon.

    </p></details>

    <details><summary><b>Google QUIC (patch)</b></summary><p>

    Our patch currently lives at [https://github.com/ccp-project/ccp-quic](https://github.com/ccp-project/ccp-quic)

    Follow the instructions in `README.md` for applying the patch.

    More specific instructions for getting QUIC setup from scratch coming soon.

    </p></details>
    <br />

3. Clone an existing algorithm or write your own by following the rest of the guide. We have implemented the following algorithms:

    -   [BBR](https://github.com/ccp-project/bbr)
    -   [Copa](https://github.com/venkatarun95/ccp_copa)
    -   [Nimbus](https://github.com/ccp-project/nimbus)
    -   [Reno and Cubic](https://github.com/ccp-project/generic-cong-avoid)

4. Build the algorithm 

    <details><summary><b>Rust</b></summary><p>

    Just run `cargo build` in the root of the repository. If you run into any build errors, see [Common Problems](./problems/index.md).

    </p></details>
    <details><summary><b>Python</b></summary><p>

    Assuming you have python and pip installed, just run `pip install --user portus`. If you run into any build errors, see [Common Problems](./problems/index.md).

    </p></details>

5. Follow instructions in [Running Algorithms](../running.md) to use CCP.
