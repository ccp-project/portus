use portus_export::register_ccp_alg;

#[register_ccp_alg]
pub struct MySampleAlgorithm;

#[test]
fn works() {
    let _x = MySampleAlgorithm {};
}
