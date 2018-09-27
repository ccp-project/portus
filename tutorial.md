# Tutorial

In this tutorial, we'll walk through the process of implementing a very simple congestion control algorithm in CCP and then running and debugging that implementation in an emulator.

We'll be using **Portus v0.3.3**. The first part covering algorithm implementation is agnostic to datapath, but for the second part we'll be using the **Linux Kernel datapath**. Please let us know if you run into any problems during this tutorial by submitting a GitHub issue.

## Setup

CCP is written in Rust, and thus requires Rust to be built, even if you are only using the Python bindings.

1. Install Rust and the nightly toolchain.

(Rust and toolchain manager):
`curl https://sh.rustup.rs -sSf | sh`

(Nightly toolchain):
`rustup install nightly`

2. Checkout Portus version 0.3.3 (you can use a newer version if one exists, but this is the most up to date version at the time of writing).

`git checkout tags/v0.3.3`

3. Run `make` to build Portus

4. Install datapath support (The instructions below are specific to the Linux kernel. We also have support for [Google QUIC](http://github.com/ccp-project/ccp-quic) and [mTCP/DPDK](http://github.com/ccp-project/ccp-mtcp). If you want to use a different datpath, checkout [libccp](http://github.com/ccp-project/libccp).)

Clone our kernel module:

`git clone https://github.com/ccp-project/ccp-kernel.git`

Build:

`cd ccp-kernel && make`

Install: (provide `ipc=0` to use netlink sockets):

`sudo ./ccp_kernel_load ipc=0`

5. Install Python dependencies:

`sudo pip install setuptools setuptools_rust`

6. Build the python bindings

`cd portus/python && make`

## Background

The focus here is to explain the CCP programming model, so we'll be implementing a very simple scheme: AIMD (additive-increase multiplicative-decrease). Before we even start talking about CCP, let's briefly go over exactly how the algorithm works and what kind of behavior we expect to see. We assume a basic familiarity with the problem of congestion control. If you need some background, [Van Jacobson's paper](http://web.mit.edu/6.829/www/currentsemester/papers/vanjacobson-congavoid.pdf) is a good place to start.

### AIMD Scheme

The high-level idea is to start with a low cwnd, and then as ACKs are received, probe for more bandwidth by continually increasing the cwnd (additively) until eventually a loss occurs, which signals congestion. We then cut our rate (multiplicatively) and repeat. If you were to graph the congestion window over time of a single flow running this scheme in the prescence of a droptail buffer, it would exhibit the classic "sawtooth" behavior:

[img=sawtooth]

Specifically, we'll use the following algorithm:

-   On each ACK, increase CWND by 1/cwnd (this has the affect of increasing the cwnd by roughly 1 packet per RTT)
-   On each loss, cut CWND by 1/2

### CCP Programming Model

Traditionally, congestion control algorithms, and thus the API provided by datapaths, have been designed around the idea of taking some action upon receiving each packet acknowledgement.

However, CCP is built around the idea of moving the core congestion control logic out of the immediate datapath in order to gain programmability. In order to maintain good performance, rather than moving it entirely, we split the implementation of a congestion control algorithm between the datapath and a userspace agent. The datapath component is restricted to a simple LISP-like language and is primarily used for collecting statistics and dictating sending behavior, while the userspace algorithm can be arbitrarily complex and is written in a language like Rust or Python with the support of all their available libraries.
Thus, writing an algorithm in CCP actually involves writing two programs that work in tandem and communicate asynchronously, which requires a slightly different way of thinking about congestion control implementation.

More details can be found in our [SIGCOMM '18 paper](https://people.csail.mit.edu/frankc/pubs/ccp-sigcomm.pdf).

## Implementation

In Portus, a congestion control algorithm is represented as a python class and a new instance of this class is created for each flow (and deleted when this flow ends). This class must be a subclass of `portus.AlgBase` and must implement the following 3 methods:

1. `init_programs()`: return all DATAPATH programs that your algorithm will use.
    - Returns a list of 2-tuples, where each tuple consists of (1) a unique (string-literal) name for the program, and (2) the program itself (also a string-literal).
    - **Note**: this is a static/class method, it does not take self, and thus will produce the same result for all flows
2. `on_create(self)`: initialize any per-flow state (and store it in `self`), and choose an initial datapath program to use.
    - Doesn't return anything.
3. `on_report(self, r)`: all algorithm logic goes here, called each time your datapath progarm executes `(report)`.
    - The report parameter `r` is a report containing all of the fields you defined in your datapath program Report structure, plus two permanent fields, `Cwnd` and `Rate`, which are the current congestion window and rate in the datapath at the time of this report.
    - Doesn't return anything.

Since python (2) doesn't have type annotations, we use our own runtime type checker to ensure:

-   Your class is a subclass of `portus.AlgBase`.
-   All 3 methods are implemented.
-   Each method takes the correct parameters (names must match _exactly_).
-   Each method returns the correct type (mainly relevant for `init_programs`).

Thus, the minimal working example that will pass our type checker is as follows:

```python
import portus

class AIMD(portus.AlgBase):          # <-- subclass of AlgBase

    def init_programs(self):
        return [("name", "program")] # <-- note the example return type

    def on_create(self):
        # TODO initial state
        # TODO self.datapath.set_program(...)
        pass

    def on_report(self, r):
        # TODO do stuff with r
        # TODO maybe change cwnd or rate: self.datapath.update_field(...)
        pass
```

Starting with this base, we will go through the implementation of AIMD one function at a time:

### init_programs(self)

To start out, we need to think about how to split the implemention between the datapath and userspace. For AIMD, the main things we need to keep track of are ACKs to increment our window and losses to cut our window. Although we could ask the datapath to give us this information on every ACK or loss detected, this wouldn't scale well. Since it takes one RTT for our algorithm to get feedback about a given packet on the network in steady state, it is pretty natural to update our state once per RTT (note: this is by no means fundamentally _correct_, so feel free to play around with differnet time scales!). A common pattern in CCP is to aggregate per-ACK statistics (such as the number of bytes ACKed) over a given time interval and then periodically report them to the userspace agent, which handles the logic of how to adjust the window or rate.

Although they work in tandem, it makes sense to think about the datapath program first, since the userspace agent reacts to events generated by the datapath. (For a detailed background on CCP datapath programs, [read this](TODO).)

As mentioned above, for this algorithm we want to collect two statistics in the datapath, number of packets ACKed and number of packets lost, so we'll define our `Report` structure as follows:

```
(def (Report
    (volatile packets_acked 0)
    (volatile packets_lost 0)
))
```

The `0` after each value sets the default value and the `volatile` keyword tells the datapath to reset each value to it's default (0) after each `report` command.

Next, we'll specify our event handlers. First, we'll use the `when true` event to update our counters on each ACK (the event handler is run on each ACK, represented by the `Ack` structure):

```
(when true
    (:= Report.packets_acked (+ Report.packets_acked Ack.packets_acked))
    (:= Report.packets_lost (+ Report.packets_lost Ack.lost_pkts_sample))
    (fallthrough)
)
```

The `(fallthrough)` statement tells the datapath to continue checking the rest of our event handlers. Without this statement, the datapath would stop here even if the other event conditions resolved to true.

The only other condition we need is a timer that sends a report once per RTT. This can be implemented using the `Micros` variable. This variable starts at 0 and represents the number of microseconds since it was last reset. (`Flow` contains some flow-level statistics, such as the datapath's current estimate of the RTT in microseconds, which comes in handy here):

```
(when (> Micros Flow.rtt_sample_us)
    (report)
    (:= Micros 0)
)
```

This condition resolves to true if `Micros` is greater than one RTT, and then resets it so that it can fire again on the next RTT. (NOTE: Micros is only reset by you. If you forgot to reset it, this condition would keep firing on every ACK because it will only be increment on each ACK).

Although this is not absolutely necessary, when a loss happens, we should probably know about that right away. A loss (in the simplistic model assumed by this algorithm) indicates that we are putting packets into the network too quickly. Therefore, if we were to continue sending at this rate for up to 1 RTT after receiving the first loss, we may introduce further losses. We can add another `when` clause to give us a report immediately upon any loss.

```
(when (> Report.packets_lost 0)
    (report)
)
```

We can now write `init_programs` by putting this together into a string literal and giving our program a name ("default"):

```python
def init_programs(self):
    return [
        (("default"), ("""\
            (def (Report
                (volatile packets_acked 0)
                (volatile packets_lost 0)
            ))
            (when true
                (:= Report.packets_acked (+ Report.packets_acked Ack.packets_acked))
                (:= Report.packets_lost (+ Report.packets_lost Ack.lost_pkts_sample))
                (fallthrough)
            )
            (when (> Micros Flow.rtt_sample_us)
                (report)
                (:= Micros 0)
            )
            (when (> Report.packets_lost 0)
                (report)
            )
        """)
    ]
```

**NOTE**: If you don't return any programs here, there will be no logic to decide when your algorithm receives reports, and thus your algorithm won't receive any callbacks beyond the creation of each flow.

### on_create(self)

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

### on_report(self, r)

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

### Putting it all together

[aimd.py](https://github.com/ccp-project/portus/blob/master/python/aimd.py)

## Running Portus

Now that we have an algorithm implementation, we can start CCP by calling `portus.connect`. Connect has 2 required arguments and two optional arguments:

1. (required) `ipc_method` : string, name of ipc mechanism to use, mostly dependent on datapath (e.g. use "netlink" for the Linux kernel, this must match the IPC mechanism supplied to `ccp_kernel_load`)
2. (required) `algorithm` : the Python class representing the algorithm you wish to run
3. (optional) `debug` : boolean, default `False`, enable/disable verbose output
4. (optional) `blocking` : boolean, default `True`, which means blocks forever. If `False`, this call will spawn a new thread for CCP and then return. You probably want to use `False` if you are embeding your algorithm implementation within a larger application.

For example, if your `AIMD` source is in `aimd.py`, running the following script would start CCP and run forever until killed.

```python
import portus
from aimd import AIMD

portus.connect("netlink", AIMD, debug=True, blocking=True)
```

### Debugging (TODO)

Use `sys.stdout.write("...\n")`

### Emulation (TODO)

Install Mahimahi

`iperf -s -p 9001`

`iperf -c $MAHIMAHI-BASE -p 9001 -i 1 -Z ccp`

### Live Monitoring (TODO)

[mm-live](http://github.com/fcangialosi/mm-live)
