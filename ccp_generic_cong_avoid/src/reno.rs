use GenericCongAvoidAlg;
use GenericCongAvoidMeasurements;

pub struct Reno {
    mss: u32,
    init_cwnd: u32,
    cwnd: u32,
}

impl GenericCongAvoidAlg for Reno {
    fn new(init_cwnd: u32, mss: u32) -> Self {
        Reno {
            mss: mss,
            init_cwnd: init_cwnd,
            cwnd: init_cwnd,
        }
    }

    fn curr_cwnd(&self) -> u32 {
        self.cwnd
    }

    fn set_cwnd(&mut self, cwnd: u32) {
        self.cwnd = cwnd;
    }

    fn increase(&mut self, m: &GenericCongAvoidMeasurements) {
        // increase cwnd by 1 / cwnd per packet
        self.cwnd += (f64::from(self.mss) * (f64::from(m.acked) / f64::from(self.cwnd))) as u32;
    }

    fn reduction(&mut self, _m: &GenericCongAvoidMeasurements) {
        self.cwnd /= 2;
        if self.cwnd <= self.init_cwnd {
            self.cwnd = self.init_cwnd;
        }
    }
}
