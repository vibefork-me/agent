#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(m) = serde_json::from_slice::<vf_protocol::ws::ClientMessage>(data) {
        assert!(serde_json::to_vec(&m).is_ok());
    }
});
