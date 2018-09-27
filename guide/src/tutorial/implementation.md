# Implementation

<details><summary><b>Python</b></summary>
<p>
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

Starting with this base, we will go through the implementation of AIMD one function at a time...

</p>
</details>

<details><summary>Rust</summary>
<p>
    Some rust stuff here...
</p>
</details>
