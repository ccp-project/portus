//! In a cluster congestion control scenario, the controller sends Allocation
//! messages to each participating host letting it know the aggregate rate
//! it should attempt to send at. It is up to the host to decide how to 
//! schedule its own flows to meet that allocation.

use bytes::{ByteOrder, LittleEndian};

const ALLOCATION_MSG_SIZE: usize = 16;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
#[repr(C, packed)]
pub struct Allocation {
    pub id: u32,
    pub rate: u32,
    pub burst: u32, 
    pub next_summary_in_ms: u32,
}

impl Allocation {
    pub fn write_to(&self, buf: &mut [u8]) {
        LittleEndian::write_u32(&mut buf[0..4], self.id);
        LittleEndian::write_u32(&mut buf[4..8], self.rate);
        LittleEndian::write_u32(&mut buf[8..12], self.burst);
        LittleEndian::write_u32(&mut buf[12..16], self.next_summary_in_ms);
    }

    pub fn read_from(&mut self, buf: &[u8]) {
        self.id = LittleEndian::read_u32(&buf[0..4]);
        self.rate = LittleEndian::read_u32(&buf[4..8]);
        self.burst = LittleEndian::read_u32(&buf[8..12]);
        self.next_summary_in_ms = LittleEndian::read_u32(&buf[12..16]);
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe {
            ::std::slice::from_raw_parts(
                (self as *const Allocation) as *const u8,
                ALLOCATION_MSG_SIZE,
            )
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8]{
        unsafe {
            ::std::slice::from_raw_parts_mut(
                (self as *const Allocation) as *mut u8,
                ALLOCATION_MSG_SIZE
            )
        }
    }

}

#[cfg(test)]
mod tests {
    use allocation::Allocation;

    fn fake_alloc() -> Allocation {
        Allocation {
            id: 1,
            rate : 101010,
            burst: 10000,
            next_summary_in_ms: 10,
        }
    }
    #[test]
    fn test_allocation_copy() {
        let alloc = fake_alloc();
        let mut got : Allocation = Default::default();
        let mut send_buf = [0u8;16];

        alloc.write_to(&mut send_buf);
        got.read_from(&send_buf);
        assert_eq!(alloc, got);
    }

    #[test]
    fn test_allocation_size() {
        assert_eq!(::std::mem::size_of::<Allocation>(), 16)
    }

    #[test]
    fn test_allocation_zero_copy() {
        let alloc = fake_alloc();
        let _alloc_buf: &[u8] = alloc.as_slice();
    }

    #[test]
    fn test_allocation_zero_copy_mut() {
        let mut alloc = fake_alloc();
        let _mut_alloc_buf: &mut [u8] = alloc.as_mut_slice();
    }

}
