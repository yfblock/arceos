//! YUV420 planar → 24-bit BMP encoder (bottom-up, BGR, 4-byte row padding).

use axstd::vec::Vec;

const BMP_HEADER_SIZE: usize = 54;

/// Encode JPU YUV420 planar output as a 24-bit BMP file.
pub fn yuv420_to_bmp(width: u32, height: u32, yuv_data: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }

    let stride_y = ((width + 15) / 16) * 16;
    let stride_c = stride_y / 2;
    let y_plane_size = (stride_y * height) as usize;
    let c_plane_size = (stride_c * (height / 2)) as usize;
    let needed = y_plane_size + c_plane_size * 2;
    if yuv_data.len() < needed {
        return None;
    }

    let row_size = ((width * 3 + 3) / 4) * 4;
    let pixel_bytes = (row_size * height) as usize;
    let file_size = BMP_HEADER_SIZE + pixel_bytes;

    let mut out = Vec::with_capacity(file_size);
    out.resize(file_size, 0);

    // BITMAPFILEHEADER (14 bytes)
    out[0] = b'B';
    out[1] = b'M';
    write_u32_le(&mut out[2..6], file_size as u32);
    write_u32_le(&mut out[10..14], BMP_HEADER_SIZE as u32);

    // BITMAPINFOHEADER (40 bytes)
    write_u32_le(&mut out[14..18], 40);
    write_i32_le(&mut out[18..22], width as i32);
    write_i32_le(&mut out[22..26], height as i32);
    out[26] = 1; // planes
    out[27] = 0;
    out[28] = 24; // bpp
    out[29] = 0;

    let mut dst = BMP_HEADER_SIZE;
    for row in (0..height).rev() {
        for col in 0..width {
            let (r, g, b) = yuv420_to_rgb(
                yuv_data,
                row,
                col,
                stride_y,
                stride_c,
                y_plane_size,
                c_plane_size,
            );
            out[dst] = b;
            out[dst + 1] = g;
            out[dst + 2] = r;
            dst += 3;
        }
        let pad = (row_size - width * 3) as usize;
        dst += pad;
    }

    Some(out)
}

fn yuv420_to_rgb(
    yuv: &[u8],
    row: u32,
    col: u32,
    stride_y: u32,
    stride_c: u32,
    y_plane_size: usize,
    c_plane_size: usize,
) -> (u8, u8, u8) {
    let y_idx = (row * stride_y + col) as usize;
    let c_row = row / 2;
    let c_col = col / 2;
    let cb_idx = y_plane_size + (c_row * stride_c + c_col) as usize;
    let cr_idx = y_plane_size + c_plane_size + (c_row * stride_c + c_col) as usize;

    let y = yuv[y_idx] as i32;
    let u = yuv[cb_idx] as i32;
    let v = yuv[cr_idx] as i32;
    let uv_u = u - 128;
    let uv_v = v - 128;

    let r = (y + (359 * uv_v + 128) / 256).clamp(0, 255) as u8;
    let g = (y - (88 * uv_u + 183 * uv_v + 128) / 256).clamp(0, 255) as u8;
    let b = (y + (454 * uv_u + 128) / 256).clamp(0, 255) as u8;
    (r, g, b)
}

fn write_u32_le(dst: &mut [u8], val: u32) {
    dst.copy_from_slice(&val.to_le_bytes());
}

fn write_i32_le(dst: &mut [u8], val: i32) {
    dst.copy_from_slice(&val.to_le_bytes());
}
