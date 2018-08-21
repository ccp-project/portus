//! Summary messages aggregate all the Reports received from a bundle of flows
//! over a given period of time and are sent to a controller. These messages
//! are only used in cluster congestion control (i.e. algorithms that implement
//! Aggregator and provide a remote address).

use bytes::{ByteOrder, LittleEndian};

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Summary {
    pub id: u32,
    pub num_active_flows: u32,
    pub bytes_acked: u32,
    pub min_rtt: u32,
    pub rtt: u32, 
    pub num_drop_events: u32,
}

impl Summary {
    pub fn write_to(&self, buf: &mut [u8]) {
        LittleEndian::write_u32(&mut buf[0..4], self.id);
        LittleEndian::write_u32(&mut buf[4..8], self.num_active_flows);
        LittleEndian::write_u32(&mut buf[8..12], self.bytes_acked);
        LittleEndian::write_u32(&mut buf[12..16], self.min_rtt);
        LittleEndian::write_u32(&mut buf[16..20], self.rtt);
        LittleEndian::write_u32(&mut buf[20..24], self.num_drop_events);
    }

    pub fn read_from(&mut self, buf: &[u8]) {
        self.id = LittleEndian::read_u32(&buf[0..4]);
        self.num_active_flows = LittleEndian::read_u32(&buf[4..8]);
        self.bytes_acked = LittleEndian::read_u32(&buf[8..12]);
        self.min_rtt = LittleEndian::read_u32(&buf[12..16]);
        self.rtt = LittleEndian::read_u32(&buf[16..20]);
        self.num_drop_events = LittleEndian::read_u32(&buf[20..24]);
    }
}


#[cfg(test)]
mod tests {
    use serialize::summary::Summary;
    #[test]
    fn test_summary_1() {

        let sum = Summary {
            id: 1,
            num_active_flows: 3,
            bytes_acked: 4344,
            min_rtt: 100000,
            rtt: 123456,
            num_drop_events: 0,
        };
        let mut got : Summary = Default::default();
        let mut send_buf = [0u8;24];

        sum.write_to(&mut send_buf);
        got.read_from(&send_buf);
        assert_eq!(sum, got);
    }
}
