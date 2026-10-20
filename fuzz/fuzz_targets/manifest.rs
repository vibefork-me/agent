#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(key) = vf_updater::signing::public_key(vf_updater::RELEASE_PUBLIC_KEY) else {
        return;
    };
    let (manifest, sig) = data.split_at(data.len() / 2);
    assert!(vf_updater::verify_manifest(manifest, &String::from_utf8_lossy(sig), &key).is_err());
});
