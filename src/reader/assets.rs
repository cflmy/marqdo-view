//! The single embedded asset bundle.
//!
//! Live serving and static export both read these exact bytes (ADR-0002). Every
//! asset carries its SHA-256; browser URLs use a hash prefix so a changed asset
//! can never be confused with a cached older one.

use crate::reader::source::sha256_hex;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone)]
pub struct Asset {
    pub name: &'static str,
    pub extension: &'static str,
    pub mime: &'static str,
    pub bytes: &'static [u8],
    /// Lowercase hex SHA-256 of `bytes`.
    pub sha256: String,
}

impl Asset {
    /// Hashed file name, e.g. `reader.1a2b3c4d5e6f7788.css`.
    pub fn hashed_name(&self) -> String {
        format!("{}.{}.{}", self.name, &self.sha256[..16], self.extension)
    }

    pub fn data_uri(&self) -> String {
        format!("data:{};base64,{}", self.mime, base64(self.bytes))
    }
}

pub struct AssetBundle {
    assets: Vec<Asset>,
    by_hashed_name: HashMap<String, usize>,
    /// CSS with the two image placeholders replaced by data URIs, so there is
    /// still exactly one stylesheet and no extra network request.
    css_with_images: String,
    css_served_sha256: String,
}

impl AssetBundle {
    fn new() -> Self {
        let assets = vec![
            asset(
                "reader",
                "css",
                "text/css; charset=utf-8",
                include_bytes!("../../assets/reader.css"),
            ),
            asset(
                "reader",
                "js",
                "text/javascript; charset=utf-8",
                include_bytes!("../../assets/reader.js"),
            ),
            asset(
                "logo",
                "svg",
                "image/svg+xml",
                include_bytes!("../../assets/logo.svg"),
            ),
            asset(
                "blossom",
                "webp",
                "image/webp",
                include_bytes!("../../assets/blossom.webp"),
            ),
            asset(
                "feather",
                "webp",
                "image/webp",
                include_bytes!("../../assets/feather.webp"),
            ),
        ];
        let mut by_hashed_name = HashMap::new();
        for (index, asset) in assets.iter().enumerate() {
            by_hashed_name.insert(asset.hashed_name(), index);
        }
        let css_text = std::str::from_utf8(assets[0].bytes).expect("reader.css is UTF-8");
        let css_with_images = css_text
            .replace("__BLOSSOM__", &assets[3].data_uri())
            .replace("__FEATHER__", &assets[4].data_uri());
        let css_served_sha256 = sha256_hex(css_with_images.as_bytes());
        Self {
            assets,
            by_hashed_name,
            css_with_images,
            css_served_sha256,
        }
    }

    pub fn get() -> &'static AssetBundle {
        static BUNDLE: OnceLock<AssetBundle> = OnceLock::new();
        BUNDLE.get_or_init(AssetBundle::new)
    }

    pub fn all(&self) -> &[Asset] {
        &self.assets
    }

    pub fn css(&self) -> &Asset {
        &self.assets[0]
    }

    pub fn js(&self) -> &Asset {
        &self.assets[1]
    }

    pub fn logo(&self) -> &Asset {
        &self.assets[2]
    }

    pub fn by_path(&self, hashed_name: &str) -> Option<&Asset> {
        self.by_hashed_name
            .get(hashed_name)
            .map(|index| &self.assets[*index])
    }

    pub fn css_bytes(&self) -> &[u8] {
        self.css_with_images.as_bytes()
    }

    /// SHA-256 of the served CSS (after image substitution).
    pub fn css_served_sha256(&self) -> &str {
        &self.css_served_sha256
    }

    pub fn hashed_css_name(&self) -> String {
        format!("reader.{}.css", &self.css_served_sha256[..16])
    }

    pub fn hashed_js_name(&self) -> String {
        self.js().hashed_name()
    }
}

fn asset(
    name: &'static str,
    extension: &'static str,
    mime: &'static str,
    bytes: &'static [u8],
) -> Asset {
    Asset {
        name,
        extension,
        mime,
        bytes,
        sha256: sha256_hex(bytes),
    }
}

/// Minimal standard base64 (RFC 4648) encoder for data URIs.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) as usize & 0x3f] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_digests_match_source_files() {
        let bundle = AssetBundle::get();
        assert_eq!(
            bundle.css().sha256,
            sha256_hex(include_bytes!("../../assets/reader.css"))
        );
        assert_eq!(
            bundle.js().sha256,
            sha256_hex(include_bytes!("../../assets/reader.js"))
        );
        assert_eq!(
            bundle.logo().sha256,
            sha256_hex(include_bytes!("../../assets/logo.svg"))
        );
        assert_eq!(bundle.assets[3].sha256.len(), 64);
        assert!(bundle.by_path(&bundle.logo().hashed_name()).is_some());
        assert!(bundle.by_path("missing.css").is_none());
        // Exactly one theme and one script entry.
        let css_count = bundle
            .all()
            .iter()
            .filter(|asset| asset.extension == "css")
            .count();
        let js_count = bundle
            .all()
            .iter()
            .filter(|asset| asset.extension == "js")
            .count();
        assert_eq!(css_count, 1);
        assert_eq!(js_count, 1);
    }

    #[test]
    fn css_substitutes_images_and_keeps_one_asset() {
        let bundle = AssetBundle::get();
        let css = std::str::from_utf8(bundle.css_bytes()).unwrap();
        assert!(!css.contains("__BLOSSOM__"));
        assert!(!css.contains("__FEATHER__"));
        assert!(css.contains("data:image/webp;base64,"));
        assert!(bundle.hashed_css_name().starts_with("reader."));
        assert_eq!(
            bundle.hashed_css_name().len(),
            "reader.".len() + 16 + ".css".len()
        );
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
