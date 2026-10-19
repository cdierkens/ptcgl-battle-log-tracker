//! Uncompressed BMP encoding for Vision OCR input.
//!
//! `VNImageRequestHandler` needs image bytes, and the previous path handed it a
//! PNG — so every OCR call paid a full deflate pass. BMP is uncompressed, so
//! encoding is a header plus a bottom-up row flip.

use crate::ports::Frame;

const FILE_HEADER_LEN: usize = 14;
const INFO_HEADER_LEN: usize = 40;
const HEADER_LEN: usize = FILE_HEADER_LEN + INFO_HEADER_LEN;
const BPP: u32 = 32;

/// Encode an RGBA8 frame as an uncompressed 32-bit BMP (BI_RGB, bottom-up).
///
/// Alpha is forced opaque because a screen capture can carry `alpha == 0`,
/// which Vision may read as a fully transparent image and match nothing.
/// Returns `None` for a degenerate (zero-sized) frame or a buffer too short to
/// hold `width * height` pixels.
pub fn encode_bmp(frame: &Frame) -> Option<Vec<u8>> {
    if frame.width == 0 || frame.height == 0 {
        return None;
    }
    // width * 4 is always 4-byte aligned at 32bpp, so no row padding is needed.
    let row_bytes = frame.width as usize * 4;
    let pixel_bytes = row_bytes * frame.height as usize;
    // A short buffer would panic below and take the detection thread with it.
    // The PNG path this replaced returned no matches instead; keep that.
    if frame.rgba.len() < pixel_bytes {
        return None;
    }
    let mut out = Vec::with_capacity(HEADER_LEN + pixel_bytes);

    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((HEADER_LEN + pixel_bytes) as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved
    out.extend_from_slice(&(HEADER_LEN as u32).to_le_bytes()); // pixel data offset

    out.extend_from_slice(&(INFO_HEADER_LEN as u32).to_le_bytes());
    out.extend_from_slice(&(frame.width as i32).to_le_bytes());
    // A positive height means rows are stored bottom-up.
    out.extend_from_slice(&(frame.height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&(BPP as u16).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB, uncompressed
    out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes()); // x pixels per metre
    out.extend_from_slice(&0i32.to_le_bytes()); // y pixels per metre
    out.extend_from_slice(&0u32.to_le_bytes()); // palette colours used
    out.extend_from_slice(&0u32.to_le_bytes()); // important colours

    let rgba = &frame.rgba;
    // BMP stores the first row last, so walk y in reverse. Pixels are BGRA.
    for y in (0..frame.height as usize).rev() {
        let row = y * row_bytes;
        for px in rgba[row..row + row_bytes].as_chunks::<4>().0 {
            out.push(px[2]); // B
            out.push(px[1]); // G
            out.push(px[0]); // R
            out.push(0xFF); // A, forced opaque
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, rgba: Vec<u8>) -> Frame {
        Frame { width: w, height: h, rgba }
    }

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Frame {
        frame(w, h, c.repeat((w * h) as usize))
    }

    #[test]
    fn writes_the_bmp_file_header() {
        let bmp = encode_bmp(&solid(2, 2, [1, 2, 3, 4])).unwrap();
        assert_eq!(&bmp[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()), (HEADER_LEN + 16) as u32);
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), HEADER_LEN as u32);
    }

    #[test]
    fn writes_the_info_header_dimensions() {
        let bmp = encode_bmp(&solid(3, 5, [0, 0, 0, 255])).unwrap();
        assert_eq!(u32::from_le_bytes(bmp[14..18].try_into().unwrap()), 40);
        assert_eq!(i32::from_le_bytes(bmp[18..22].try_into().unwrap()), 3);
        assert_eq!(i32::from_le_bytes(bmp[22..26].try_into().unwrap()), 5);
        assert_eq!(u16::from_le_bytes(bmp[26..28].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(bmp[28..30].try_into().unwrap()), 32);
        assert_eq!(u32::from_le_bytes(bmp[30..34].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(bmp[34..38].try_into().unwrap()), 60);
    }

    #[test]
    fn total_length_is_header_plus_pixels() {
        let bmp = encode_bmp(&solid(7, 3, [9, 9, 9, 255])).unwrap();
        assert_eq!(bmp.len(), HEADER_LEN + 7 * 3 * 4);
    }

    #[test]
    fn pixels_are_bgra_bottom_up() {
        // Top row red, bottom row blue.
        let bmp = encode_bmp(&frame(1, 2, vec![255, 0, 0, 255, 0, 0, 255, 255])).unwrap();
        // BMP stores the first row last, so the bottom (blue) row comes first.
        assert_eq!(&bmp[HEADER_LEN..HEADER_LEN + 4], &[255, 0, 0, 0xFF]);
        assert_eq!(&bmp[HEADER_LEN + 4..HEADER_LEN + 8], &[0, 0, 255, 0xFF]);
    }

    #[test]
    fn alpha_is_forced_opaque() {
        let bmp = encode_bmp(&solid(2, 1, [10, 20, 30, 0])).unwrap();
        // Alpha is every 4th byte of the BGRA pixel data.
        let alphas: Vec<u8> = bmp[HEADER_LEN + 3..].iter().step_by(4).copied().collect();
        assert_eq!(alphas, vec![0xFF, 0xFF]);
        // The colour channels must survive the rewrite untouched.
        assert_eq!(&bmp[HEADER_LEN..HEADER_LEN + 3], &[30, 20, 10]);
    }

    #[test]
    fn a_short_buffer_is_none() {
        // The PNG path this replaced went through `RgbaImage::from_raw`, which
        // returned None on a length mismatch and made the OCR scan report no
        // matches. Indexing the buffer directly would panic instead, and a
        // panic on the detection thread stops the engine silently.
        assert!(encode_bmp(&frame(2, 2, vec![0u8; 8])).is_none());
        assert!(encode_bmp(&frame(4, 4, Vec::new())).is_none());
    }

    #[test]
    fn a_degenerate_frame_is_none() {
        assert!(encode_bmp(&frame(0, 4, Vec::new())).is_none());
        assert!(encode_bmp(&frame(4, 0, Vec::new())).is_none());
    }
}
