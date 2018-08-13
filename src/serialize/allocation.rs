//! In a cluster congestion control scenario, the controller sends Allocation
//! messages to each participating host letting it know the aggregate rate
//! it should attempt to send at. It is up to the host to decide how to 
//! schedule its own flows to meet that allocation.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub id: u32,
    pub rate: u32,
}

#[cfg(test)]
mod tests {
    use serialize::allocation::Allocation;
    use serde::{Serialize,Deserialize};
    use rmps::{Serializer,Deserializer};
    #[test]
    fn test_allocation_1() {

        let all = Allocation {
            id: 1,
            rate : 101010,
        };

        let mut buf = Vec::new();
        all.serialize(&mut Serializer::new(&mut buf)).unwrap();

        let mut de = Deserializer::new(&buf[..]);
        assert_eq!(all, Deserialize::deserialize(&mut de).unwrap());

    }
}
