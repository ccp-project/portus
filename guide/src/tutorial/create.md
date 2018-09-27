# Writing `create`

# on_create(self)

As mentioned before, we'll do two simple things here:

1. Initialize state.

The only state our algorithm needs is a congestion window. We'll start by setting a congestion window equal to 10 full packets. We can get the size of a packet (the maximum segment size, or MSS) from the `datapath_info` struct that is automatically provided in `self`:

```python
self.cwnd = float(self.datapath_info.mss * 10)
```

2. Set the initial datapath program.

In this case, we only have one, but if you had created multiple programs in `init_programs` you could choose to use any of them here. The second argument allows us to initialize some values within the datapath. The `self.cwnd` variable we created above is our internal notion of the cwnd, but we need to explicitly send this value to the datapath as well.

```python
self.set_program("default", ["Cwnd", self.cwnd])
```

**NOTE**: There is no default program. Even if you returned programs in `init_programs`, if you don't set one here, again your algorithm won't receive any more callbacks for this flow.

**NOTE**: As mentioned above, the userspace agent is totally separate from the datapath. Any state kept here does not change anything in the datapath automatically. If you want to update variables, such as the cwnd or rate, in the datapath, you need to explicitly do so using `set_program` or `update_fields`.
