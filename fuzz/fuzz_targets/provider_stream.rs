#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) {
        for ev in vf_providers::claude::normalize(&v)
            .into_iter()
            .chain(vf_providers::codex::normalize(&v))
        {
            assert!(serde_json::to_string(&ev).is_ok());
        }
    }
});
