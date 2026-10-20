#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    if let Ok(p) = vf_core::ProjectPath::parse(&s) {
        let root = std::path::Path::new(if cfg!(windows) { r"D:\proj" } else { "/proj" });
        assert!(p.under(root).starts_with(root));
        assert!(
            p.as_path()
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        );
    }
});
