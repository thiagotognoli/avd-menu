//! Redimensionamento de PNG e geração de .icns (ícone do macOS).

use crate::{Error, Result};

/// Imagem RGBA 8 bits.
pub struct Rgba {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

pub fn decode_png(data: &[u8]) -> Result<Rgba> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| Error::bad(e.to_string()))?;
    let mut buf = vec![0u8; reader.output_buffer_size().ok_or_else(|| Error::bad("PNG grande demais"))?];
    let info = reader.next_frame(&mut buf).map_err(|e| Error::bad(e.to_string()))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let src = &buf[..info.buffer_size()];
    let mut px = Vec::with_capacity(w * h * 4);
    match info.color_type {
        png::ColorType::Rgba => px.extend_from_slice(src),
        png::ColorType::Rgb => src.chunks(3).for_each(|c| px.extend_from_slice(&[c[0], c[1], c[2], 255])),
        png::ColorType::GrayscaleAlpha => src.chunks(2).for_each(|c| px.extend_from_slice(&[c[0], c[0], c[0], c[1]])),
        png::ColorType::Grayscale => src.iter().for_each(|&g| px.extend_from_slice(&[g, g, g, 255])),
        png::ColorType::Indexed => return Err(Error::bad("PNG indexado não suportado")),
    }
    Ok(Rgba { w, h, px })
}

pub fn encode_png(img: &Rgba) -> Result<Vec<u8>> {
    let mut out = vec![];
    {
        let mut enc = png::Encoder::new(&mut out, img.w as u32, img.h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| Error::internal(e.to_string()))?;
        w.write_image_data(&img.px).map_err(|e| Error::internal(e.to_string()))?;
    }
    Ok(out)
}

/// Reamostra para `size` x `size` (filtro de área, ponderado pelo alfa). Basta
/// para ícones e evita depender de bibliotecas de imagem.
pub fn resize(src: &Rgba, size: usize) -> Rgba {
    let (sw, sh) = (src.w, src.h);
    let (fx, fy) = (sw as f64 / size as f64, sh as f64 / size as f64);
    let mut px = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let (x0, x1) = (x as f64 * fx, (x + 1) as f64 * fx);
            let (y0, y1) = (y as f64 * fy, (y + 1) as f64 * fy);
            let (mut r, mut g, mut b, mut a, mut wsum) = (0.0, 0.0, 0.0, 0.0, 0.0);
            let mut sy = y0.floor() as usize;
            while (sy as f64) < y1.ceil() && sy < sh {
                let wy = y1.min((sy + 1) as f64) - y0.max(sy as f64);
                let mut sx = x0.floor() as usize;
                while (sx as f64) < x1.ceil() && sx < sw {
                    let wx = x1.min((sx + 1) as f64) - x0.max(sx as f64);
                    let wgt = wx * wy;
                    let i = (sy * sw + sx) * 4;
                    let pa = src.px[i + 3] as f64 * wgt;
                    r += src.px[i] as f64 * pa;
                    g += src.px[i + 1] as f64 * pa;
                    b += src.px[i + 2] as f64 * pa;
                    a += pa;
                    wsum += wgt;
                    sx += 1;
                }
                sy += 1;
            }
            if a > 0.0 && wsum > 0.0 {
                let o = (y * size + x) * 4;
                px[o] = (r / a + 0.5) as u8;
                px[o + 1] = (g / a + 0.5) as u8;
                px[o + 2] = (b / a + 0.5) as u8;
                px[o + 3] = (a / wsum + 0.5) as u8;
            }
        }
    }
    Rgba { w: size, h: size, px }
}

/// Converte um PNG em .icns com os tamanhos 16 a 1024 px.
pub fn icns(png_data: &[u8]) -> Result<Vec<u8>> {
    let src = decode_png(png_data)?;
    let types: [(&[u8; 4], usize); 7] = [(b"icp4", 16), (b"icp5", 32), (b"icp6", 64), (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024)];
    let mut body = vec![];
    for (kind, size) in types {
        let enc = encode_png(&resize(&src, size))?;
        body.extend_from_slice(kind);
        body.extend_from_slice(&((enc.len() + 8) as u32).to_be_bytes());
        body.extend_from_slice(&enc);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icns_has_all_sizes() {
        let img = Rgba { w: 64, h: 64, px: [61u8, 220, 132, 255].repeat(64 * 64) };
        let out = icns(&encode_png(&img).unwrap()).unwrap();
        assert_eq!(&out[..4], b"icns");
        assert_eq!(u32::from_be_bytes(out[4..8].try_into().unwrap()) as usize, out.len());
        for tag in ["icp4", "icp5", "icp6", "ic07", "ic08", "ic09", "ic10"] {
            assert!(out.windows(4).any(|w| w == tag.as_bytes()), "faltou {tag}");
        }
    }

    #[test]
    fn resize_keeps_alpha() {
        let mut src = Rgba { w: 4, h: 4, px: vec![0; 64] };
        src.px[0..4].copy_from_slice(&[255, 0, 0, 255]);
        let out = resize(&src, 2);
        assert_ne!(out.px[3], 0);
        assert_eq!(out.px[3 * 4 + 3], 0);
    }
}
