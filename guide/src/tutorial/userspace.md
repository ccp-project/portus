# Userspace Library

<details><summary><b>Rust (cargo)</b></summary><p>

First, install Rust, Cargo, and the nightly toolchain (Portus relies on some nightly Rust features):

    curl https://sh.rustup.rs -sSf | sh -- -y -v --default-toolchain nightly

Next, use `cargo` to create a new crate for your algorithm:

    cargo new [your-algorithm-name]

This will create a directory called `[your-algorithm-name]`. Inside that directory, edit the auto-generated `Cargo.toml` and add `portus` as a dependency (at the time of writing, 0.3.3 is the latest stable version):

```toml
...

[dependencies]
portus = "0.3.3"
```

Now you can add `extern crate portus` at the root of your application and then `use portus::[anything]` to include anything from the Portus library. See the [docs](https://docs.rs/portus) for a list of available functions. Minimally, you will need to call either [portus::run](https://docs.rs/portus/0.3.3/portus/fn.run.html) or [portus::spawn](https://docs.rs/portus/0.3.3/portus/fn.spawn.html) and provide it a struct that implements the [CongAlg](https://docs.rs/portus/0.3.3/portus/trait.CongAlg.html) trait. If you need more information, see the [tutorial](../tutorial/index.md), which walks through the full implementation of a simple algorithm.

</p></details>
<br /><br />
<details><summary><b>Python (pip)</b></summary><p>

Assuming you already have python and pip on your machine, all that should be necessary is:

    pip install portus

If you have problems installing through `pip`, try building `Portus` manually using the instructions below.

Once you have the library installed, you should be able to `import portus`. Minimally, you will need to call `portus.connect` and provide it a class that is a subclass of `portus.AlgBase`. For more information, see the [python documentation](../documentation/python/index.md) and the [tutorial](../tutorial/index.md)

</p></details>
<br /><br />
<details><summary><b>Build Manually</b></summary><p>

1. Install Rust and the nightly toolchain.

    (Rust and toolchain manager): `curl https://sh.rustup.rs -sSf | sh -- -y -v --default-toolchain nightly`

    (Nightly toolchain): `rustup install nightly`

2. Checkout Portus version 0.3.3 (you can use a newer version if one exists, but this is the most up to date version at the time of writing).

    `git checkout tags/v0.3.3`

3. Run `make build && make test-portus`. If you run into any issues, check out this page for resolving common problems.

4. (Optional) Install Python dependencies:

    `sudo pip install setuptools setuptools_rust`

5. (Optional) Build the python bindings:

    `cd portus/python && make`

</p></details>
