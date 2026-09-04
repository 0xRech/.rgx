#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Write;
use tempfile::NamedTempFile;

fuzz_target!(|data: &[u8]| {
    let mut archive = NamedTempFile::new().expect("create fuzz input");
    archive.write_all(data).expect("write fuzz input");
    let path = archive.path();

    let _ = rgx::archive::list(path);
    let _ = rgx::archive::info(path);
    let _ = rgx::archive::verify(path);
});
