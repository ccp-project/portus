# on_report(self, r)

Now we can implement the core algorithm logic. The structure of `r` was defined in the `Report` structure of our datapath program, so we can access our two fields here: `r.packets_acked` and `r.packets_lost`. First, we'll calculate the correct cwnd adjustment:

```python
if r.packets_lost > 0:
    self.cwnd /= 2
else:
    self.cwnd += (self.datapath_info.mss * r.packets_acked) * (1 / self.cwnd)
```

Then we send that value to the datapath:

```
self.datapath.update_fields(["Cwnd", self.cwnd])
```
