//! Renders the program icon (SVG) into all needed sizes and formats:
//! PNG files, a Windows .ico and a macOS .icns (both with PNG entries).
//!
//! Usage: `cargo run --release -- <icon.svg> <small-icon.svg> <out-dir>`
//! The small variant is used for 16 to 32 pixels, where details get lost.

use std::fs;
use std::path::{Path, PathBuf};

use resvg::{tiny_skia, usvg};

fn render(svg: &[u8], size: u32) -> Vec<u8> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).expect("valid SVG");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let scale = size as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().expect("png")
}

/// .ico with PNG entries (allowed since Windows Vista).
fn ico(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len() as u32;
    for (size, png) in entries {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in entries {
        out.extend_from_slice(png);
    }
    out
}

/// .icns with PNG entries; `entries` are (type code, png).
fn icns(entries: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (code, png) in entries {
        body.extend_from_slice(*code);
        body.extend_from_slice(&(png.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(png);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [big, small, out] = args.as_slice() else {
        eprintln!("usage: make-icons <icon.svg> <small-icon.svg> <out-dir>");
        std::process::exit(2);
    };
    let big = fs::read(big).expect("read icon");
    let small = fs::read(small).expect("read small icon");
    fs::create_dir_all(out).expect("create out dir");
    let png = |size: u32| render(if size <= 32 { &small } else { &big }, size);
    let write = |name: &str, data: &[u8]| fs::write(Path::new(out).join(name), data).expect("write");

    for size in [16, 24, 32, 48, 64, 128, 256, 512, 1024] {
        write(&format!("mrsizeme-{size}.png"), &png(size));
    }
    let ico_entries: Vec<_> = [16, 24, 32, 48, 64, 128, 256].iter().map(|&s| (s, png(s))).collect();
    write("mrsizeme.ico", &ico(&ico_entries));
    let icns_entries = [
        (b"icp4", png(16)),
        (b"icp5", png(32)),
        (b"ic11", png(32)),  // 16@2x
        (b"ic07", png(128)),
        (b"ic12", png(64)),  // 32@2x
        (b"ic08", png(256)),
        (b"ic13", png(256)), // 128@2x
        (b"ic09", png(512)),
        (b"ic14", png(512)), // 256@2x
        (b"ic10", png(1024)), // 512@2x
    ];
    write("mrsizeme.icns", &icns(&icns_entries));
    println!("icons written to {}", out.display());
}
