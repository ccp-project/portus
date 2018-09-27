# Common Problems

## Building Userspace Agent

-   Make sure `rust` and `cargo` are up to date using `rustup update`
-   Make sure you are using the nightly version of Rust when building with Cargo
-   If you have problems installing python with `pip`, try building the language 
bindings from the Portus repository directly (just clone Portus and run `make` in the `python/` directory).

## Building Datapath Component

**Linux Kernel**

-   Make sure you are using a supported kernel version: `4.13 <= version <= 4.16`
-   If you recently installed a new kernel, make sure you have rebooted your machine
-   If you are unable to install

## Running CCP

-   If you get an error like the following, it means portus is not able to communicate with 
your datapath. This is either because (1) the datapath integration is not installed / running,
(2) you have selected a different IPC mechanism than the datapath, or (3) the IPC mechanism is not
working. 

    > 'called `Result::unwrap()` on an `Err` value: Error("Failed to install datapath program \"copa\": Error(\"portus err: No such file or directory (os error 2)\")")', src/libcore/result.rs:1009:5
