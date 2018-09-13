//! In a cluster congestion control scenario, the controller sends Allocation
//! messages to each participating host letting it know the aggregate rate
//! it should attempt to send at. It is up to the host to decide how to 
//! schedule its own flows to meet that allocation.

use bytes::{ByteOrder, LittleEndian};

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Allocation {
    pub id: u32,
    pub rate: u32,
}

impl Allocation {
    pub fn write_to(&self, buf: &mut [u8]) {
        LittleEndian::write_u32(&mut buf[0..4], self.id);
        LittleEndian::write_u32(&mut buf[4..8], self.rate);
    }

    pub fn read_from(&mut self, buf: &[u8]) {
        self.id = LittleEndian::read_u32(&buf[0..4]);
        self.rate = LittleEndian::read_u32(&buf[4..8]);
    }
}

#[cfg(test)]
mod tests {
    use serialize::allocation::Allocation;
    #[test]
    fn test_allocation_1() {

        let alloc = Allocation {
            id: 1,
            rate : 101010,
        };
        let mut got : Allocation = Default::default();
        let mut send_buf = [0u8;8];

        alloc.write_to(&mut send_buf);
        got.read_from(&send_buf);
        assert_eq!(alloc, got);
    }
}
