#![no_main]

use libfuzzer_sys::fuzz_target;
use photocraft_codecs::{decode_as_with, DecodeOptions, Format, Limits};

// heic-rs is young and parses untrusted HEIF/HEVC: run with `-timeout=10` so a hang is reported,
// not just a panic. Both orientation paths (container transforms applied, or kept) are covered.
fuzz_target!(|data: &[u8]| {
    let limits = Limits { max_width: 4096, max_height: 4096, max_pixels: 1 << 22, max_alloc: 256 << 20 };
    let keep_orientation = data.first().is_some_and(|b| b & 1 == 1);
    let _ = decode_as_with(Format::Heif, data, &DecodeOptions { limits, keep_orientation });
});
