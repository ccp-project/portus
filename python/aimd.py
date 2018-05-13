import sys
import portus

class AIMD(portus.AlgBase):
    INIT_CWND = 10

    def on_create(self):
        self.init_cwnd = float(self.datapath_info.mss * AIMD.INIT_CWND)
        self.cwnd = self.init_cwnd
        self.install_program()

    def install_program(self):
        self.datapath.install(
	    """\
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
            """.format(self.cwnd)
	)

    def handle_timeout(self):
        sys.stdout.write("Timeout! Ignoring...\n")

    def on_report(self, r):
        if r.timeout:
            self.handle_timeout()
            return

        if r.loss > 0 or r.sacked > 0:
            self.cwnd /= 2
        else:
            self.cwnd += (self.datapath_info.mss * (r.acked / self.cwnd))

        self.cwnd = max(self.cwnd, self.init_cwnd)
        self.datapath.update_field("Cwnd", int(self.cwnd))



portus.connect("netlink", AIMD, debug=True, blocking=True)
