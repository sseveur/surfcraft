//! `--screenshot <file.png>`: render a few frames, save the window as PNG, exit. For docs and smoke tests.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;

type Shot = Arc<Mutex<Option<Arc<egui::ColorImage>>>>;

pub struct Screenshot {
    pub path: PathBuf,
    frames: u32,
    requested: bool,
    image: Shot,
}

impl Screenshot {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            frames: 0,
            requested: false,
            image: Shot::default(),
        }
    }

    /// Call once per frame. Returns true when done (the app should close).
    pub fn tick(&mut self, ctx: &egui::Context) -> bool {
        self.frames += 1;
        ctx.request_repaint();
        // Let the background worker finish and layout settle.
        if self.frames > 30 && !self.requested {
            let slot = self.image.clone();
            let repaint = ctx.clone();
            let cb = egui::ScreenshotCallback::new(move |img| {
                *slot.lock().unwrap() = Some(img);
                repaint.request_repaint();
            });
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(cb));
            self.requested = true;
        }
        let image = self.image.lock().unwrap().take();
        if let Some(img) = image {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Err(e) = write_png(&self.path, img.size[0] as u32, img.size[1] as u32, &rgba) {
                eprintln!("screenshot failed: {e}");
            }
            return true;
        }
        self.frames > 600
    }
}

/// Minimal PNG writer (RGBA8, uncompressed deflate blocks).
fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for row in rgba.chunks(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(c) = chunks.next() {
        zlib.push(if chunks.peek().is_none() { 1 } else { 0 });
        let len = c.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = std::fs::File::create(path)?;
    out.write_all(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])?;
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr)?;
    chunk(&mut out, b"IDAT", &zlib)?;
    chunk(&mut out, b"IEND", &[])
}

fn chunk(out: &mut impl Write, kind: &[u8; 4], data: &[u8]) -> std::io::Result<()> {
    out.write_all(&(data.len() as u32).to_be_bytes())?;
    out.write_all(kind)?;
    out.write_all(data)?;
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in kind.iter().chain(data) {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    out.write_all(&(!crc).to_be_bytes())
}
