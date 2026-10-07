//! HEIF/HEIC decoding against Apple's own encoder and decoder. Most fixtures in `tests/heif/`
//! (from heic-rs, MIT OR Apache-2.0) were encoded by macOS `sips` from synthetic PNGs; each
//! `.ref.png` is Apple's decode of the `.heic` next to it, the ground truth we compare with.
//! The 10-bit one is pillow-heif's (BSD-3-Clause), with the source it was encoded from.

mod common;
use common::*;
use photocraft_codecs::*;

const CHECKER_64: &[u8] = include_bytes!("heif/checker-64.heic");
const STRIPS_96: &[u8] = include_bytes!("heif/rgb-strips-96.heic");
const CHECKER_GRID: &[u8] = include_bytes!("heif/checker-1024.heic");
const WITH_EXIF: &[u8] = include_bytes!("heif/with-exif.heic");

fn reference(png: &[u8]) -> Image {
    decode_as(Format::Png, png).unwrap().convert(ChannelLayout::Rgb, SampleType::U8)
}

/// `bytes` with the first `irot` box set to `angle` × 90° anticlockwise (every fixture has one, at 0).
fn with_irot(bytes: &[u8], angle: u8) -> Vec<u8> {
    let at = bytes.windows(4).position(|w| w == b"irot").unwrap() + 4;
    let mut b = bytes.to_vec();
    b[at] = angle;
    b
}

/// `bytes` with the big-endian EXIF Orientation entry (SHORT, count 1) set to `o`.
fn with_exif_orientation(bytes: &[u8], o: u8) -> Vec<u8> {
    let at = bytes.windows(8).position(|w| w == [0x01, 0x12, 0, 3, 0, 0, 0, 1]).unwrap() + 9;
    let mut b = bytes.to_vec();
    b[at] = o;
    b
}

fn keep() -> DecodeOptions {
    DecodeOptions { keep_orientation: true, ..Default::default() }
}

#[test]
fn heif_is_read_only_and_listed() {
    let c = caps(Format::Heif);
    assert!(c.read && !c.write);
    assert!(ASYMMETRIC_EXCEPTIONS.iter().any(|(f, _)| *f == Format::Heif));
    for ext in ["heic", "HEIC", "photo.heif", "IMG_0001.HIF"] {
        assert_eq!(from_extension(ext), Some(Format::Heif), "{ext}");
    }
    let img = decode(CHECKER_64).unwrap();
    assert!(matches!(encode(&img, Format::Heif, &EncodeOptions::default()), Err(CodecError::Unsupported { .. })));
    assert_eq!(fidelity_warnings(&img, Format::Heif), vec![FidelityWarning::WriteUnsupported { format: Format::Heif }]);
}

#[test]
fn detect_heif_brands() {
    for f in [CHECKER_64, STRIPS_96, CHECKER_GRID, WITH_EXIF] {
        assert_eq!(detect(f), Some(Format::Heif));
    }
    assert_eq!(detect(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"), Some(Format::Heif));
    assert_eq!(detect(b"\0\0\0\x18ftypmif1\0\0\0\0mif1heic"), Some(Format::Heif), "generic major brand");
    assert_eq!(detect(b"\0\0\0\x18ftypheix\0\0\0\0mif1heix"), Some(Format::Heif), "10-bit");
    assert_eq!(detect(b"\0\0\0\x18ftypmsf1\0\0\0\0msf1hevc"), Some(Format::Heif), "image sequence");
    assert_eq!(detect(b"\0\0\0\x18ftypmif1\0\0\0\0mif1avif"), Some(Format::Avif), "AVIF wins over the generic brand");
    assert_eq!(detect(b"\0\0\0\x18ftypisom\0\0\0\0isommp41"), None, "mp4 is not HEIF");
    assert_eq!(detect(b"\0\0\0\x14ftypqt  \0\0\0\0qt  "), None, "QuickTime is not HEIF");
}

#[test]
fn single_picture_matches_apple_decode() {
    for (heic, png) in [(CHECKER_64, &include_bytes!("heif/checker-64.ref.png")[..]), (STRIPS_96, include_bytes!("heif/rgb-strips-96.ref.png"))] {
        let img = decode(heic).unwrap();
        let want = reference(png);
        assert_eq!((img.layout(), img.sample_type()), (ChannelLayout::Rgb, SampleType::U8));
        assert_eq!(img.dimensions(), want.dimensions());
        assert!(max_abs_diff(&img, &want) <= 3.0 / 255.0, "max diff {}", max_abs_diff(&img, &want) * 255.0);
        assert!(psnr(&img, &want) > 45.0);
    }
}

#[test]
fn grid_tiled_photo_matches_apple_decode() {
    // iPhone photos are grids of 512x512 HEVC tiles: here 2x2.
    let img = decode(CHECKER_GRID).unwrap();
    let want = reference(include_bytes!("heif/checker-1024.ref.png"));
    assert_eq!(img.dimensions(), (1024, 1024));
    assert!(max_abs_diff(&img, &want) <= 3.0 / 255.0);
}

#[test]
fn rgb_strips_keep_their_colours() {
    // Three vertical strips, pure red, green and blue: catches swapped channels or matrices.
    let img = decode(STRIPS_96).unwrap();
    for (x, c) in [(16, 0), (48, 1), (80, 2)] {
        for ch in 0..3 {
            let v = img.get(x, 16, ch);
            assert!(if ch == c { v > 0.9 } else { v < 0.1 }, "strip at x={x} channel {ch} = {v}");
        }
    }
}

#[test]
fn container_rotation_is_applied_once() {
    // irot 1 = 90° anticlockwise: the red strip (left) ends at the bottom, blue (right) on top.
    let rotated = with_irot(STRIPS_96, 1);
    let img = decode(&rotated).unwrap();
    assert_eq!(img.dimensions(), (32, 96));
    assert!(img.get(16, 8, 2) > 0.9 && img.get(16, 8, 0) < 0.1, "blue on top");
    assert!(img.get(16, 88, 0) > 0.9 && img.get(16, 88, 2) < 0.1, "red at the bottom");
    // keep_orientation hands back the pixels as coded.
    assert_eq!(decode_with(&rotated, &keep()).unwrap().dimensions(), (96, 32));
}

#[test]
fn exif_and_xmp_survive_and_exif_orientation_is_not_applied_twice() {
    let img = decode(WITH_EXIF).unwrap();
    assert_eq!(img.dimensions(), (2048, 1536));
    let exif = img.meta.exif.as_deref().unwrap();
    assert!(exif.starts_with(b"MM\0*"), "TIFF-structured EXIF, without the HEIF offset header");
    assert!(img.meta.xmp.as_deref().unwrap().contains("x:xmpmeta"));

    // HEIF's EXIF Orientation repeats what irot says; the container is the authority.
    let tagged = with_exif_orientation(WITH_EXIF, 6);
    let img = decode(&tagged).unwrap();
    assert_eq!(img.dimensions(), (2048, 1536), "not turned by the EXIF tag");
    assert_eq!(exif_orientation(img.meta.exif.as_deref().unwrap()), 1, "rewritten so exports don't turn it either");
    let raw = decode_with(&tagged, &keep()).unwrap();
    assert_eq!(exif_orientation(raw.meta.exif.as_deref().unwrap()), 6, "kept verbatim when asked");
}

#[test]
fn limits_are_checked_before_decoding() {
    let tight = |limits| DecodeOptions { limits, ..Default::default() };
    let small = Limits { max_width: 512, ..Default::default() };
    assert!(matches!(decode_with(CHECKER_GRID, &tight(small)), Err(CodecError::LimitExceeded(_))));
    let few = Limits { max_pixels: 1000, ..Default::default() };
    assert!(matches!(decode_with(CHECKER_64, &tight(few)), Err(CodecError::LimitExceeded(_))));
    let bytes = Limits { max_alloc: 64 * 64 * 3 - 1, ..Default::default() };
    assert!(matches!(decode_with(CHECKER_64, &tight(bytes)), Err(CodecError::LimitExceeded(_))));
}

#[test]
fn unsupported_and_broken_files_are_clear_errors() {
    // An ftyp and nothing else: a HEIF image sequence would have a moov here instead of a meta.
    let r = decode(b"\0\0\0\x18ftypmsf1\0\0\0\0msf1hevc");
    assert!(matches!(&r, Err(CodecError::Unsupported { format: Format::Heif, reason }) if reason.contains("sequence")), "{r:?}");
    assert!(decode_as(Format::Heif, b"").is_err());
    assert!(decode_as(Format::Heif, &CHECKER_GRID[..CHECKER_GRID.len() / 2]).is_err(), "half a file");
}

#[test]
fn every_truncation_errors_or_decodes_never_panics() {
    for bytes in [CHECKER_64, STRIPS_96, CHECKER_GRID] {
        for cut in 0..bytes.len() {
            let _ = decode_as(Format::Heif, &bytes[..cut]);
        }
    }
}

#[test]
fn ten_bit_with_alpha_decodes_to_16_bit_rgba() {
    // pillow-heif's fixture: a 16-bit RGBA gradient (the .src.png) encoded as 10-bit HEIF by libheif/x265.
    let img = decode(include_bytes!("heif/rgba-10bit-29x100.heic")).unwrap();
    let src = decode_as(Format::Png, include_bytes!("heif/rgba-10bit-29x100.src.png")).unwrap();
    assert_eq!((img.dimensions(), img.layout(), img.sample_type()), ((29, 100), ChannelLayout::Rgba, SampleType::U16));
    // Depth is kept, not 8 bits stretched: most samples are not multiples of 257.
    let samples = img.to_u16_samples().unwrap();
    assert!(samples.iter().filter(|v| *v % 257 != 0).count() > samples.len() / 2);
    // The alpha plane is coded on its own and comes back almost exactly.
    for y in 0..100 {
        for x in 0..29 {
            assert!((img.get(x, y, 3) - src.get(x, y, 3)).abs() < 0.002, "alpha at {x},{y}");
        }
    }
    // Colour went through 4:2:0 chroma on a 29-pixel-wide saturated gradient, which costs about
    // 26 dB at any depth (the 8-bit encode of the same source scores the same, as does a second
    // decoder); a decoder bug (range, matrix, bit shift) lands far below.
    let rgb = |i: &Image| i.convert(ChannelLayout::Rgb, SampleType::U16);
    assert!(psnr(&rgb(&img), &rgb(&src)) > 24.0, "{}", psnr(&rgb(&img), &rgb(&src)));
}
