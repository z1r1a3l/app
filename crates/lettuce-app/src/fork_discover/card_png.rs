//! Writes a Tavern character card into a PNG: the avatar image re-encoded as
//! PNG with the card JSON in `tEXt` chunks (`chara`, plus `ccv3` for V3), the
//! same layout SillyTavern exports and the importer reads.

use base64::Engine as _;
use lettuce_contracts::{ApiError, ApiErrorCode};
use serde_json::Value;

use super::common::error;

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

pub(crate) fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(PNG_MAGIC)
}

/// The avatar as PNG bytes; any format the `image` crate reads (WebP, JPEG,
/// AVIF is not among them) is converted. Without a usable image a neutral
/// square stands in, so the card still imports.
fn avatar_png(image: Option<&[u8]>) -> Result<Vec<u8>, ApiError> {
    let decoded = image.and_then(|bytes| image::load_from_memory(bytes).ok());
    let picture = decoded.unwrap_or_else(|| {
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(256, 256, image::Rgba([90, 96, 110, 255])))
    });
    let mut out = std::io::Cursor::new(Vec::new());
    picture
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| error(ApiErrorCode::Internal, format!("could not encode the card image: {e}")))?;
    Ok(out.into_inner())
}

/// Removes existing text chunks so a stale embedded card cannot win over the
/// one written here.
fn strip_text_chunks(png: &[u8]) -> Vec<u8> {
    let mut out = PNG_MAGIC.to_vec();
    let mut at = PNG_MAGIC.len();
    while at + 8 <= png.len() {
        let length = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
        let end = at + 12 + length;
        if end > png.len() {
            break;
        }
        let kind = &png[at + 4..at + 8];
        if !matches!(kind, b"tEXt" | b"iTXt" | b"zTXt") {
            out.extend_from_slice(&png[at..end]);
        }
        at = end;
    }
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 12);
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out
}

fn text_chunk(keyword: &str, card: &Value) -> Vec<u8> {
    let mut data = keyword.as_bytes().to_vec();
    data.push(0);
    data.extend_from_slice(base64::engine::general_purpose::STANDARD.encode(card.to_string()).as_bytes());
    chunk(b"tEXt", &data)
}

/// A card PNG holding `card` with `image` as its picture.
pub(crate) fn write_card_png(image: Option<&[u8]>, card: &Value) -> Result<Vec<u8>, ApiError> {
    let png = match image {
        Some(bytes) if is_png(bytes) => bytes.to_vec(),
        other => avatar_png(other)?,
    };
    let png = strip_text_chunks(&png);
    // IHDR is always first: magic (8) + length/type (8) + 13 bytes + CRC (4).
    let ihdr_end = PNG_MAGIC.len() + 25;
    if png.len() < ihdr_end || &png[12..16] != b"IHDR" {
        return write_card_png(None, card);
    }
    let mut out = png[..ihdr_end].to_vec();
    out.extend(text_chunk("chara", card));
    if card.get("spec").and_then(Value::as_str) == Some("chara_card_v3") {
        out.extend(text_chunk("ccv3", card));
    }
    out.extend_from_slice(&png[ihdr_end..]);
    Ok(out)
}

/// The card JSON a PNG carries in its `ccv3` or `chara` chunk, if any.
#[cfg(test)]
pub(crate) fn read_card(png: &[u8]) -> Option<Value> {
    let mut at = PNG_MAGIC.len();
    let mut found = None;
    while at + 8 <= png.len() {
        let length = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
        let end = at + 12 + length;
        if end > png.len() {
            break;
        }
        if &png[at + 4..at + 8] == b"tEXt" {
            let data = &png[at + 8..at + 8 + length];
            if let Some(split) = data.iter().position(|b| *b == 0) {
                let keyword = &data[..split];
                if keyword == b"ccv3" || (keyword == b"chara" && found.is_none()) {
                    let decoded = base64::engine::general_purpose::STANDARD.decode(&data[split + 1..]).ok()?;
                    found = serde_json::from_slice(&decoded).ok();
                }
            }
        }
        at = end;
    }
    found
}
