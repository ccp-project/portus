# Introduction

The congestion control plane (CCP) is a new platform for writing and sharing datapath-agnostic congestion control algorithms.
It makes it easy to to program sophisticated algorithms (write Rust or Python in a safe user-space environment
as opposed to writing C and a risk of crashing your kernel), and allows the same algorithm 
implementation to be run on a variety of datapaths (Linux Kernel, DPDK or QUIC). 

You probably ended up at this guide for one of three reasons. You want to...
1. **Run an existing algorithm** -- If you just want to use a CCP algorithm that's already been implemented (either by us or someone else),
use the [following section](./setup/index.md) to install the necessary dependencies, then skip to 
[Section 5](./running.md) for instructions on building and running algorithms. 
2. **Build a new algorithm** -- If you want to write your own algorithm using CCP, you should follow the rest of the guide in order.
If you just want to do something simple, it may be sufficient to copy one of our existing algorithm
repositories and modify it to fit your needs, but at the very least you will want to follow
the instructions in the [setup](./setup/index.md) section below.
3. **Reproduce Results** -- If you'd like to reproduce the results found in our SIGCOMM '18 paper, please see the
[eval-scripts](https://github.com/ccp-project/eval-scripts) repository.

**NOTE**: Portus is our Rust implementation of CCP, but in most cases you *won't actually need to 
clone or build it* individually because it is provided as a library. Whether you are writing your 
own algorithm or using an existing one, Portus is available through package managers in Rust (cargo)
and Python (pip). You only need to check out the repository directly if you would like to make changes
to the library/API itself.
