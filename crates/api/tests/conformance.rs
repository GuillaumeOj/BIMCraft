//! The in-process transport passes the transport conformance suite (API-04).

use api::InProcess;
use api::conformance::run;
use core_txn::Origin;

#[test]
fn the_in_process_transport_passes_the_conformance_suite() {
    let plugin = Origin::Plugin {
        name: "conformance".to_owned(),
    };
    let result = run(|server| InProcess::new(server, plugin.clone()));
    assert_eq!(result, Ok(()));
}
