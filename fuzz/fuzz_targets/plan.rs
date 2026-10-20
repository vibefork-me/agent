#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    if let Ok(steps) = vf_orchestrator::parse_plan(&s) {
        assert!(!steps.is_empty() && steps.len() <= vf_orchestrator::MAX_STEPS);
    }
    let _ = vf_orchestrator::parse_rating(&s, "fuzz");
});
