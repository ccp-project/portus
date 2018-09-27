# CCP Programming Model

Traditionally, congestion control algorithms, and thus the API provided by datapaths, have been designed around the idea of taking some action upon receiving each packet acknowledgement.

In contrast, CCP is built around the idea of moving the core congestion control logic off of the immediate datapath in order to gain programmability. However, in order to maintain good performance, rather than moving it entirely, we split the implementation of a congestion control algorithm between the datapath and a userspace agent. The datapath component is restricted to a simple LISP-like language and is primarily used for collecting statistics and dictating sending behavior, while the userspace algorithm can be arbitrarily complex and is written in a language like Rust or Python with the support of all their available libraries.

Thus, writing an algorithm in CCP actually involves writing two programs that work in tandem and communicate asynchronously, which requires a slightly different way of thinking about congestion control implementation. Typically, the datapath program will look at each ACK to gather statistics and periodically report a summary of those statistics to the userspace agent, who then makes a decision about how to modify the sending behavior. In other words, the userspace agent is typically making a decision based on a batch of ACKs rather than each individual one. We initially experimented with reporting every single ACK to the userspace agent, but we found that (1) this doesn't scale sufficiently, and (2) responding on a less-frequent basis, e.g. once-per-RTT, does not compromise performance of any algorithms we tested. 

More details and experimental results can be found in our [SIGCOMM '18 paper](https://people.csail.mit.edu/frankc/pubs/ccp-sigcomm18.pdf).

The syntax and structure of the datapath program language is detailed in the [following section](../documentation/datapath.md). 
