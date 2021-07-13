import sys
import portus

class OscFlow():
    INIT_CWND = 10

    def __init__(self, datapath, datapath_info):
        self.datapath = datapath
        self.datapath_info = datapath_info
        self.datapath.set_program("default", [("Rate", 5000000)])

    def on_report(self, r):
        sys.stdout.write("rtt: {} rin: {} rout: {}\n".format(r.rtt, r.rin, r.rout))

class Osc(portus.AlgBase):

    def datapath_programs(self):
        return {
                "default" : """\
                (def (Report
                    (volatile rtt 0)
                    (volatile rin 0)
                    (volatile rout 0))
                    (pulseState 0)
                )
                (when true
                    (:= Report.rtt Flow.rtt_sample_us)
                    (:= Report.rin Flow.rate_incoming)
                    (:= Report.rout Flow.rate_outgoing)
                    (fallthrough)
                )
                (when (> Micros Flow.rtt_sample_us)
                    (:= pulseState (+ pulseState 1))
                    (:= Micros 0)
                    (report)
                    (fallthrough)
                )
                (when (== pulseState 1)
                    (:= Rate 5500000)
                )
                (when (== pulseState 2)
                    (:= Rate 6000000)
                )
                (when (== pulseState 3)
                    (:= Rate 5500000)
                )
                (when (== pulseState 4)
                    (:= Rate 5000000)
                )
                (when (== pulseState 5)
                    (:= Rate 4500000)
                )
                (when (== pulseState 6)
                    (:= Rate 4000000)
                )
                (when (== pulseState 7)
                    (:= Rate 4500000)
                )
                (when (== pulseState 8)
                    (:= Rate 5000000)
                    (:= pulseState 0)
                )
            """
        }

    def new_flow(self, datapath, datapath_info):
        return OscFlow(datapath, datapath_info)

alg = Osc()

portus.start("unix", alg, debug=True)
