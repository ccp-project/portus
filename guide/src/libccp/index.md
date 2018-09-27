# Adding Datapath Support

In order to collect measurements about the network and actually affect the sending behavior of flows, the userspace agent must communicate with the transport layer (datapath). We have designed a common API for the communication between userspace agent and datapath and implemented this in a `C` library called `libccp`. The API is simple and makes supporting a new datapath relatively easy. This rest of this section describes how to use `libccp` to implement a new datapath.

All three of the datapath integrations we provide use `libccp`, so these may be useful references for writing your own datapath integration.



