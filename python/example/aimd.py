import sys
import pyportus as portus

class AIMDFlow():
    INIT_CWND = 10

    def __init__(self, datapath, datapath_info):
        self.datapath = datapath
        self.datapath_info = datapath_info
        self.init_cwnd = float(self.datapath_info.mss * AIMDFlow.INIT_CWND)
        self.cwnd = self.init_cwnd
        self.datapath.set_program("default", [("Cwnd", int(self.cwnd))])

    def on_report(self, r):
        if r.loss > 0 or r.sacked > 0:
            self.cwnd /= 2
        else:
            self.cwnd += (self.datapath_info.mss * (r.acked / self.cwnd))

        print(f"acked {r.acked} rtt {r.rtt} inflight {r.inflight}")
        self.cwnd = max(self.cwnd, self.init_cwnd)
        self.datapath.update_field("Cwnd", int(self.cwnd))


class AIMD(portus.AlgBase):
    def datapath_programs(self):
        return {
                "default" : """\
                (def (Report
                    (volatile acked 0)
                    (volatile sacked 0)
                    (volatile loss 0)
                    (volatile timeout false)
                    (volatile rtt 0)
                    (volatile inflight 0)
                ))
                (when true
                    (:= Report.inflight Flow.packets_in_flight)
                    (:= Report.rtt Flow.rtt_sample_us)
                    (:= Report.acked (+ Report.acked Ack.bytes_acked))
                    (:= Report.sacked (+ Report.sacked Ack.packets_misordered))
                    (:= Report.loss Ack.lost_pkts_sample)
                    (:= Report.timeout Flow.was_timeout)
                    (fallthrough)
                )
                (when (|| Report.timeout (> Report.loss 0))
                    (report)
                    (:= Micros 0)
                )
                (when (> Micros Flow.rtt_sample_us)
                    (report)
                    (:= Micros 0)
                )
            """
        }

    def new_flow(self, datapath, datapath_info):
        return AIMDFlow(datapath, datapath_info)

alg = AIMD()

portus.start("netlink", alg)
