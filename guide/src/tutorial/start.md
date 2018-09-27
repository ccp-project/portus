# Start CCP 

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
