//! Summary messages aggregate all the Reports received from a bundle of flows
//! over a given period of time and are sent to a controller. These messages
//! are only used in cluster congestion control (i.e. algorithms that implement
//! Aggregator and provide a remote address).

use bytes::{ByteOrder, LittleEndian};

const SUMMARY_MSG_SIZE: usize = 24;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
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

    pub fn as_slice(&self) -> &[u8] {
        unsafe {
            ::std::slice::from_raw_parts(
                (self as *const Summary) as *const u8,
                SUMMARY_MSG_SIZE,
            )
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8]{
        unsafe {
            ::std::slice::from_raw_parts_mut(
                (self as *const Summary) as *mut u8,
                SUMMARY_MSG_SIZE
            )
        }
    }
}


#[cfg(test)]
mod tests {
    use summary::Summary;

    fn fake_summary() -> Summary {
        Summary {
            id: 1,
            num_active_flows: 3,
            bytes_acked: 4344,
            min_rtt: 100000,
            rtt: 123456,
            num_drop_events: 0,
        }
    }
    #[test]
    fn test_summary_copy() {

        let sum = fake_summary();
        let mut got : Summary = Default::default();
        let mut send_buf = [0u8;24];

        sum.write_to(&mut send_buf);
        got.read_from(&send_buf);
        assert_eq!(sum, got);
    }

    #[test]
    fn test_summary_size() {
        assert_eq!(::std::mem::size_of::<Summary>(), 24)
    }

    #[test]
    fn test_summary_zero_copy() {
        let sum = fake_summary();
        let _sum_buf: &[u8] = sum.as_slice();
    }

    #[test]
    fn test_summary_zero_copy_mut() {
        let mut sum = fake_summary();
        let _mut_sum_buf: &mut [u8] = sum.as_mut_slice();  
    }
}
