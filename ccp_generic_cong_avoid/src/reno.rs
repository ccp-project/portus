use GenericCongAvoidAlg;
use GenericCongAvoidMeasurements;

pub struct Reno {
    mss: u32,
    init_cwnd: f64,
    cwnd: f64,
}

impl GenericCongAvoidAlg for Reno {
    fn name() -> String {
        String::from("reno")
    }

    fn new(init_cwnd: u32, mss: u32) -> Self {
        Reno {
            mss: mss,
            init_cwnd: f64::from(init_cwnd),
            cwnd: f64::from(init_cwnd),
        }
    }

    fn curr_cwnd(&self) -> u32 {
        self.cwnd as u32
    }

    fn set_cwnd(&mut self, cwnd: u32) {
        self.cwnd = f64::from(cwnd);
    }

    fn increase(&mut self, m: &GenericCongAvoidMeasurements) {
        // increase cwnd by 1 / cwnd per packet
        self.cwnd += f64::from(self.mss) * (f64::from(m.acked) / self.cwnd);
    }

    fn reduction(&mut self, _m: &GenericCongAvoidMeasurements) {
        self.cwnd /= 2.0;
        if self.cwnd <= self.init_cwnd {
            self.cwnd = self.init_cwnd;
        }
    }
}
