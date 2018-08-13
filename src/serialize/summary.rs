//! Summary messages aggregate all the Reports received from a bundle of flows
//! over a given period of time and are sent to a controller. These messages
//! are only used in cluster congestion control (i.e. algorithms that implement
//! Aggregator and provide a remote address).


#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Summary {
    pub id: u32,
    pub num_active_flows: u32,
    pub bytes_acked: u32,
    pub min_rtt: u32,
    pub rtt: u32, 
    pub num_drop_events: u16,
}


#[cfg(test)]
mod tests {
    use serialize::summary::Summary;
    use serde::{Serialize,Deserialize};
    use rmps::{Serializer,Deserializer};
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

        let mut buf = Vec::new();
        sum.serialize(&mut Serializer::new(&mut buf)).unwrap();

        let mut de = Deserializer::new(&buf[..]);
        assert_eq!(sum, Deserialize::deserialize(&mut de).unwrap());

    }
}
