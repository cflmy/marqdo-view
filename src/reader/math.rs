//! Bounded TeX math to inert MathML, shared by all rendering modes.

use pulldown_latex::{config::DisplayMode, push_mathml, Parser, RenderConfig, Storage};

pub fn render(source: &str, block: bool) -> Option<String> {
    if source.len() > 4096
        || source.chars().filter(|ch| *ch == '{').count() > 128
        || source.split('\\').skip(1).any(|rest| {
            let end = rest
                .find(|ch: char| !ch.is_ascii_alphabetic())
                .unwrap_or(rest.len());
            matches!(
                &rest[..end],
                "def"
                    | "gdef"
                    | "edef"
                    | "xdef"
                    | "newcommand"
                    | "renewcommand"
                    | "providecommand"
                    | "let"
                    | "futurelet"
                    | "csname"
                    | "input"
                    | "include"
                    | "href"
                    | "url"
                    | "includegraphics"
                    | "htmlClass"
                    | "htmlStyle"
                    | "htmlId"
            )
        })
    {
        return None;
    }
    let storage = Storage::new();
    let config = RenderConfig {
        display_mode: if block {
            DisplayMode::Block
        } else {
            DisplayMode::Inline
        },
        ..Default::default()
    };
    let mut html = String::new();
    let mut parsed = true;
    let parser = Parser::new(source, &storage).inspect(|event| parsed &= event.is_ok());
    push_mathml(&mut html, parser, config).ok()?;
    if !parsed || html.len() > 64 * 1024 {
        return None;
    }
    let xml = roxmltree::Document::parse(&html).ok()?;
    if xml.root_element().tag_name().name() != "math" {
        return None;
    }
    let safe = xml
        .descendants()
        .filter(roxmltree::Node::is_element)
        .all(|node| {
            matches!(
                node.tag_name().name(),
                "math"
                    | "mrow"
                    | "mi"
                    | "mo"
                    | "mn"
                    | "mtext"
                    | "mfrac"
                    | "msqrt"
                    | "mroot"
                    | "msub"
                    | "msup"
                    | "msubsup"
                    | "munder"
                    | "mover"
                    | "munderover"
                    | "mtable"
                    | "mtr"
                    | "mtd"
                    | "mspace"
                    | "mpadded"
                    | "mstyle"
                    | "menclose"
                    | "mphantom"
                    | "mmultiscripts"
                    | "mprescripts"
                    | "none"
            ) && node.attributes().all(|attr| {
                attr.namespace().is_none()
                    && matches!(
                        attr.name(),
                        "display"
                            | "mathvariant"
                            | "mathsize"
                            | "mathcolor"
                            | "displaystyle"
                            | "scriptlevel"
                            | "stretchy"
                            | "symmetric"
                            | "movablelimits"
                            | "form"
                            | "fence"
                            | "separator"
                            | "lspace"
                            | "rspace"
                            | "minsize"
                            | "maxsize"
                            | "accent"
                            | "accentunder"
                            | "linethickness"
                            | "width"
                            | "height"
                            | "depth"
                            | "columnalign"
                            | "rowalign"
                            | "rowspacing"
                            | "columnspacing"
                            | "notation"
                    )
            })
        });
    safe.then_some(html)
}
