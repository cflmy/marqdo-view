//! Bounded project-local assets used by the reader and static export.

use crate::reader::links::relative_url;
use crate::reader::markdown::{DocBlock, Inline};
use crate::reader::source::{sha256_hex, Project, SafeRelativePath};
use image::{ImageFormat, ImageReader};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Cursor, Read};
use std::sync::Arc;

const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_REFERENCES: usize = 128;
const MAX_AXIS: u32 = 8192;
const MAX_PIXELS: u64 = 16_000_000;

#[derive(Debug)]
pub struct Media {
    pub path: SafeRelativePath,
    pub bytes: Vec<u8>,
    pub digest: String,
    pub mime: &'static str,
    pub extension: &'static str,
    pub width: u32,
    pub height: u32,
}

impl Media {
    pub fn filename(&self) -> String {
        format!("{}.{}", self.digest, self.extension)
    }
}

pub fn collect(
    project: &Project,
    current: &SafeRelativePath,
    blocks: &[DocBlock],
) -> BTreeMap<String, Arc<Media>> {
    let mut urls = BTreeSet::new();
    scan_blocks(blocks, &mut urls);
    let mut total = 0;
    let mut resolved = BTreeMap::<SafeRelativePath, Arc<Media>>::new();
    let mut result = BTreeMap::new();
    for url in urls {
        let Some(path) = relative_url(current, url) else {
            continue;
        };
        if let Some(media) = resolved.get(&path) {
            result.insert(url.to_string(), Arc::clone(media));
            continue;
        }
        if let Some(media) = load(project, &path) {
            total += media.bytes.len();
            if total > MAX_DOCUMENT_BYTES {
                break;
            }
            let media = Arc::new(media);
            resolved.insert(path, Arc::clone(&media));
            result.insert(url.to_string(), media);
        }
    }
    result
}

fn scan_blocks<'a>(blocks: &'a [DocBlock], urls: &mut BTreeSet<&'a str>) {
    for block in blocks {
        match block {
            DocBlock::Heading { content, .. } | DocBlock::Paragraph { content, .. } => {
                scan_inlines(content, urls)
            }
            DocBlock::List { items, .. } => scan_blocks(items, urls),
            DocBlock::ListItem { children, .. } | DocBlock::BlockQuote { children, .. } => {
                scan_blocks(children, urls)
            }
            DocBlock::Table { header, rows, .. } => {
                for cell in header.iter().chain(rows.iter().flatten()) {
                    scan_inlines(cell, urls);
                }
            }
            _ => {}
        }
    }
}

fn scan_inlines<'a>(inlines: &'a [Inline], urls: &mut BTreeSet<&'a str>) {
    for inline in inlines {
        if urls.len() >= MAX_REFERENCES {
            return;
        }
        match inline {
            Inline::Image { url, .. } => {
                urls.insert(url);
            }
            Inline::Emph { children, .. }
            | Inline::Strong { children, .. }
            | Inline::Strikethrough { children, .. }
            | Inline::Link { children, .. } => scan_inlines(children, urls),
            _ => {}
        }
    }
}

pub fn load(project: &Project, path: &SafeRelativePath) -> Option<Media> {
    let extension = path.file_name().rsplit_once('.')?.1.to_ascii_lowercase();
    let (mime, extension, format) = match extension.as_str() {
        "png" => ("image/png", "png", Some(ImageFormat::Png)),
        "jpeg" | "jpg" => ("image/jpeg", "jpg", Some(ImageFormat::Jpeg)),
        "gif" => ("image/gif", "gif", Some(ImageFormat::Gif)),
        "webp" => ("image/webp", "webp", Some(ImageFormat::WebP)),
        "svg" => ("image/svg+xml", "svg", None),
        _ => return None,
    };
    let file = open_beneath(project, path)?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let (width, height) = if let Some(format) = format {
        if image::guess_format(&bytes).ok()? != format {
            return None;
        }
        let dimensions = ImageReader::with_format(Cursor::new(&bytes), format)
            .into_dimensions()
            .ok()?;
        if !valid_dimensions(dimensions.0, dimensions.1) {
            return None;
        }
        let mut decoder = ImageReader::with_format(Cursor::new(&bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_AXIS);
        limits.max_image_height = Some(MAX_AXIS);
        limits.max_alloc = Some(128 * 1024 * 1024);
        decoder.limits(limits);
        decoder.decode().ok()?;
        dimensions
    } else {
        svg_dimensions(&bytes)?
    };
    if !valid_dimensions(width, height) {
        return None;
    }
    Some(Media {
        path: path.clone(),
        digest: sha256_hex(&bytes),
        bytes,
        mime,
        extension,
        width,
        height,
    })
}

fn valid_dimensions(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_AXIS
        && height <= MAX_AXIS
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

#[cfg(target_os = "linux")]
fn open_beneath(project: &Project, path: &SafeRelativePath) -> Option<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let root = project.root_handle();
    let name = std::ffi::CString::new(path.as_str()).ok()?;
    // SAFETY: open_how is a C struct of integer fields.
    let mut how: libc::open_how = unsafe { std::mem::zeroed() };
    how.flags = (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK) as u64;
    how.resolve = libc::RESOLVE_BENEATH | libc::RESOLVE_NO_MAGICLINKS;
    // SAFETY: the descriptor and both pointers stay valid for openat2.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            root.as_raw_fd(),
            name.as_ptr(),
            &how as *const libc::open_how,
            std::mem::size_of_val(&how),
        )
    };
    if fd < 0 {
        return None;
    }
    // SAFETY: openat2 returned a new owned descriptor.
    Some(unsafe { File::from_raw_fd(fd as std::os::fd::RawFd) })
}

#[cfg(not(target_os = "linux"))]
fn open_beneath(_project: &Project, _path: &SafeRelativePath) -> Option<File> {
    None
}

fn svg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SVG: &str = "http://www.w3.org/2000/svg";
    let xml = std::str::from_utf8(bytes).ok()?;
    let options = roxmltree::ParsingOptions {
        nodes_limit: 10_000,
        ..Default::default()
    };
    let tree = roxmltree::Document::parse_with_options(xml, options).ok()?;
    if tree.descendants().any(|node| node.is_pi()) {
        return None;
    }
    let root = tree.root_element();
    if !root.has_tag_name((SVG, "svg")) {
        return None;
    }
    for node in tree.descendants().filter(roxmltree::Node::is_element) {
        if node.tag_name().namespace() != Some(SVG)
            || !matches!(
                node.tag_name().name(),
                "svg"
                    | "g"
                    | "path"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "text"
                    | "tspan"
                    | "title"
                    | "desc"
            )
        {
            return None;
        }
        for attr in node.attributes() {
            if attr.namespace().is_some()
                || !matches!(
                    attr.name(),
                    "width"
                        | "height"
                        | "viewBox"
                        | "preserveAspectRatio"
                        | "x"
                        | "y"
                        | "x1"
                        | "x2"
                        | "y1"
                        | "y2"
                        | "rx"
                        | "ry"
                        | "r"
                        | "cx"
                        | "cy"
                        | "d"
                        | "points"
                        | "transform"
                        | "fill"
                        | "fill-rule"
                        | "fill-opacity"
                        | "stroke"
                        | "stroke-width"
                        | "stroke-opacity"
                        | "stroke-linecap"
                        | "stroke-linejoin"
                        | "stroke-dasharray"
                        | "opacity"
                        | "font-size"
                        | "text-anchor"
                        | "dominant-baseline"
                )
                || (matches!(attr.name(), "fill" | "stroke")
                    && !attr
                        .value()
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '#'))
            {
                return None;
            }
        }
    }
    let dimension = |key| {
        root.attribute(key)?
            .trim_end_matches("px")
            .parse::<u32>()
            .ok()
    };
    if let (Some(width), Some(height)) = (dimension("width"), dimension("height")) {
        return Some((width, height));
    }
    let viewbox: Vec<f64> = root
        .attribute("viewBox")?
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if viewbox.len() != 4 || !viewbox.iter().all(|value| value.is_finite()) {
        return None;
    }
    Some((viewbox[2].ceil() as u32, viewbox[3].ceil() as u32))
}
