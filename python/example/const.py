import sys
import pyportus as portus

class ConstFlow():
    INIT_RATE = 1000000

    def __init__(self, datapath, datapath_info):
        self.datapath = datapath
        self.datapath_info = datapath_info
        self.rate = ConstFlow.INIT_RATE
        self.datapath.set_program("default", [("Rate", self.rate)])

    def on_report(self, r):
        self.datapath.update_field("Rate", self.rate)


class Const(portus.AlgBase):

    def datapath_programs(self):
        return {
                "default" : """\
                (def (Report
                    (volatile acked 0)
                    (volatile loss 0)
                    (volatile rtt 0)
                ))
                (when true
                    (:= Report.rtt Flow.rtt_sample_us)
                    (:= Report.acked (+ Report.acked Ack.bytes_acked))
                    (:= Report.loss Ack.lost_pkts_sample)
                    (report)
                )
            """
        }

    def new_flow(self, datapath, datapath_info):
        return ConstFlow(datapath, datapath_info)

alg = Const()

portus.start("unix", alg)
