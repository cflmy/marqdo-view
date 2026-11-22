# Third-party and reused visual assets

The three visual assets in `assets/` were selected from the legacy `cflmy/marqdo`
repository at commit `1c55baabd570e18b9da8f9057cef2f848e176690`, which is licensed
under the Apache License 2.0. They are reused as static decorative artwork only;
no legacy CSS/JS/code is copied. Source hashes were verified before copying
(see `docs/audits/legacy-render-paths.md` §6).

| File | Legacy path | SHA-256 | License |
|---|---|---|---|
| `assets/logo.svg` | `src/view/assets/logo.svg` | `52b2338468c62a7da86178ecec86cbb7ea44a996cd1bc4275ebb56f16f2244fe` | Apache-2.0 (legacy marqdo) |
| `assets/blossom.webp` | `src/view/assets/blossom.webp` | `901fd6b37ebb20d1dfc3bf4d265a063308605350fd317e41914fa35b06f46e00` | Apache-2.0 (legacy marqdo) |
| `assets/feather.webp` | `src/view/assets/feather.webp` | `fbd787174be1e8027ae4661b99f7f2cc01b824d8fc875a1d2d39cbd0c052e5de` | Apache-2.0 (legacy marqdo) |

The legacy `assets/README.md` states the blossom and feather artwork was generated
for the reader redesign (original transparent artwork), and the logo is a
project-native geometric M/book mark. No KaTeX or other vendored third-party
assets are carried into this repository.

Bundling: `src/assets.rs` embeds these files with SHA-256 digests; Live and
Export serve the exact same bytes (ADR-0002). Replacing an asset requires
updating this file and the hashes asserted by `tests/rendering/asset_tests.rs`.
