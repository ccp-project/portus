extern crate clap;
extern crate time;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate portus;

use portus::{CongAlg, Config, Datapath, DatapathInfo, Report};
use portus::pattern;
use portus::ipc::Ipc;
use portus::lang::Scope;

pub mod reno;
pub mod cubic;

pub trait GenericCongAvoidAlg {
    fn name() -> String;
    fn new(init_cwnd: u32, mss: u32) -> Self;
    fn curr_cwnd(&self) -> u32;
    fn set_cwnd(&mut self, cwnd: u32);
    fn increase(&mut self, m: &GenericCongAvoidMeasurements);
    fn reduction(&mut self, m: &GenericCongAvoidMeasurements);
    fn reset(&mut self) {}
}

pub struct GenericCongAvoid<T: Ipc, A: GenericCongAvoidAlg> {
    report_option: GenericCongAvoidConfigReport,
	use_compensation: bool,
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    sock_id: u32,
    ss_thresh: u32,
    in_startup: bool,
    mss: u32,
    rtt: u32,
    init_cwnd: u32,
    curr_cwnd_reduction: u32,
    last_cwnd_reduction: time::Timespec,
    alg: A,
}

pub const DEFAULT_SS_THRESH: u32 = 0x7fff_ffff;

#[derive(Debug, Clone)]
pub enum GenericCongAvoidConfigReport {
    Ack,
    Rtt,
    Interval(time::Duration),
}

#[derive(Debug, Clone)]
pub enum GenericCongAvoidConfigSS {
    Fold,
    Pattern,
    Ccp,
}

#[derive(Clone)]
pub struct GenericCongAvoidConfig {
    pub ss_thresh: u32,
    pub init_cwnd: u32,
    pub report: GenericCongAvoidConfigReport,
    pub ss: GenericCongAvoidConfigSS,
    pub use_compensation: bool,
}

impl Default for GenericCongAvoidConfig {
    fn default() -> Self {
        GenericCongAvoidConfig {
            ss_thresh: DEFAULT_SS_THRESH,
            init_cwnd: 0,
            report: GenericCongAvoidConfigReport::Rtt,
            ss: GenericCongAvoidConfigSS::Ccp,
            use_compensation: false,
        }
    }
}

pub struct GenericCongAvoidMeasurements {
    pub acked:       u32,
    pub was_timeout: bool,
    pub sacked:      u32,
    pub loss:        u32,
    pub rtt:         u32,
    pub inflight:    u32,
}

impl<T: Ipc, A: GenericCongAvoidAlg> GenericCongAvoid<T, A> {
    fn send_pattern(&self) {
        match self.control_channel.send_pattern(
            self.sock_id,
            match self.report_option {
                GenericCongAvoidConfigReport::Ack => make_pattern!(
                    pattern::Event::SetCwndAbs(self.alg.curr_cwnd()) => 
                    pattern::Event::WaitNs(100_000_000) 
                ),
                GenericCongAvoidConfigReport::Rtt => make_pattern!(
                    pattern::Event::SetCwndAbs(self.alg.curr_cwnd()) => 
                    pattern::Event::WaitRtts(1.0) => 
                    pattern::Event::Report
                ),
                GenericCongAvoidConfigReport::Interval(dur) => make_pattern!(
                    pattern::Event::SetCwndAbs(self.alg.curr_cwnd()) => 
                    pattern::Event::WaitNs(dur.num_nanoseconds().unwrap() as u32) => 
                    pattern::Event::Report
                ),
            }
        ) {
            Ok(_) => (),
            Err(e) => {
                self.logger.as_ref().map(|log| {
                    warn!(log, "send_pattern"; "err" => ?e);
                });
            }
        }
    }

    fn send_pattern_ss(&self) {
        self.control_channel.send_pattern(
            self.sock_id,
            make_pattern!(
                pattern::Event::SetCwndAbs(self.ss_thresh) =>
                pattern::Event::WaitRtts(1.0) =>
                pattern::Event::SetRateRel(2.0)
            ),
        ).unwrap();
    }

    fn send_pattern_wait(&self) {
        self.control_channel.send_pattern(
            self.sock_id,
            make_pattern!(
                pattern::Event::WaitNs(500_000_000) //=> // 500ms
                //pattern::Event::Report
            ),
        ).unwrap();
    }

    /// Don't update acked, since those acks are already accounted for in slow start
    fn install_fold_ss(&self) -> Option<Scope> {
        match self.control_channel.install_measurement(
            self.sock_id,
            b"
                (def (acked 0) (sacked 0) (loss 0) (timeout false) (rtt 0) (inflight 0))
                (bind Report.sacked (+ Report.sacked Ack.packets_misordered))
                (bind Report.loss Ack.lost_pkts_sample)
                (bind Report.timeout Flow.was_timeout)
                (bind Report.inflight Flow.packets_in_flight)
                (bind Report.rtt Flow.rtt_sample_us)
                (bind Cwnd (+ Cwnd Ack.bytes_acked))
                (bind isUrgent Flow.was_timeout)
                (bind isUrgent (!if isUrgent (> Report.loss 0)))
            "
        ) {
            Ok(s) => Some(s),
            Err(_) => None,
        }
    }

    fn install_fold(&self) -> Option<Scope> {
        match self.control_channel.install_measurement(
            self.sock_id,
            if let GenericCongAvoidConfigReport::Ack = self.report_option {
                b"
                    (def (acked 0) (sacked 0) (loss 0) (timeout false) (rtt 0) (inflight 0))
                    (bind Report.inflight Flow.packets_in_flight)
                    (bind Report.rtt Flow.rtt_sample_us)
                    (bind Report.acked (+ Report.acked Ack.bytes_acked))
                    (bind Report.sacked (+ Report.sacked Ack.packets_misordered))
                    (bind Report.loss Ack.lost_pkts_sample)
                    (bind Report.timeout Flow.was_timeout)
                    (bind isUrgent true)
                "
            } else {
                b"
                    (def (acked 0) (sacked 0) (loss 0) (timeout false) (rtt 0) (inflight 0))
                    (bind Report.inflight Flow.packets_in_flight)
                    (bind Report.rtt Flow.rtt_sample_us)
                    (bind Report.acked (+ Report.acked Ack.bytes_acked))
                    (bind Report.sacked (+ Report.sacked Ack.packets_misordered))
                    (bind Report.loss Ack.lost_pkts_sample)
                    (bind Report.timeout Flow.was_timeout)
                    (bind isUrgent Report.timeout)
                    (bind isUrgent (!if isUrgent (> Report.loss 0)))
                "
            }
        ) {
            Ok(s) => Some(s),
            Err(_) => None,
        }
    }

    fn get_fields(&mut self, m: &Report) -> GenericCongAvoidMeasurements {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let ack = m.get_field(&String::from("Report.acked"), sc).expect(
            "expected acked field in returned measurement",
        ) as u32;

        let sack = m.get_field(&String::from("Report.sacked"), sc).expect(
            "expected sacked field in returned measurement",
        ) as u32;

        let was_timeout = m.get_field(&String::from("Report.timeout"), sc).expect(
            "expected timeout field in returned measurement",
        ) as u32;

        let inflight = m.get_field(&String::from("Report.inflight"), sc).expect(
            "expected inflight field in returned measurement",
        ) as u32;

        let loss = m.get_field(&String::from("Report.loss"), sc).expect(
            "expected loss field in returned measurement",
        ) as u32;

        let rtt = m.get_field(&String::from("Report.rtt"), sc).expect(
            "expected rtt field in returned measurement",
        ) as u32;

        GenericCongAvoidMeasurements{
            acked: ack,
            was_timeout: was_timeout == 1,
            sacked: sack,
            loss: loss,
            rtt: rtt,
            inflight: inflight,
        }
    }

    fn handle_timeout(&mut self) {
        self.ss_thresh /= 2;
        if self.ss_thresh < self.init_cwnd {
            self.ss_thresh = self.init_cwnd;
        }

        self.alg.reset();
        self.alg.set_cwnd(self.init_cwnd);
        self.curr_cwnd_reduction = 0;

        self.logger.as_ref().map(|log| {
            warn!(log, "timeout"; 
                "curr_cwnd (pkts)" => self.init_cwnd / self.mss, 
                "ssthresh" => self.ss_thresh,
            );
        });

        self.send_pattern();
        return;
    }

    fn maybe_reduce_cwnd(&mut self, m: &GenericCongAvoidMeasurements) {
        if m.loss > 0 || m.sacked > 0 {
            if time::now().to_timespec() - self.last_cwnd_reduction > time::Duration::microseconds((f64::from(self.rtt) * 2.0) as i64) {
                self.curr_cwnd_reduction = 0;
            }

            // if loss indicator is nonzero
            // AND the losses in the lossy cwnd have not yet been accounted for
            // OR there is a partial ACK AND cwnd was probing ss_thresh
            if m.loss > 0 && self.curr_cwnd_reduction == 0 || (m.acked > 0 && self.alg.curr_cwnd() == self.ss_thresh) {
                self.alg.reduction(m);
                self.last_cwnd_reduction = time::now().to_timespec();
                self.ss_thresh = self.alg.curr_cwnd();
                self.send_pattern();
            }

            self.curr_cwnd_reduction += m.sacked + m.loss;
        } else if m.acked < self.curr_cwnd_reduction {
            self.curr_cwnd_reduction -= (m.acked as f32 / self.mss as f32) as u32;
        } else {
            self.curr_cwnd_reduction = 0;
        }
    }

    fn slow_start_increase(&mut self, acked: u32) -> u32 {
        let mut new_bytes_acked = acked;
        if self.alg.curr_cwnd() < self.ss_thresh {
            // increase cwnd by 1 per packet, until ssthresh
            if self.alg.curr_cwnd() + new_bytes_acked > self.ss_thresh {
                new_bytes_acked -= self.ss_thresh - self.alg.curr_cwnd();
                self.alg.set_cwnd(self.ss_thresh);
            } else {
                let curr_cwnd = self.alg.curr_cwnd();
                if self.use_compensation {
                    // use a compensating increase function: deliberately overshoot
                    // the "correct" update to keep account for lost throughput due to
                    // infrequent updates. Usually this doesn't matter, but it can when 
                    // the window is increasing exponentially (slow start).
                    let delta = f64::from(new_bytes_acked) / (2.0_f64).ln();
                    self.alg.set_cwnd(curr_cwnd + delta as u32);
                    // let ccp_rtt = (rtt_us + 10_000) as f64;
                    // let delta = ccp_rtt * ccp_rtt / (rtt_us as f64 * rtt_us as f64);
                    // self.cwnd += (new_bytes_acked as f64 * delta) as u32;
                } else {        
                    self.alg.set_cwnd(curr_cwnd + new_bytes_acked);
                }

                new_bytes_acked = 0
            }
        }

        new_bytes_acked
    }
}

impl<T: Ipc, A: GenericCongAvoidAlg> CongAlg<T> for GenericCongAvoid<T, A> {
    type Config = GenericCongAvoidConfig;

    fn name() -> String {
        A::name()
    }

    fn create(control: Datapath<T>, cfg: Config<T, GenericCongAvoid<T, A>>, info: DatapathInfo) -> Self {
        let init_cwnd = if cfg.config.init_cwnd != 0 {
            cfg.config.init_cwnd
        } else {
            info.init_cwnd
        };

        let mut s = Self {
            control_channel: control,
            logger: cfg.logger,
            report_option: cfg.config.report,
            sc: None,
            sock_id: info.sock_id,
            ss_thresh: cfg.config.ss_thresh,
            rtt: 0,
            in_startup: false,
            mss: info.mss,
		    use_compensation: cfg.config.use_compensation,
            init_cwnd: init_cwnd,
            curr_cwnd_reduction: 0,
            last_cwnd_reduction: time::now().to_timespec() - time::Duration::milliseconds(500),
            alg: A::new(init_cwnd, info.mss),
        };

        match cfg.config.ss {
            GenericCongAvoidConfigSS::Fold => {
                s.sc = s.install_fold_ss();
                s.send_pattern_wait();
                s.in_startup = true;
            }
            GenericCongAvoidConfigSS::Pattern => {
                s.sc = s.install_fold();
                s.send_pattern_ss();
            }
            GenericCongAvoidConfigSS::Ccp => {
                s.sc = s.install_fold();
                s.send_pattern();
            }
        }

        s
    }

    fn on_report(&mut self, _sock_id: u32, m: Report) {
        let mut ms = self.get_fields(&m);

        if self.in_startup {
            // install new fold
            self.sc = self.install_fold();
            self.alg.set_cwnd(ms.inflight * self.mss);
            self.in_startup = false;
        }

        self.rtt = ms.rtt;
        if ms.was_timeout {
            self.handle_timeout();
            return;
        }

        ms.acked = self.slow_start_increase(ms.acked);

        // increase the cwnd corresponding to new in-order cumulative ACKs
        self.alg.increase(&ms);
        self.maybe_reduce_cwnd(&ms);
        if self.curr_cwnd_reduction > 0 {
            self.logger.as_ref().map(|log| {
                debug!(log, "in cwnd reduction"; "acked" => ms.acked / self.mss, "deficit" => self.curr_cwnd_reduction);
            });
            return;
        }

        self.send_pattern();

        self.logger.as_ref().map(|log| {
            debug!(log, "got ack"; 
                "acked(pkts)" => ms.acked / self.mss, 
                "curr_cwnd (pkts)" => self.alg.curr_cwnd() / self.mss,
                "inflight (pkts)" => ms.inflight, 
                "loss" => ms.loss, 
                "ssthresh" => self.ss_thresh,
                "rtt" => ms.rtt,
            );
        });
    }
}
