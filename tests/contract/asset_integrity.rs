//! Asset provenance and integrity: the three reused legacy visual assets are
//! pinned by SHA-256 in `assets/licenses/ASSETS.md` and embedded by
//! `src/assets.rs` (ADR-0002).

use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut out = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[test]
fn reused_assets_match_their_recorded_hashes() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (file, expected) in [
        (
            "assets/logo.svg",
            "52b2338468c62a7da86178ecec86cbb7ea44a996cd1bc4275ebb56f16f2244fe",
        ),
        (
            "assets/blossom.webp",
            "901fd6b37ebb20d1dfc3bf4d265a063308605350fd317e41914fa35b06f46e00",
        ),
        (
            "assets/feather.webp",
            "fbd787174be1e8027ae4661b99f7f2cc01b824d8fc875a1d2d39cbd0c052e5de",
        ),
    ] {
        let bytes =
            std::fs::read(root.join(file)).unwrap_or_else(|err| panic!("read {file}: {err}"));
        assert_eq!(sha256_hex(&bytes), expected, "{file} digest changed");
    }
    let licenses = std::fs::read_to_string(root.join("assets/licenses/ASSETS.md")).unwrap();
    for expected in [
        "52b2338468c62a7da86178ecec86cbb7ea44a996cd1bc4275ebb56f16f2244fe",
        "901fd6b37ebb20d1dfc3bf4d265a063308605350fd317e41914fa35b06f46e00",
        "fbd787174be1e8027ae4661b99f7f2cc01b824d8fc875a1d2d39cbd0c052e5de",
        "Apache-2.0",
    ] {
        assert!(licenses.contains(expected), "ASSETS.md misses {expected}");
    }
}

#[test]
fn embedded_asset_bundle_matches_disk_and_has_one_theme() {
    let bundle = marqdo_view::reader::assets::AssetBundle::get();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let css = std::fs::read(root.join("assets/reader.css")).unwrap();
    assert_eq!(sha256_hex(&css), bundle.css().sha256);
    let js = std::fs::read(root.join("assets/reader.js")).unwrap();
    assert_eq!(sha256_hex(&js), bundle.js().sha256);
    assert_eq!(
        bundle
            .all()
            .iter()
            .filter(|asset| asset.extension == "css" || asset.extension == "js")
            .count(),
        2
    );
}

#[test]
fn theme_has_one_token_layer_and_no_legacy_skin() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let css = std::fs::read_to_string(root.join("assets/reader.css")).unwrap();
    assert_eq!(css.matches("@layer tokens").count(), 1);
    assert!(css.contains(
        "@layer reset, tokens, layout, typography, semantics, overlays, interactions, responsive;"
    ));
    for banned in ["notebook-page", "reader-page", "legacy.css", "new.css"] {
        assert!(!css.contains(banned), "legacy skin marker {banned} in CSS");
    }
    assert!(
        !css.contains("!important"),
        "the theme must not rely on !important overrides"
    );
    // No remote font/CDN requests.
    for banned in ["fonts.googleapis", "https://"] {
        assert!(!css.contains(banned), "remote resource marker {banned}");
    }
}
