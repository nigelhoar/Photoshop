//! HEIF / HEIC (the iPhone and Mac photo format) via `heic-rs`, a pure-Rust
//! HEVC still-picture decoder: single pictures and grid-tiled photos, 8- and
//! 10-bit (10-bit decodes to 16-bit), alpha auxiliary images, ICC, EXIF and XMP.
//! Read-only: see [`crate::ASYMMETRIC_EXCEPTIONS`].
//!
//! HEIF records orientation in the container (`irot`/`imir`, plus a `clap`
//! crop), not in EXIF, so those are applied here and the EXIF Orientation is
//! rewritten to 1. Its EXIF tag only mirrors what the container already says,
//! and applying it as well would turn the photo twice.

use std::borrow::Cow;

use heic_rs::PixelLayout;

use crate::Format;
use crate::error::CodecError;
use crate::image::{ChannelLayout, Image, Metadata, SampleType};
use crate::options::Limits;

const F: Format = Format::Heif;

fn err(e: heic_rs::Error) -> CodecError {
    match e {
        heic_rs::Error::Unsupported(what) => CodecError::unsupported(F, what),
        // The primary item is neither an HEVC picture nor a grid of them.
        heic_rs::Error::MissingBox("hvcC") => CodecError::unsupported(F, "the image is an overlay, an identity derivation or not HEVC-coded"),
        heic_rs::Error::PixelLimit { .. } | heic_rs::Error::BoxTooLarge { .. } => CodecError::LimitExceeded(e.to_string()),
        e => CodecError::malformed(F, e),
    }
}

pub(crate) fn decode(bytes: &[u8], limits: &Limits, keep_orientation: bool) -> Result<Image, CodecError> {
    // The container alone: the declared size is checked before any pixel is decoded.
    let info = heic_rs::probe(bytes).map_err(|e| match e {
        // No `meta` box: no still image, only a `moov` image sequence (a video track).
        heic_rs::Error::MissingBox("meta") => CodecError::unsupported(F, "this file holds an image sequence; only HEIF still images can be opened"),
        e => err(e),
    })?;
    let (layout, sample) =
        (if info.has_alpha { ChannelLayout::Rgba } else { ChannelLayout::Rgb }, if info.bit_depth > 8 { SampleType::U16 } else { SampleType::U8 });
    let (w, h) = if keep_orientation { (info.coded_width, info.coded_height) } else { (info.width, info.height) };
    limits.check(w, h, layout, sample)?;
    let pixel_layout = match (layout, sample) {
        (ChannelLayout::Rgba, SampleType::U16) => PixelLayout::Rgba16,
        (ChannelLayout::Rgba, _) => PixelLayout::Rgba8,
        (_, SampleType::U16) => PixelLayout::Rgb16,
        _ => PixelLayout::Rgb8,
    };
    let options = heic_rs::DecodeOptions::default()
        .with_layout(pixel_layout)
        .with_max_pixels(Some(limits.max_pixels))
        .with_transforms(!keep_orientation)
        .with_alpha(info.has_alpha);
    let decoded = heic_rs::decode(bytes, &options).map_err(err)?;
    let mut img = Image::from_raw(decoded.width, decoded.height, layout, sample, decoded.data)?;
    let (icc, exif, xmp) = metadata(bytes);
    img.icc = icc;
    img.meta = Metadata {
        exif: exif.map(|e| if keep_orientation { e } else { crate::orientation::upright_exif(&e).into_owned() }),
        xmp: xmp.map(|x| if keep_orientation { x } else { crate::orientation::upright_xmp(&x).into_owned() }),
        ..Default::default()
    };
    Ok(img)
}

/// The primary image's ICC profile, EXIF (TIFF structure) and XMP packet. Metadata is a
/// courtesy: a broken metadata item never fails a decode whose pixels came out fine.
fn metadata(bytes: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>, Option<String>) {
    let Ok(ctx) = heic_rs::context::Context::open(bytes) else {
        return (None, None, None);
    };
    let id = ctx.meta.primary;
    let icc = ctx.props(id).ok().and_then(|p| ctx.icc(&p).map(<[u8]>::to_vec));
    let exif = ctx.exif(id).ok().flatten().map(<[u8]>::to_vec);
    let xmp = ctx.xmp_item(id).and_then(|x| ctx.item_data(x).ok()).and_then(|d| match d {
        Cow::Borrowed(b) => String::from_utf8(b.to_vec()).ok(),
        Cow::Owned(b) => String::from_utf8(b).ok(),
    });
    (icc, exif, xmp.map(|x| x.trim_end_matches('\0').to_string()))
}
