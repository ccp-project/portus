# pyportus

This module provides a python interface to the Portus CCP implementation.


## Setup

To build and use the python bindings, you also need setuptools_rust

```bash
sudo pip install setuptools_rust
```

You need to tell it to use the nightly version of rust since some features
are still experimental:
* Check where the package was installed: `pip show setuptools_rust`
* Edit `packages/setuptools_rust/build.py`
* Search for the line containing "rustc" (should be ~102), and change the args to be `["cargo", "+nightly", "rustc", ...`

Now, rather than running make, you can build with

```bash
python setup.py develop
```

Depending on your python environment setup, you may need to run this with `sudo`
(and ensure that your `PATH` variable is preserved):

```bash
sudo env PATH=$PATH python setup.py develop
```

Now you should be able to import the package like so:

```python
import portus
```



## Writing Algorithms


### Overview

An algorithm in portus is represented by a Python class and an instance of this class represents a single TCP flow. A new instance is created *for each* flow. 

This class must be a subclass of `portus.AlgBase` and must implement the two 
following method signatures:
* `on_create(self)` 
* `on_report(self, r)` 
  - `r` is a Report object containing all the fields defined in your datapath program, as well as the current `Cwnd` and `Rate`. Suppose your program defines just a single variable: `(def (acked 0))`, where `acked` adds up the total bytes acked since the last report. This value can be accessed as `r.acked`. Similarly, you can access the cwnd or rate as `r.Cwnd` and `r.Rate` (captialization important!).

Each instantiation of the class will automatically have two fields inside self:
  - `self.datapath` is a pointer to the datapath object that can be used to install
    new datapath programs. It has two available methods:
    1. `datapath.install( str )`, which takes a datapath program as a string. It compiles the program and installs it in the datapath. It does not return anything, though it may raise an exception if your program fails to compile.  
    2. `datapath.update_field(field, val)`, which takes a variable in the `Report` scope of your datapath program and sets the value to `val`. For example, to update just the cwnd, you could use `datapath.update_field("Cwnd", 10000)` (note: cwnd is denoted in bytes, not packets). 
  - `self.datapath_info` is a struct containing fields about this particular flow from the datapath (this could be used, for example, in `on_create` to set an initial cwnd based on the datapath's `mss`)
    * `sock_id`: unique id of this flow in the datapath
    * `init_cwnd`: the initial congestion window this flow will have until you set it
    * `src_ip`, `src_port`, `dst_ip`, `dst_port`: the ip address and port of the source and destination for the flow 


### Datapath Programs

Datapath programs are used to (1) define *which* statistics to send back to your usespace program and *how often* and (2) set the congestion window and/or pacing rate. A datapath program is written in a very simple lisp-like dialect and consists of a single variable definition line followed by any number of when clauses:
```
(def ( ... ) ( ... ))
(when (event) (
  do_stuff ...
)
(when (other_event) (
    do_other_stuff ...
)
```

# NOTE: the following info is out of date as the datapath program API has been
updated

##### 1. Report Variable Definitions

Example: `(def (Report.acked 0) (Report.rtt 0) (Report.timeout false))`

This line defines the names and initial values of variables in the __report scope__. Calling `(report)` in your datapath program results in a call to your algorithm's `on_report` function with the current value of these variables. *After the call these variables are reset back to their initial value.*

__NOTE__: Variables in datapath programs are written as `{scope}.{name}`. For example, the `acked` variable in the `Report` scope is written as `Report.acked`. Therefore, *all variables defined in this line must* start with `Report.` However, when you access them in `on_report`, you just provide the variable name. In our example above, `Report.rtt` defines the variable `rtt` in the `Report` scope. If we want to access this value in `on_report(r)`, we'd use `r.rtt` (i.e. *not* `r.Report.rtt`). 


##### 2. When Clauses

When clauses consist of a boolean expression and a set of instructions. On each ack, the datapath checks the boolean expression, and if it evaluates to `true`, it runs the set of instructions. For example, the following when clause sends a report (i.e. calls the `on_report` function) once every rtt:
```
(when (> Micros Flow.rtt_sample_us)
    (report)
)
```

### Putting it all together

A sample algorithm definition showing the full API:
```python
import portus

# Class must sublcass portus.AlgBase
class SampleCCAlg(portus.AlgBase):
  # Init must take exactly these parameters
  def __init__(self, datapath, datapath_info):
    # Store a copy of the datapath and info for later
    self.datapath = datapath
    self.datapath_info = datapath_info
    
    # Internally store an initial cwnd value
    self.cwnd = 10 * self.datapath_info.mss
    
    # Install an initial datapath program to keep track of the RTT and report it once per RTT
    # The first when clause is true on every single ack,
    #    which means the 'Report.rtt' field will always keep the latest rtt sample
    # The second when clause is true once one rtt's worth of time has passed, 
    #    at which point it will trigger on_report, and Micros (and Report.rtt) will be reset to 0
    self.datapath.install("""\
    (def
        (Report.rtt 0)
    )
    (when true
        (:= Report.rtt Flow.rtt_sample_us)
        (fallthrough)
    )
    (when (> Micros Flow.rtt_sample_us)
        (report)
    )
    """)

  # This function will be called once per RTT, and the report struct `r` will contain:
  # "rtt", "Cwnd", and "Rate"
  def on_report(self, r):
      # Compute new cwnd internally 
      # If the rtt has decreased, increase the cwnd by 1 packet, else decrease by 1 packet
      if self.last_rtt < r.rtt:
          self.cwnd += self.datapath_info.mss
      else:
          self.cwnd -= self.datapath_info.mss
      self.last_rtt = r.rtt
      
      # Send this new value of cwnd to the datapath
      self.datapath.update_field("Cwnd", self.cwnd)
    
```


### Important Notes
1. You should install an initial datapath program in your `__init__` implementation, otherwise you will not receive any reports and nothing else will happen. You can always install a different datapath program later when handling `on_report`.
2. If you want to print anything, you should use `sys.stderr.write()` (note that you need to `import sys` and that it doesn't automatically add new lines for you like `print` does). 
3. You *must* store a reference to `datapath` in `self` called "datapath" (i.e. `self.datapath = datapath`), because the library internally uses this to access the datapath struct as well. 


### Starting CCP 

The CCP entry point is `portus.connect(ipc_type, class, debug, blocking)`:
* `ipc_type (string)`: (netlink | unix | char) on linux or (unix) on mac
* `class`: your algorithm class, e.g. `SampleCCAlg`
* `debug (bool)`: if true, the CCP will log all messages passed between the ccp and datapath
* `blocking (bool)`: if true, use blocking ipc reads, otherwise use non-blocking

For example: `portus.connect("netlink", SampleCCAlg, debug=True, blocking=True)`. 

Regardless of whether you use blocking or non-blocking sockets, `connect` will block forever (to stop the CCP just send ctrl+c or kill the process). 

### Example

For a full working example of both defining an algorithm and running the CCP, see the simple AIMD scheme in `./aimd.py` and try running it: `sudo python aimd.py`. 
