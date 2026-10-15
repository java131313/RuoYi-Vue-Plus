//! Math captcha with a hand-rolled PNG encoder (no image dependencies).
//! plus-ui prefixes the returned base64 with `data:image/gif;base64,`;
//! browsers sniff image content for <img> data URLs, so real PNG bytes
//! render fine.

use rand::Rng;

const W: usize = 160;
const H: usize = 60;
const SCALE: usize = 5;

/// Returns (expression_text, answer).
pub fn gen_math() -> (String, String) {
    let mut rng = rand::thread_rng();
    let a: i32 = rng.gen_range(0..10);
    let b: i32 = rng.gen_range(0..10);
    let op = *["+", "-", "x"][rng.gen_range(0..3)].as_bytes().first().unwrap() as char;
    let (text, ans) = match op {
        '+' => (format!("{a}+{b}=?"), a + b),
        '-' => {
            if a >= b {
                (format!("{a}-{b}=?"), a - b)
            } else {
                (format!("{b}-{a}=?"), b - a)
            }
        }
        _ => (format!("{a}x{b}=?"), a * b),
    };
    (text, ans.to_string())
}

struct Canvas {
    px: Vec<u8>,
}

impl Canvas {
    fn new() -> Self {
        let mut c = Canvas { px: vec![0u8; W * H * 3] };
        let mut rng = rand::thread_rng();
        for i in 0..W * H {
            let v = rng.gen_range(244..=255) as u8;
            c.px[i * 3] = v;
            c.px[i * 3 + 1] = v;
            c.px[i * 3 + 2] = v - rng.gen_range(0..8);
        }
        c
    }

    fn set(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8) {
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            let i = (y as usize * W + x as usize) * 3;
            self.px[i] = r;
            self.px[i + 1] = g;
            self.px[i + 2] = b;
        }
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, r: u8, g: u8, b: u8) {
        let (mut x0, mut y0, x1, y1) = (x0, y0, x1, y1);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set(x0, y0, r, g, b);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    fn glyph(&mut self, ch: char, ox: i32, oy: i32, r: u8, g: u8, b: u8) {
        let glyphs = glyphs();
        let Some(rows) = glyphs.get(&ch) else { return };
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits >> (4 - col) & 1 == 1 {
                    for sy in 0..SCALE {
                        for sx in 0..SCALE {
                            self.set(ox + (col * SCALE + sx) as i32, oy + (row * SCALE + sy) as i32, r, g, b);
                        }
                    }
                }
            }
        }
    }
}

/// 5x7 bitmap font, bit 4 = leftmost column. Built per call — 15 entries,
/// negligible next to PNG encoding.
fn glyphs() -> std::collections::HashMap<char, [u8; 7]> {
    [
        ('0', [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E]),
        ('1', [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E]),
        ('2', [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F]),
        ('3', [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E]),
        ('4', [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02]),
        ('5', [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E]),
        ('6', [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E]),
        ('7', [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08]),
        ('8', [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E]),
        ('9', [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C]),
        ('+', [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00]),
        ('-', [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00]),
        ('x', [0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x00]),
        ('=', [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00]),
        ('?', [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04]),
    ]
    .into_iter()
    .collect()
}

/// Render the expression to a PNG, returned as base64.
pub fn render_png_base64(text: &str) -> String {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    let mut c = Canvas::new();
    let mut rng = rand::thread_rng();
    // noise lines
    for _ in 0..5 {
        let r = rng.gen_range(120..200) as u8;
        let g = rng.gen_range(120..200) as u8;
        let b = rng.gen_range(120..200) as u8;
        c.line(
            rng.gen_range(0..W as i32),
            rng.gen_range(0..H as i32),
            rng.gen_range(0..W as i32),
            rng.gen_range(0..H as i32),
            r,
            g,
            b,
        );
    }
    // glyphs with jitter
    let chars: Vec<char> = text.chars().collect();
    let cell = (W - 20) / chars.len().max(1);
    for (i, ch) in chars.iter().enumerate() {
        let ox = 10i32 + (i * cell) as i32 + rng.gen_range(-3..4);
        let oy = 12 + rng.gen_range(-4..5);
        let r = rng.gen_range(20..80) as u8;
        let g = rng.gen_range(40..100) as u8;
        let b = rng.gen_range(60..140) as u8;
        c.glyph(*ch, ox as i32, oy as i32, r, g, b);
    }
    // noise dots
    for _ in 0..40 {
        let x = rng.gen_range(0..W as i32);
        let y = rng.gen_range(0..H as i32);
        let v = rng.gen_range(100..200) as u8;
        c.set(x, y, v, v, v);
    }
    B64.encode(png_encode(W as u32, H as u32, &c.px))
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB88320 } else { crc >> 1 };
        }
    }
    crc ^ 0xFFFFFFFF
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

/// Minimal PNG encoder: 8-bit RGB, zlib stream made of stored (uncompressed)
/// DEFLATE blocks. Valid output, ~65KB per image, plenty fast for captchas.
fn png_encode(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let stride = w as usize * 3;
    let mut raw = Vec::with_capacity((stride + 1) * h as usize);
    for row in 0..h as usize {
        raw.push(0); // filter: None
        raw.extend_from_slice(&rgb[row * stride..(row + 1) * stride]);
    }
    // zlib: 78 01 + stored blocks + adler32
    let mut zlib = Vec::with_capacity(raw.len() + 64);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        zlib.push(if last { 1 } else { 0 });
        let len = chunk.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(chunk);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |typ: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(typ);
        out.extend_from_slice(data);
        let mut crc_in = Vec::with_capacity(4 + data.len());
        crc_in.extend_from_slice(typ);
        crc_in.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_in).to_be_bytes());
    };
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolor
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn png_has_valid_signature_and_chunks() {
        let (text, ans) = gen_math();
        assert!(!ans.is_empty());
        let png = render_png_base64(&text);
        let bytes = base64::engine::general_purpose::STANDARD.decode(png).unwrap();
        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n");
        // IEND present
        let tail = &bytes[bytes.len() - 12..];
        assert_eq!(&tail[4..8], b"IEND");
    }
}
