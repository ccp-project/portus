# Portus [![Build Status](https://travis-ci.org/ccp-project/portus.svg?branch=master)](https://travis-ci.org/ccp-project/portus)

To get started with CCP, please see our [guide](https://ccp-project.github.io/guide).

*Portus* is an implementation of a congestion control plane (CCP).
It is a library that can be used to write new congestion control
algorithms in user-space. 

Congestion control algorithm implementations live in independent crates
which use this library for common functionality. Each algorithm crate
provides a binary which runs a CCP with that algorithm activated.


### Notes

- The `ipc::netlink` and `ipc::kp` modules will only compile on Linux. If the CCP kernel module is loaded, the test will refuse to run.
- There are no algorithm binaries in this repository: it is just a library and runtime for CCP algorithms. 
