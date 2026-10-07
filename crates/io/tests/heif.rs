//! HEIC photos (#355) open as documents: through the same `import` File › Open uses, upright,
//! with their EXIF and XMP, and an export never turns them a second time.

use photocraft_codecs::{Format, decode, detect, exif_orientation};
use photocraft_doc::{ColorMode, SampleType};
use photocraft_io::{ExportOptions, export, import};

const STRIPS: &[u8] = include_bytes!("../../codecs/tests/heif/rgb-strips-96.heic");
const WITH_EXIF: &[u8] = include_bytes!("../../codecs/tests/heif/with-exif.heic");

#[test]
fn heic_opens_as_an_8_bit_rgb_document() {
    let r = import("IMG_0001.HEIC", STRIPS).unwrap();
    let d = &r.document;
    assert_eq!((d.size.width, d.size.height), (96, 32));
    assert_eq!((d.mode, d.depth), (ColorMode::Rgb, SampleType::U8));
    assert_eq!(d.layers.len(), 1);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn ten_bit_heic_opens_as_a_16_bit_document_with_transparency() {
    let d = import("hdr.heic", include_bytes!("../../codecs/tests/heif/rgba-10bit-29x100.heic")).unwrap().document;
    assert_eq!((d.size.width, d.size.height), (29, 100));
    assert_eq!((d.mode, d.depth), (ColorMode::Rgb, SampleType::U16));
    assert!(!d.layers[0].locks.transparency, "an image with alpha opens as an unlocked layer");
}

#[test]
fn heic_metadata_reaches_the_document_and_exports_upright() {
    let r = import("photo.heic", WITH_EXIF).unwrap();
    let d = &r.document;
    assert_eq!((d.size.width, d.size.height), (2048, 1536));
    assert!(d.metadata.exif.is_some() && d.metadata.xmp.as_deref().is_some_and(|x| x.contains("x:xmpmeta")));
    // HEIC is read-only: save the photo as a JPEG, which keeps the EXIF with Orientation 1.
    let out = export(d, "photo.jpg", &ExportOptions::default()).unwrap();
    assert_eq!(detect(&out.bytes), Some(Format::Jpeg));
    let back = decode(&out.bytes).unwrap();
    assert_eq!(back.dimensions(), (2048, 1536));
    assert_eq!(exif_orientation(back.meta.exif.as_deref().unwrap()), 1);
}

#[test]
fn heic_cannot_be_written_and_says_so() {
    let d = import("x.heic", STRIPS).unwrap().document;
    assert!(export(&d, "x.heic", &ExportOptions::default()).is_err());
}

#[test]
fn broken_heic_is_an_error_not_a_crash() {
    for cut in [0, 12, 40, STRIPS.len() / 2, STRIPS.len() - 1] {
        assert!(import("x.heic", &STRIPS[..cut]).is_err(), "cut at {cut}");
    }
}
