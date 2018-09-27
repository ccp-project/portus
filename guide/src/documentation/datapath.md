# Datapath Programs

CCP datapath programs use a LISP-like syntax (parenthesized, prefix notation), but have a very limited set of functionality: the language is expressive enough to accomplish anything you might need for congestion control, but limited enough that the programs are very easy to reason about. The language is intentionally not turing complete for security purposes.

Datapath programs are [event-driven](https://en.wikipedia.org/wiki/Event-driven_programming) programs that run in the context of each individual flow.

The required structure of these programs is simple: a single block of variable definitions, followed by a series of events and corresponding event handlers.

## Variable Definitions

Variables only have two possible types: booleans and integers (which are internally represented as `u64`), and must be given a default value.

There are two classes of variables:

-   *Report* variables are included in all reports sent to the userspace agent each time the `(report)` command is called (more on this below).
-   *Control* variables are not included in reports.

Adding the keyword `volatile` before a variable name means that it will automatically be reset to its provided default value when the `(report)` command is called. Both report variables and control variables may be volatile. Volatile control variables will still be reset to their default on each report, they just won't be included in the report.

The first expression in a datapath program _must_ be the variable definition. The syntax is best explained by example:

```c
(def
	(Report
		(volatile reportVarA 0)
		(reportBarB true)
		...
	)
	(contolVarA true)
	(volatile controlVarB false)
	...
)
```

In this example, we have two report variables and two control variables. The include "report" and "control" in the names for clarity, but this is not a requirement. When the `(report)` statement is reached, `reportVarA` and `controlVarB` will be reset to 0 and false, respectively, while `reportVarB` and `controlVarA` will not. 

## Events and Handlers

Following the variable definition expression is a series of `when` clauses, each of which define a condition and a series of statements to be executed when that condition becomes true.

The syntax is `(when [condition] [statements])` where "condition" is any expression (all expressions evaluate to a true or false value, as in C), and "statements" is a series of expressions.

## Operations

-   `+`, `-`, `*`, `/`
-   `:=` or `bind` for setting variables
-   Boolean operators (`||` and `&&`)
-   `>`, `>=`, `<`, `<=`, `==`

## Control Flow

-   `If` statements
-   No loops
-   `(fallthrough)`
-   `(report)`

## Flow and ACK Statistics

The `Flow` struct provides read-only access to a number of a flow-level statistics that are maintained by the datapath:

-   `Flow.packets_in_flight`
-   `Flow.bytes_in_flight`
-   `Flow.bytes_pending`: bytes currently buffered in the network stack waiting to be sent
-   `Flow.rtt_sample_us`: sample of the RTT in microseconds based on ACK arrivals for this flow
-   `Flow.was_timeout`: boolean indicating whether or not this flow has received a timeout
-   `Flow.rate_incoming`: estimate of the receive rate over the last RTT
-   `Flow.rate_outgoing`: estimate of the send rate over the last RTT

The `Ack` struct provides read-only access to specific information from the single most recently received ack.

-   `Ack.bytes_acked`
-   `Ack.packets_acked`
-   `Ack.bytes_misordered`
-   `Ack.packets_misordered`
-   `Ack.ecn_bytes`
-   `Ack.ecn_packets`
-   `Ack.lost_pkts_sample`
-   `Ack.now`

## Sending Behavior

The sending behavior of a flow can only be controlled by modifying either the `Cwnd` or `Rate` variables. These can be set either directly inside of a datapath program, or via an `update_fields` call in the userspace agent.

For example, to increase the cwnd by the number of bytes acked on each ack (i.e. slow-start):

```c
(when true
	(:= Cwnd (+ Cwnd Ack.bytes_acked))
)
```

This is equivalent to `cwnd += Ack.bytes_acked`.

## Common Expressions

### when true

Expressions inside of a `when true` clause are run on every ack.

### Timers

Timers can be achieved using `Micros`

### Rate Patterns

```
(
)
```
