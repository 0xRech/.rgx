#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Write;

fn exercise(bytes: &[u8]) {
    if let Ok(mut file) = tempfile::NamedTempFile::new() {
        if file.write_all(bytes).is_ok() {
            let _ = rgx::recipient_archive::read_envelope(file.path());
        }
    }
}

fn structured_envelope(data: &[u8], password_slot: bool) -> Vec<u8> {
    const PREFIX: usize = 16;
    const RECIPIENT_SLOT: usize = 120;
    const PASSWORD_SLOT: usize = 100;

    let header_len = PREFIX + RECIPIENT_SLOT + if password_slot { PASSWORD_SLOT } else { 0 };
    let mut bytes = vec![0u8; header_len];
    bytes[0..4].copy_from_slice(b"RGXR");
    bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
    bytes[6..8].copy_from_slice(&(u16::from(password_slot)).to_le_bytes());
    bytes[8..10].copy_from_slice(&1u16.to_le_bytes());
    bytes[12..16].copy_from_slice(&(header_len as u32).to_le_bytes());

    if !data.is_empty() {
        for (index, byte) in bytes[PREFIX..].iter_mut().enumerate() {
            *byte = data[index % data.len()];
        }
    }

    bytes.extend_from_slice(b"RGXK");
    bytes.push(0);
    bytes.extend_from_slice(data);
    bytes
}

fuzz_target!(|data: &[u8]| {
    // Exercise arbitrary/truncated/corrupt input first.
    exercise(data);

    // Also force structurally plausible RGXR prefixes so fuzzing reaches the
    // recipient-slot and password-slot parsing paths instead of spending most
    // executions rediscovering the magic/version/header-length fields.
    exercise(&structured_envelope(data, false));
    exercise(&structured_envelope(data, true));
});
