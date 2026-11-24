// Rendering appends explicit newlines to keep the generated HTML readable;
// the `write_with_newline` style lint is therefore not useful here.
#![allow(clippy::write_with_newline)]
#![allow(clippy::type_complexity)]

//! The one ReaderRenderer.
//!
//! Live and Export both call `render()`; the only difference is `LinkPolicy`
//! and whether interactive controls are available (RFC-VIEW-0001 §14.3).
//! Markdown is serialized here from the View-owned `DocBlock` tree, with raw
//! HTML omitted by default and every URL scheme sanitized.

use crate::protocol::analysis_v2::{source_id, UnitKind};
use crate::reader::assets::AssetBundle;
use crate::reader::markdown::{DocBlock, Inline, TableAlign};
use crate::reader::model::{anchor_for_span, PresentationModel};
use crate::reader::source::SafeRelativePath;
use crate::reader::source::SourceSpan;
use std::fmt::Write as _;

/// How links to other documents are resolved. This is the only Live/Export
/// difference in rendered content.
#[derive(Debug, Clone)]
pub enum LinkPolicy {
    /// `GET /file?path=<relative>` on the local server.
    LocalServer,
    /// Relative links from a static page at `page_depth` directories below the
    /// site root (`0` for `index.html`).
    StaticRelative { page_depth: usize },
}

impl LinkPolicy {
    pub fn document_href(&self, current: &SafeRelativePath, target: &SafeRelativePath) -> String {
        match self {
            LinkPolicy::LocalServer => {
                format!("/file?path={}", percent_encode_path(target.as_str()))
            }
            LinkPolicy::StaticRelative { page_depth } => {
                let _ = current;
                let mut prefix = String::new();
                for _ in 0..*page_depth {
                    prefix.push_str("../");
                }
                format!(
                    "{prefix}pages/{}.html",
                    percent_encode_path(target.as_str())
                )
            }
        }
    }

    fn media_href(&self, media: &super::media::Media) -> String {
        match self {
            LinkPolicy::LocalServer => format!(
                "/media/{}?path={}",
                media.filename(),
                percent_encode_path(media.path.as_str())
            ),
            LinkPolicy::StaticRelative { page_depth } => {
                format!("{}media/{}", "../".repeat(*page_depth), media.filename())
            }
        }
    }

    fn asset_prefix(&self, page_depth: usize) -> String {
        match self {
            LinkPolicy::LocalServer => "/assets/".to_string(),
            LinkPolicy::StaticRelative { .. } => {
                let mut prefix = String::new();
                for _ in 0..page_depth {
                    prefix.push_str("../");
                }
                format!("{prefix}assets/")
            }
        }
    }
}

/// Everything the renderer needs beyond the model.
#[derive(Debug, Clone)]
pub struct RenderContext {
    pub links: LinkPolicy,
    pub live: bool,
    pub csrf_token: Option<String>,
    /// Whether a Run control may be shown as available. False until the
    /// explicit-run flow exists; the button then explains that it is disabled.
    pub run_available: bool,
    /// Static exports may hide semantic/run annotations without changing prose.
    pub annotations: bool,
    /// View wall-clock limit shown in the explicit run form (seconds).
    pub run_timeout_secs: u64,
    /// Whether a frozen Core was negotiated (an explicit run form can exist).
    pub core_available: bool,
}

impl RenderContext {
    pub fn live() -> Self {
        Self {
            links: LinkPolicy::LocalServer,
            live: true,
            csrf_token: None,
            run_available: false,
            annotations: true,
            run_timeout_secs: 30,
            core_available: false,
        }
    }

    pub fn static_site(page_depth: usize) -> Self {
        Self {
            links: LinkPolicy::StaticRelative { page_depth },
            live: false,
            csrf_token: None,
            run_available: false,
            annotations: true,
            run_timeout_secs: 0,
            core_available: false,
        }
    }

    /// Static export with annotations optionally disabled.
    pub fn static_export(page_depth: usize, annotations: bool) -> Self {
        let mut context = Self::static_site(page_depth);
        context.annotations = annotations;
        context
    }
}

pub struct RenderedPage {
    pub html: String,
    /// Asset hashed names actually referenced by the page.
    pub assets_used: Vec<String>,
}

pub struct ReaderRenderer {
    assets: &'static AssetBundle,
}

impl ReaderRenderer {
    pub fn new() -> Self {
        Self {
            assets: AssetBundle::get(),
        }
    }

    pub fn render(&self, model: &PresentationModel, ctx: &RenderContext) -> RenderedPage {
        let asset_prefix = ctx.links.asset_prefix(match ctx.links {
            LinkPolicy::StaticRelative { page_depth } => page_depth,
            LinkPolicy::LocalServer => 0,
        });
        let css_name = self.assets.hashed_css_name();
        let js_name = self.assets.hashed_js_name();
        let logo_uri = self.assets.logo().data_uri();

        let navigation = &model.navigation;
        let document_href = format!(
            "{prefix}reader.{hash}.css",
            prefix = asset_prefix,
            hash = &self.assets.css_served_sha256()[..16]
        );
        let _ = document_href;
        let mut html = String::with_capacity(16 * 1024 + model.document.source.len());
        let relative_path = model.document.identity.relative_path.clone();
        let analysis_digest = model
            .semantic
            .as_ref()
            .map(|semantic| semantic.analysis_digest.clone())
            .unwrap_or_default();
        let has_main = model.has_entry_main();
        let page_title = if navigation.site_title.is_empty() {
            model.title.clone()
        } else {
            format!("{} · {}", model.title, navigation.site_title)
        };
        let _ = write!(
            html,
            r##"<!DOCTYPE html>
<html lang="zh-CN" data-view-version="{presentation_version}">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<meta name="generator" content="marqdo-view {view_version}"/>
<meta name="referrer" content="no-referrer"/>
<meta http-equiv="Content-Security-Policy" content="{csp}"/>
<title>{title}</title>
<link rel="icon" type="image/svg+xml" href="{logo}"/>
<link rel="stylesheet" href="{css}"/>
<script defer src="{js}"></script>
</head>
<body class="mq-reader" data-mode="document" data-theme="light" data-annotations="{annotations}" data-size="normal" data-source-status="{status}" data-run-state="{run_state}" data-run-available="{run_available}" data-has-main="{has_main}" data-core-available="{core_available}" data-document="{document}" data-analysis-digest="{analysis_digest}" data-run-timeout="{run_timeout}" data-live="{live}">
<a class="mq-skip" href="#mq-main">跳到正文</a>
"##,
            presentation_version = model.version,
            view_version = env!("CARGO_PKG_VERSION"),
            csp = csp_policy(),
            title = escape_text(&page_title),
            logo = escape_attr(&logo_uri),
            css = escape_attr(&format!("{asset_prefix}{css_name}")),
            js = escape_attr(&format!("{asset_prefix}{js_name}")),
            status = model.source_status.data_value(),
            annotations = if ctx.annotations { "on" } else { "off" },
            run_state = model.run_state().data_value(),
            run_available = if ctx.run_available { "true" } else { "false" },
            has_main = if has_main { "true" } else { "false" },
            core_available = if ctx.core_available { "true" } else { "false" },
            document = escape_attr(&relative_path),
            analysis_digest = escape_attr(&analysis_digest),
            run_timeout = ctx.run_timeout_secs,
            live = if ctx.live { "true" } else { "false" },
        );

        self.render_topbar(&mut html, model, ctx, navigation);
        self.render_ornament(&mut html);
        let _ = write!(html, "<main class=\"mq-main\" id=\"mq-main\">\n");

        if navigation.welcome {
            self.render_welcome(&mut html, navigation);
        }
        self.render_document_meta(&mut html, model);
        self.render_diagnostics(&mut html, model);
        self.render_functions(&mut html, model, ctx);
        self.render_document(&mut html, model, ctx);
        if ctx.live {
            self.render_live_run_shell(&mut html, ctx);
        } else {
            self.render_run_panel(&mut html, model, ctx);
        }
        self.render_source_pane(&mut html, model, ctx);
        self.render_footer(&mut html, model);
        let _ = write!(html, "</main>\n");

        self.render_dock(&mut html, ctx);
        self.render_toast(&mut html);
        self.render_dialogs(&mut html, model, ctx);
        html.push_str("</body>\n</html>\n");

        RenderedPage {
            html,
            assets_used: vec![css_name, js_name],
        }
    }

    fn render_topbar(
        &self,
        html: &mut String,
        model: &PresentationModel,
        ctx: &RenderContext,
        navigation: &crate::reader::model::Navigation,
    ) {
        let active = model
            .navigation
            .active
            .as_ref()
            .map(|path| path.as_str())
            .unwrap_or("");
        let files_count = navigation.documents.len();
        let run_tab = if ctx.live {
            "<button type=\"button\" class=\"mq-tab\" data-action=\"run\" title=\"显式运行\">运行</button>".to_string()
        } else {
            String::new()
        };
        let _ = write!(
            html,
            r##"<header class="mq-topbar">
  <div class="mq-topbar-start">
    <button type="button" class="mq-icon-btn" data-action="more" aria-label="打开文件与阅读菜单" title="文件与更多操作">{menu_icon}</button>
  </div>
  <a class="mq-brand" href="#mq-main" aria-label="{site_title}，返回顶部">
    <img class="mq-brand-logo" src="{logo}" width="44" height="44" alt=""/>
    <span class="mq-brand-word">{site_title}<small class="mq-brand-sub">EXECUTABLE KNOWLEDGE</small></span>
  </a>
  <div class="mq-topbar-end">
    <button type="button" class="mq-icon-btn" data-action="search" aria-label="在当前文档中搜索" title="搜索（Ctrl/Cmd+K）">{search_icon}</button>
  </div>
</header>
<nav class="mq-tabs" aria-label="阅读导航">
  <button type="button" class="mq-tab" data-action="mode-document" aria-current="page">文档</button>
  <button type="button" class="mq-tab" data-action="functions">函数</button>
{run_tab}
</nav>
"##,
            menu_icon = icon("M4 6h16M4 12h16M4 18h16"),
            site_title = escape_text(&navigation.site_title),
            logo = escape_attr(&self.assets.logo().data_uri()),
            search_icon = icon("M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0"),
        );
        let _ = (files_count, active);
    }

    fn render_ornament(&self, html: &mut String) {
        html.push_str(
            "<div class=\"mq-ornament-blossom\" aria-hidden=\"true\" data-decoration=\"blossom\"></div>\n",
        );
    }

    fn render_welcome(&self, html: &mut String, navigation: &crate::reader::model::Navigation) {
        let _ = write!(
            html,
            r##"<section class="mq-welcome" aria-label="欢迎页">
  <p class="mq-eyebrow">WELCOME TO</p>
  <h1>{title}</h1>
  <span class="mq-feather" aria-hidden="true"></span>
  <p class="mq-motto">代码即文档 · 文档即知识库 · 知识即可执行</p>
</section>
"##,
            title = escape_text(&navigation.site_title)
        );
    }

    fn render_document_meta(&self, html: &mut String, model: &PresentationModel) {
        let status = model.source_status;
        let _ = write!(
            html,
            r##"<div class="mq-docmeta">
  <span class="mq-filename">{file}</span>
  <span class="mq-status" data-source-status="{status_value}" role="status">{label}</span>
"##,
            file = escape_text(&model.document.identity.relative_path),
            status_value = status.data_value(),
            label = escape_text(status.label()),
        );
        if let Some(run) = &model.run {
            let _ = write!(
                html,
                "<span class=\"mq-status mq-run-status\" data-run-state=\"{}\">本次：{}</span>\n",
                run.state.data_value(),
                escape_text(run.state_label())
            );
        }
        if let Some(projection) = &model.semantic {
            if !projection.imports.is_empty() {
                html.push_str("<ul class=\"mq-shares\" aria-label=\"导入的模块\">\n");
                for import in &projection.imports {
                    let _ = write!(
                        html,
                        "<li><span class=\"mq-chip\">{alias} → {target}</span></li>\n",
                        alias = escape_text(&import.alias),
                        target = escape_text(&import.target)
                    );
                }
                html.push_str("</ul>\n");
            }
        }
        html.push_str("</div>\n");
        if let Some(note) = &model.status_note {
            let _ = write!(
                html,
                "<p class=\"mq-colophon\" role=\"note\">{note}</p>\n",
                note = escape_text(note)
            );
        }
    }

    fn render_diagnostics(&self, html: &mut String, model: &PresentationModel) {
        let mut items: Vec<(&str, &str, Option<u64>, Option<u64>, &str)> = Vec::new();
        for diagnostic in &model.diagnostics {
            items.push((
                &diagnostic.severity,
                &diagnostic.code,
                diagnostic.line,
                diagnostic.column,
                &diagnostic.message,
            ));
        }
        for warning in &model.warnings {
            let (line, column) = warning
                .span
                .map(|span| model.document.source_index.line_col(span.start as usize))
                .map(|(line, column)| (Some(line as u64), Some(column as u64)))
                .unwrap_or((None, None));
            items.push(("warning", &warning.code, line, column, &warning.message));
        }
        if items.is_empty() {
            return;
        }
        let has_error = items.iter().any(|item| item.0 == "error");
        let open = if has_error { " open" } else { "" };
        let _ = write!(
            html,
            "<details class=\"mq-diagnostics\" data-severity=\"{severity}\"{open}>\n<summary>{count} 条诊断（Core 与 View 来源已区分）</summary>\n<ol>\n",
            severity = if has_error { "error" } else { "warning" },
            open = open,
            count = items.len()
        );
        for (severity, code, line, column, message) in items {
            let _ = write!(
                html,
                "<li data-severity=\"{severity}\"><span class=\"mq-diag-code\">{code}</span>{loc}<span class=\"mq-diag-message\">{message}</span></li>\n",
                severity = escape_attr(severity),
                code = escape_text(code),
                loc = match (line, column) {
                    (Some(line), Some(column)) => format!(
                        "<span class=\"mq-diag-loc\">第 {line} 行第 {column} 列</span>"
                    ),
                    _ => String::new(),
                },
                message = escape_text(message)
            );
        }
        html.push_str("</ol>\n</details>\n");
    }

    fn render_functions(&self, html: &mut String, model: &PresentationModel, ctx: &RenderContext) {
        if model.outline.is_empty() {
            return;
        }
        let functions: Vec<_> = model.functions().collect();
        if functions.is_empty() {
            return;
        }
        let _ = write!(
            html,
            "<details class=\"mq-functions\" id=\"mq-functions\">\n<summary>本文函数（{count}）</summary>\n<ul>\n",
            count = functions.len()
        );
        for entry in functions {
            let function = model.semantic.as_ref().and_then(|semantic| {
                semantic.functions.iter().find(|function| {
                    anchor_for_span(function.heading_span.start, function.heading_span.end)
                        == entry.anchor
                })
            });
            let (function_id, params, callable) = match function {
                Some(function) => (function.id.clone(), function_params_json(function), true),
                None => (String::new(), "[]".to_string(), false),
            };
            let call_button = if callable && ctx.core_available {
                format!(
                    "<button type=\"button\" class=\"mq-fn-call\" data-action=\"call-function\" data-function-id=\"{id}\" data-function-label=\"{label}\" data-params=\"{params}\">调用</button>",
                    id = escape_attr(&function_id),
                    label = escape_attr(&entry.label),
                    params = escape_attr(&params)
                )
            } else {
                String::new()
            };
            let _ = write!(
                html,
                "<li><a href=\"#{anchor}\" data-goto-anchor=\"{anchor}\">{label}</a><span class=\"mq-fn-sig\">{detail}</span>{call_button}</li>\n",
                anchor = escape_attr(&entry.anchor),
                label = escape_text(&entry.label),
                detail = escape_text(&entry.detail),
                call_button = call_button
            );
        }
        html.push_str("</ul>\n</details>\n");
    }

    fn render_document(&self, html: &mut String, model: &PresentationModel, ctx: &RenderContext) {
        html.push_str("<article class=\"mq-document\" id=\"mq-document\">\n");
        let current = SafeRelativePath::parse(&model.document.identity.relative_path).ok();
        match &current {
            Some(current) => {
                render_blocks(
                    html,
                    &model.document.blocks,
                    model,
                    ctx,
                    current,
                    self.assets,
                );
            }
            None => {
                let _ = write!(
                    html,
                    "<p class=\"mq-colophon\">相对路径无效，已进入纯文本阅读模式。</p>\n"
                );
            }
        }
        html.push_str("</article>\n");
    }

    fn render_source_pane(
        &self,
        html: &mut String,
        model: &PresentationModel,
        _ctx: &RenderContext,
    ) {
        let source = &model.document.source;
        html.push_str("<section class=\"mq-source\" id=\"mq-source\" aria-label=\"原始源码\">\n<ol class=\"mq-source-lines\">\n");
        if source.is_empty() {
            html.push_str("<li class=\"mq-source-empty\">（空文件）</li>\n");
        }
        let index = &model.document.source_index;
        for line in 1..=index.line_count() {
            let text = index.line_text(source, line).unwrap_or("");
            let _ = write!(
                html,
                "<li id=\"mq-src-{line}\" data-line=\"{line}\"><span class=\"mq-line-no\" aria-hidden=\"true\">{line}</span><span class=\"mq-line-text\">{text}</span></li>\n",
                line = line,
                text = escape_text(text)
            );
        }
        html.push_str("</ol>\n</section>\n");
        // Exact bytes for the copy action (never interpreted as markup).
        let _ = write!(
            html,
            "<textarea id=\"mq-raw-source\" hidden readonly aria-hidden=\"true\">{}</textarea>\n",
            escape_text(source)
        );
    }

    fn render_footer(&self, html: &mut String, model: &PresentationModel) {
        let core = model
            .core_version
            .clone()
            .unwrap_or_else(|| "core unavailable".to_string());
        let _ = write!(
            html,
            r##"<footer class="mq-footer">
  <span>Marqdo · 静态阅读</span>
  <span>Analysis v2 · Trace v2 · {core}</span>
  <span>默认不执行 · 无自动运行</span>
</footer>
"##,
            core = escape_text(&core)
        );
    }

    /// Empty, accessible run panel that the single reader script fills from
    /// SSE events. It is never populated by the server for a live page: no run
    /// exists until the user explicitly submits one (RFC-VIEW-0002 §7.1).
    fn render_live_run_shell(&self, html: &mut String, _ctx: &RenderContext) {
        html.push_str(
            r##"<section class="mq-run-panel" id="mq-run-panel" data-run-state="idle" aria-labelledby="mq-run-title" hidden>
  <h2 id="mq-run-title">本次运行</h2>
  <p id="mq-run-status" class="mq-run-status" role="status" aria-live="polite">尚未运行。点击 Dock 的“运行”执行 main，或在函数面板选择函数。</p>
  <div id="mq-run-actions" class="mq-run-actions" hidden>
    <button type="button" class="mq-secondary" data-action="cancel-run">取消运行</button>
  </div>
  <div id="mq-run-summary"></div>
  <pre id="mq-run-stdout" class="mq-stdout" aria-label="程序标准输出" hidden></pre>
  <pre id="mq-run-stderr" class="mq-stdout mq-stderr" aria-label="程序标准错误" hidden></pre>
  <details id="mq-run-trace-wrap" class="mq-run-events-wrap" hidden>
    <summary>真实 Trace v2 事件</summary>
    <ol id="mq-run-events" class="mq-run-events"></ol>
  </details>
  <p id="mq-run-note" class="mq-colophon">结果只来自本次显式运行的 CLI --json 与经摘要校验的 Trace v2；不提供参数实际值、逐语句变量或内存快照。</p>
</section>
"##,
        );
    }

    fn render_dock(&self, html: &mut String, ctx: &RenderContext) {
        let settings = icon("M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2");
        let more = icon("M5 12h.01M12 12h.01M19 12h.01");
        if ctx.live {
            let run_disabled = if ctx.core_available {
                ""
            } else {
                " data-disabled=\"true\""
            };
            let _ = write!(
                html,
                r##"<div class="mq-dock" role="toolbar" aria-label="文档操作">
  <button type="button" class="mq-dock-btn" data-action="reload" aria-label="刷新页面">{reload}<span>刷新</span></button>
  <button type="button" class="mq-dock-btn" data-action="settings" aria-label="阅读设置">{settings}<span>设置</span></button>
  <button type="button" class="mq-dock-btn" data-action="run" aria-label="运行文档"{run_disabled}>{play}<span>运行</span></button>
  <button type="button" class="mq-dock-btn" data-action="clear" aria-label="清理本地展示状态">{clear}<span>清理</span></button>
  <button type="button" class="mq-dock-btn" data-action="more" aria-label="更多操作">{more}<span>更多</span></button>
</div>
"##,
                reload = icon("M20 7V3l-3 3a8 8 0 1 0 2 11M20 3h-5"),
                settings = settings,
                play = icon("m9 5 11 7-11 7z"),
                clear = icon("m14 3 4 2-4 8M5 12l8 2 2 6H3zM7 15l-1 4M10 16l-1 4"),
                more = more,
                run_disabled = run_disabled
            );
        } else {
            // Static export has no execution controls (RFC-VIEW-0002 §7.2).
            let _ = write!(
                html,
                r##"<div class="mq-dock" role="toolbar" aria-label="文档操作">
  <button type="button" class="mq-dock-btn" data-action="settings" aria-label="阅读设置">{settings}<span>设置</span></button>
  <button type="button" class="mq-dock-btn" data-action="more" aria-label="更多操作">{more}<span>更多</span></button>
</div>
"##,
                settings = settings,
                more = more
            );
        }
    }

    fn render_toast(&self, html: &mut String) {
        html.push_str("<div class=\"mq-toast\" id=\"mq-toast\" role=\"status\" aria-live=\"polite\" hidden></div>\n");
    }

    fn render_dialogs(&self, html: &mut String, model: &PresentationModel, ctx: &RenderContext) {
        let csrf = ctx
            .csrf_token
            .as_deref()
            .map(|token| {
                format!(
                    "<meta name=\"mq-csrf-token\" content=\"{}\"/>",
                    escape_attr(token)
                )
            })
            .unwrap_or_default();
        let _ = write!(
            html,
            r##"{csrf}
<dialog class="mq-dialog" id="mq-dialog-search" aria-labelledby="mq-search-title">
  <div class="mq-dialog-head"><h2 id="mq-search-title">搜索当前文档</h2><button type="button" class="mq-icon-btn mq-close" data-action="close-dialog" aria-label="关闭搜索">×</button></div>
  <input class="mq-search-field" id="mq-search-input" type="search" placeholder="输入至少两个字符…" aria-label="搜索正文" autocomplete="off"/>
  <ul class="mq-search-results" id="mq-search-results"></ul>
</dialog>
<dialog class="mq-dialog" id="mq-dialog-settings" aria-labelledby="mq-settings-title">
  <div class="mq-dialog-head"><h2 id="mq-settings-title">阅读设置</h2><button type="button" class="mq-icon-btn mq-close" data-action="close-dialog" aria-label="关闭设置">×</button></div>
  <ul class="mq-menu-list">
    <li><button type="button" data-action="size">正文字号 <span class="mq-menu-hint">标准 / 大 / 小</span></button></li>
    <li><button type="button" data-action="theme">主题 <span class="mq-menu-hint">暖纸 / 深色</span></button></li>
    <li><button type="button" data-action="annotations">语义标注显隐 <span class="mq-menu-hint">锚点与侧注</span></button></li>
    <li><button type="button" data-action="print">打印 / 保存为 PDF <span class="mq-menu-hint">保留正文与注释</span></button></li>
  </ul>
</dialog>
<dialog class="mq-dialog" id="mq-dialog-more" aria-labelledby="mq-more-title">
  <div class="mq-dialog-head"><h2 id="mq-more-title">文件与更多</h2><button type="button" class="mq-icon-btn mq-close" data-action="close-dialog" aria-label="关闭菜单">×</button></div>
  <ul class="mq-menu-list">
    <li><button type="button" data-action="mode-document">文档态 <span class="mq-menu-hint">原始 Markdown 阅读</span></button></li>
    <li><button type="button" data-action="mode-source">源码态 <span class="mq-menu-hint">逐行原文，含标记</span></button></li>
    <li><button type="button" data-action="functions">函数大纲 <span class="mq-menu-hint">来自 Analysis v2</span></button></li>
    <li><button type="button" data-action="unsupported">未提供的能力 <span class="mq-menu-hint">变量 / 内存 / 调试 / 证明</span></button></li>
    <li><button type="button" data-action="print">打印 <span class="mq-menu-hint">隐藏交互与装饰</span></button></li>
    <li><button type="button" data-action="about">关于 Marqdo View <span class="mq-menu-hint">版本与协议</span></button></li>
  </ul>
  {files}
</dialog>
<dialog class="mq-dialog" id="mq-dialog-functions" aria-labelledby="mq-functions-title">
  <div class="mq-dialog-head"><h2 id="mq-functions-title">函数与知识单元</h2><button type="button" class="mq-icon-btn mq-close" data-action="close-dialog" aria-label="关闭函数列表">×</button></div>
  <div id="mq-functions-body"></div>
</dialog>
<dialog class="mq-dialog" id="mq-dialog-unsupported" aria-labelledby="mq-unsupported-title">
  <div class="mq-dialog-head"><h2 id="mq-unsupported-title">当前版本不支持</h2><button type="button" class="mq-icon-btn mq-close" data-action="close-dialog" aria-label="关闭说明">×</button></div>
  <div id="mq-unsupported-body"></div>
</dialog>
{run_dialog}
<dialog class="mq-dialog" id="mq-dialog-about" aria-labelledby="mq-about-title">
  <div class="mq-dialog-head"><h2 id="mq-about-title">关于 Marqdo View</h2><button type="button" class="mq-icon-btn mq-close" data-action="close-dialog" aria-label="关闭关于">×</button></div>
  <p>版本 {view_version} · Tool Protocol v1 · Analysis v2 · Trace v2。</p>
  <p>冻结核心：{core}。</p>
  <p class="mq-colophon">本页面只渲染原始 Markdown 与已校验的静态语义；打开、刷新、搜索、导出都不会执行文档。运行需要用户显式确认。</p>
</dialog>
"##,
            csrf = csrf,
            files = self.render_file_list(model, ctx),
            run_dialog = if ctx.core_available {
                render_run_form_dialog(model, ctx)
            } else {
                String::new()
            },
            view_version = env!("CARGO_PKG_VERSION"),
            core = escape_text(model.core_version.as_deref().unwrap_or("未检测到可用核心")),
        );
    }

    fn render_run_panel(&self, html: &mut String, model: &PresentationModel, _ctx: &RenderContext) {
        let Some(run) = &model.run else {
            return;
        };
        let counts = &run.counts;
        let _ = write!(
            html,
            r##"<section class="mq-run-panel" id="mq-run-panel" data-run-state="{state}" data-run-id="{run_id}" aria-labelledby="mq-run-title">
  <h2 id="mq-run-title">本次运行 · {label}</h2>
  <p class="mq-colophon">入口 {entry} · 模式 pure · 退出码 {exit_code} · 事件 {total}（本文 {entry_count} / 其他模块 {other_count}）</p>
"##,
            state = run.state.data_value(),
            run_id = escape_attr(&run.run_id),
            label = escape_text(run.state_label()),
            entry = escape_text(run.entry.as_deref().unwrap_or("(未知入口)")),
            exit_code = run.exit_code,
            total = counts.total(),
            entry_count = run.entry_event_count,
            other_count = run.other_event_count,
        );
        if let Some(truncated) = &run.truncated {
            let _ = write!(
                html,
                "<p class=\"mq-run-warning\" role=\"note\">Trace 被截断：{}</p>\n",
                escape_text(&truncated.label())
            );
        }
        if let Some(code) = &run.error_code {
            let _ = write!(
                html,
                "<p class=\"mq-run-error\">Core 错误码：<code>{}</code></p>\n",
                escape_text(code)
            );
        }
        for warning in &run.warnings {
            let _ = write!(
                html,
                "<p class=\"mq-run-warning\" role=\"note\">{}</p>\n",
                escape_text(warning)
            );
        }
        if let Some(value) = &run.result_value {
            let _ = write!(
                html,
                "<p class=\"mq-run-result\">Core 本次返回值（公开 JSON）：<code>{}</code> <span class=\"mq-colophon\">类型 {}</span></p>\n",
                escape_text(&value.display),
                escape_text(&value.ty)
            );
        }
        if !run.stdout.is_empty() {
            let _ = write!(
                html,
                "<pre class=\"mq-stdout\" aria-label=\"程序标准输出\">{}</pre>\n",
                escape_text(&run.stdout)
            );
        }
        if !run.stderr.is_empty() {
            let _ = write!(
                html,
                "<pre class=\"mq-stdout mq-stderr\" aria-label=\"程序标准错误\">{}</pre>\n",
                escape_text(&run.stderr)
            );
        }
        if !run.events.is_empty() {
            html.push_str("<details class=\"mq-run-events-wrap\" open><summary>真实 Trace v2 事件</summary>\n<ol class=\"mq-run-events\">\n");
            for event in run.events.iter().take(200) {
                let location = match event.span {
                    Some(span) => {
                        let (line, column) =
                            model.document.source_index.line_col(span.start as usize);
                        format!("第 {line} 行第 {column} 列")
                    }
                    None => {
                        let module = event
                            .module_id
                            .as_deref()
                            .and_then(|module| module.rsplit("::").next())
                            .unwrap_or("(无模块)");
                        module.to_string()
                    }
                };
                let detail = event.detail.as_deref().unwrap_or("");
                let operation = event
                    .operation
                    .as_deref()
                    .map(|operation| format!(" · {operation}"))
                    .unwrap_or_default();
                let _ = write!(
                    html,
                    "<li data-seq=\"{seq}\"><span class=\"mq-event-kind\">{kind}</span><span class=\"mq-event-loc\">{location}</span> {detail}{operation}</li>\n",
                    seq = event.seq,
                    kind = escape_text(event.kind_label()),
                    location = escape_text(&location),
                    detail = escape_text(detail),
                    operation = escape_text(&operation)
                );
            }
            html.push_str("</ol>\n");
            if run.events.len() > 200 {
                let _ = write!(
                    html,
                    "<p class=\"mq-colophon\">仅显示前 200 条（共 {} 条）；完整数据保留在本地会话缓存中。</p>\n",
                    run.events.len()
                );
            }
            html.push_str("</details>\n");
        }
        html.push_str("<div class=\"mq-run-actions\"><button type=\"button\" class=\"mq-menu-action\" data-action=\"clear-run\">清理本次结果展示</button></div>\n</section>\n");
    }

    fn render_file_list(&self, model: &PresentationModel, ctx: &RenderContext) -> String {
        if model.navigation.documents.len() <= 1 {
            return String::new();
        }
        let Ok(current) = SafeRelativePath::parse(&model.document.identity.relative_path) else {
            return String::new();
        };
        let mut out = String::from(
            "<div class=\"mq-colophon\"><strong>项目内文档</strong><ul class=\"mq-menu-list\">\n",
        );
        for document in &model.navigation.documents {
            let href = ctx.links.document_href(&current, document);
            let active = model
                .navigation
                .active
                .as_ref()
                .map(|active| active == document)
                .unwrap_or(false);
            let _ = write!(
                out,
                "<li><a href=\"{href}\"{}>{name}</a></li>\n",
                if active { " aria-current=\"page\"" } else { "" },
                href = escape_attr(&href),
                name = escape_text(document.as_str())
            );
        }
        out.push_str("</ul></div>\n");
        out
    }
}

impl Default for ReaderRenderer {
    fn default() -> Self {
        Self::new()
    }
}

fn render_blocks(
    html: &mut String,
    blocks: &[DocBlock],
    model: &PresentationModel,
    ctx: &RenderContext,
    current: &SafeRelativePath,
    assets: &AssetBundle,
) {
    for block in blocks {
        render_block(html, block, model, ctx, current, assets);
    }
}

fn render_block(
    html: &mut String,
    block: &DocBlock,
    model: &PresentationModel,
    ctx: &RenderContext,
    current: &SafeRelativePath,
    assets: &AssetBundle,
) {
    match block {
        DocBlock::Heading {
            level,
            span,
            content,
        } => {
            let unit = resolve_heading_unit(model, *span, *level);
            let anchor = anchor_for_span(span.start, span.end);
            let slug = model
                .heading_slugs
                .get(span)
                .map(String::as_str)
                .unwrap_or(&anchor);
            if slug != anchor {
                let _ = write!(
                    html,
                    "<span class=\"mq-heading-anchor\" id=\"{}\" aria-hidden=\"true\"></span>",
                    escape_attr(&anchor)
                );
            }
            let (badge, cleaned) = display_heading_content(content, unit);
            let data_semantic = if let Some((_, data)) = badge {
                data
            } else if let Some(unit) = unit {
                match unit.kind {
                    UnitKind::Function => "exec",
                    UnitKind::Knowledge => "knowledge",
                    _ => "document",
                }
            } else {
                "document"
            };
            let _ = write!(
                html,
                "<h{} id=\"{}\" data-source-id=\"{}\" data-semantic=\"{}\">",
                level,
                escape_attr(slug),
                source_id_id(model, *span),
                data_semantic,
            );
            if let Some(badge) = badge {
                let _ = write!(
                    html,
                    "<span class=\"mq-mode-badge\" data-mode=\"{}\">{}</span>",
                    badge.1, badge.0
                );
            }
            render_inlines(html, &cleaned, model, ctx, current, assets);
            let _ = write!(html, "</h{level}>\n");
        }
        DocBlock::Paragraph { content, .. } => {
            html.push_str("<p>");
            render_inlines(html, content, model, ctx, current, assets);
            html.push_str("</p>\n");
        }
        DocBlock::List {
            ordered,
            start,
            tight: _,
            items,
            ..
        } => {
            if *ordered {
                match start {
                    Some(start) if *start != 1 => {
                        let _ = write!(html, "<ol start=\"{start}\">\n");
                    }
                    _ => html.push_str("<ol>\n"),
                }
            } else {
                html.push_str("<ul>\n");
            }
            for item in items {
                if let DocBlock::ListItem { task, children, .. } = item {
                    html.push_str("<li>");
                    if let Some(checked) = task {
                        let _ = write!(
                            html,
                            "<span class=\"mq-task\" data-checked=\"{checked}\" aria-label=\"{label}\"></span>",
                            checked = checked,
                            label = if *checked { "已完成" } else { "未完成" }
                        );
                    }
                    if let Some(first) = children.first() {
                        if let DocBlock::Paragraph { content, .. } = first {
                            render_inlines(html, content, model, ctx, current, assets);
                            for child in &children[1..] {
                                render_block(html, child, model, ctx, current, assets);
                            }
                        } else {
                            for child in children {
                                render_block(html, child, model, ctx, current, assets);
                            }
                        }
                    }
                    html.push_str("</li>\n");
                }
            }
            html.push_str(if *ordered { "</ol>\n" } else { "</ul>\n" });
        }
        DocBlock::ListItem { children, .. } => {
            html.push_str("<li>");
            for child in children {
                render_block(html, child, model, ctx, current, assets);
            }
            html.push_str("</li>\n");
        }
        DocBlock::BlockQuote { children, .. } => {
            html.push_str("<blockquote>\n");
            render_blocks(html, children, model, ctx, current, assets);
            html.push_str("</blockquote>\n");
        }
        DocBlock::CodeBlock {
            span,
            info,
            literal,
        } => {
            let resource = model
                .semantic
                .as_ref()
                .and_then(|projection| projection.resource_for_span(*span));
            let language = info.split_whitespace().next().unwrap_or("");
            let _ = write!(
                html,
                "<div class=\"mq-codeblock\" data-resource-kind=\"code\"{}>",
                resource
                    .map(|resource| format!(
                        " data-source-id=\"{}\"",
                        escape_attr(&source_id(
                            &model.document.identity.module_id,
                            resource.span,
                            "resource-code"
                        ))
                    ))
                    .unwrap_or_default()
            );
            if !language.is_empty() {
                let _ = write!(
                    html,
                    "<span class=\"mq-codeblock-lang\">{}</span>",
                    escape_text(language)
                );
            }
            let _ = write!(
                html,
                "<pre><code>{}</code></pre></div>\n",
                escape_text(literal)
            );
        }
        DocBlock::Table {
            alignments,
            header,
            rows,
            ..
        } => {
            html.push_str("<div class=\"mq-table-wrap\"><table>\n");
            if !header.is_empty() {
                html.push_str("<thead><tr>");
                for (index, cell) in header.iter().enumerate() {
                    let align = alignments.get(index).copied().unwrap_or(TableAlign::None);
                    let _ = write!(html, "<th{}>", align_attr(align));
                    render_inlines(html, cell, model, ctx, current, assets);
                    html.push_str("</th>");
                }
                html.push_str("</tr></thead>\n");
            }
            if !rows.is_empty() {
                html.push_str("<tbody>\n");
                for row in rows {
                    html.push_str("<tr>");
                    for (index, cell) in row.iter().enumerate() {
                        let align = alignments.get(index).copied().unwrap_or(TableAlign::None);
                        let _ = write!(html, "<td{}>", align_attr(align));
                        render_inlines(html, cell, model, ctx, current, assets);
                        html.push_str("</td>");
                    }
                    html.push_str("</tr>\n");
                }
                html.push_str("</tbody>\n");
            }
            html.push_str("</table></div>\n");
        }
        DocBlock::MathBlock { span, literal } => {
            let _ = write!(
                html,
                "<div class=\"mq-math-block\" data-resource-kind=\"formula\" data-source-id=\"{}\">{}</div>\n",
                escape_attr(&source_id(
                    &model.document.identity.module_id,
                    *span,
                    "resource-formula"
                )),
                render_math(literal, true)
            );
        }
        DocBlock::ThematicBreak { .. } => html.push_str("<hr/>\n"),
        DocBlock::HtmlBlock { .. } => {
            html.push_str(
                "<div class=\"mq-raw-omitted\" role=\"note\">已忽略原始 HTML 内容（安全默认）。源码态可查看原文。</div>\n",
            );
        }
        DocBlock::Other { .. } => {}
    }
}

fn resolve_heading_unit(
    model: &PresentationModel,
    heading_span: SourceSpan,
    level: u8,
) -> Option<&crate::protocol::analysis_v2::SemanticUnit> {
    let projection = model.semantic.as_ref()?;
    // A function heading maps to its function unit exactly; never borrow the
    // ancestor function for H3/H4 sections (that would duplicate DOM ids).
    if let Some(function) = projection
        .functions
        .iter()
        .find(|function| function.heading_span == heading_span)
    {
        if let Some(unit) = projection
            .units
            .iter()
            .find(|unit| unit.id == function.unit_id)
        {
            return Some(unit);
        }
    }
    projection
        .units
        .iter()
        .filter(|unit| unit.level == level && unit.span == heading_span)
        .min_by_key(|unit| unit.span.len())
        .or_else(|| projection.unit_for_heading(heading_span, level))
}

fn display_heading_content(
    content: &[Inline],
    unit: Option<&crate::protocol::analysis_v2::SemanticUnit>,
) -> (Option<(&'static str, &'static str)>, Vec<Inline>) {
    let Some(unit) = unit else {
        return (None, content.to_vec());
    };
    let plain: String = content.iter().map(Inline::plain_text).collect();
    for (prefix, badge, data) in [
        ("@exec ", "EXEC", "exec"),
        ("@knowledge ", "KNOWLEDGE", "knowledge"),
    ] {
        if let Some(rest) = plain.strip_prefix(prefix) {
            if normalize_heading_name(rest) == unit.name {
                return (
                    Some((badge, data)),
                    strip_prefix_from_inlines(content, prefix),
                );
            }
        }
    }
    (None, content.to_vec())
}

fn normalize_heading_name(raw: &str) -> String {
    // Name comparison here is only a display heuristic; the Core already
    // decided the semantic name. Collapse whitespace and trim.
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_prefix_from_inlines(content: &[Inline], prefix: &str) -> Vec<Inline> {
    let mut remaining = prefix.len();
    let mut out = Vec::new();
    for inline in content {
        if remaining == 0 {
            out.push(inline.clone());
            continue;
        }
        match inline {
            Inline::Text { span, literal } => {
                if remaining >= literal.len() {
                    remaining -= literal.len();
                } else {
                    let cut = remaining;
                    remaining = 0;
                    let rest = &literal[cut..];
                    if !rest.is_empty() {
                        out.push(Inline::Text {
                            span: *span,
                            literal: rest.to_string(),
                        });
                    }
                }
            }
            other => {
                remaining = 0;
                out.push(other.clone());
            }
        }
    }
    out
}

fn render_inlines(
    html: &mut String,
    inlines: &[Inline],
    model: &PresentationModel,
    ctx: &RenderContext,
    current: &SafeRelativePath,
    assets: &AssetBundle,
) {
    let _ = assets;
    for inline in inlines {
        render_inline(html, inline, model, ctx, current);
    }
}

fn render_inline(
    html: &mut String,
    inline: &Inline,
    model: &PresentationModel,
    ctx: &RenderContext,
    current: &SafeRelativePath,
) {
    let projection = model.semantic.as_ref();
    match inline {
        Inline::Text { span, literal } => {
            // A text node can itself be the payload of an inline code carrier;
            // anchors are applied by the enclosing carrier branch below.
            let _ = span;
            html.push_str(&escape_text(literal));
        }
        Inline::Code {
            span,
            literal,
            ticks: _,
        } => {
            let anchor = if ctx.annotations {
                projection.and_then(|projection| projection.statement_for_inline(*span))
            } else {
                None
            };
            html.push_str("<code");
            if let Some((_, statement)) = anchor {
                let id = source_id(
                    &model.document.identity.module_id,
                    statement.span,
                    &format!("statement-{}", statement.kind.data_value()),
                );
                let _ = write!(
                    html,
                    " class=\"mq-anchor\" data-annotation-source=\"analysis\" data-source-id=\"{id}\" data-statement-kind=\"{kind}\" title=\"静态语句：{label}\"",
                    id = escape_attr(&id),
                    kind = statement.kind.data_value(),
                    label = statement.kind.label()
                );
            }
            html.push('>');
            html.push_str(&escape_text(literal));
            html.push_str("</code>");
            if let Some((_, statement)) = anchor {
                render_run_chips(html, model, statement.span);
            }
        }
        Inline::Strong { span, children } => {
            let anchor = if ctx.annotations {
                projection.and_then(|projection| projection.statement_for_inline(*span))
            } else {
                None
            };
            html.push_str("<strong");
            if let Some((_, statement)) = anchor {
                let id = source_id(
                    &model.document.identity.module_id,
                    statement.span,
                    &format!("statement-{}", statement.kind.data_value()),
                );
                let _ = write!(
                    html,
                    " class=\"mq-anchor\" data-annotation-source=\"analysis\" data-source-id=\"{id}\" data-statement-kind=\"{kind}\" title=\"静态语句：{label}\"",
                    id = escape_attr(&id),
                    kind = statement.kind.data_value(),
                    label = statement.kind.label()
                );
            }
            html.push('>');
            render_inlines(html, children, model, ctx, current, assets_placeholder());
            html.push_str("</strong>");
            if let Some((_, statement)) = anchor {
                render_run_chips(html, model, statement.span);
            }
        }
        Inline::Emph { span, children } => {
            let anchor = if ctx.annotations {
                projection.and_then(|projection| projection.statement_for_inline(*span))
            } else {
                None
            };
            html.push_str("<em");
            if let Some((_, statement)) = anchor {
                let id = source_id(
                    &model.document.identity.module_id,
                    statement.span,
                    &format!("statement-{}", statement.kind.data_value()),
                );
                let _ = write!(
                    html,
                    " class=\"mq-anchor\" data-annotation-source=\"analysis\" data-source-id=\"{id}\" data-statement-kind=\"{kind}\" title=\"静态语句：{label}\"",
                    id = escape_attr(&id),
                    kind = statement.kind.data_value(),
                    label = statement.kind.label()
                );
            }
            html.push('>');
            render_inlines(html, children, model, ctx, current, assets_placeholder());
            html.push_str("</em>");
            if let Some((_, statement)) = anchor {
                render_run_chips(html, model, statement.span);
            }
        }
        Inline::Strikethrough { children, .. } => {
            html.push_str("<del>");
            render_inlines(html, children, model, ctx, current, assets_placeholder());
            html.push_str("</del>");
        }
        Inline::Link {
            span,
            url,
            title,
            children,
        } => {
            let reference = projection.and_then(|projection| projection.reference_for_span(*span));
            let state = reference.map(|reference| reference.state.as_str());
            let target = resolve_link(url, current, model, &ctx.links);
            match target {
                Some(href) => {
                    let _ = write!(html, "<a href=\"{href}\"", href = escape_attr(&href));
                    if !title.is_empty() {
                        let _ = write!(html, " title=\"{}\"", escape_attr(title));
                    }
                    if let Some(state) = state {
                        let _ = write!(html, " data-reference-state=\"{}\"", escape_attr(state));
                    }
                    html.push('>');
                    render_inlines(html, children, model, ctx, current, assets_placeholder());
                    html.push_str("</a>");
                }
                None => {
                    html.push_str(
                        "<span class=\"mq-link-blocked\" title=\"链接地址已被安全策略移除\">",
                    );
                    render_inlines(html, children, model, ctx, current, assets_placeholder());
                    html.push_str("</span>");
                }
            }
        }
        Inline::Image {
            url, alt, title, ..
        } => {
            if let Some(media) = model.media.get(url) {
                let _ = write!(html, "<img class=\"mq-document-image\" src=\"{}\" alt=\"{}\" width=\"{}\" height=\"{}\" title=\"{}\">",
                    escape_attr(&ctx.links.media_href(media)), escape_attr(alt),
                    media.width, media.height, escape_attr(title));
            } else {
                let _ = write!(html, "<span class=\"mq-raw-omitted\" role=\"note\" title=\"{}\">[图片已阻止自动加载：{}]</span>",
                    escape_attr(url), escape_text(if alt.is_empty() { "无替代文本" } else { alt }));
            }
        }
        Inline::Math {
            span,
            literal,
            display,
        } => {
            let resource = projection.and_then(|projection| projection.resource_for_span(*span));
            let _ = write!(
                html,
                "<span class=\"mq-math\" data-resource-kind=\"formula\"{}>{}</span>",
                resource
                    .map(|resource| format!(
                        " data-source-id=\"{}\"",
                        escape_attr(&source_id(
                            &model.document.identity.module_id,
                            resource.span,
                            "resource-formula"
                        ))
                    ))
                    .unwrap_or_default(),
                render_math(literal, *display)
            );
        }
        Inline::HtmlInline { .. } => {
            html.push_str("<span class=\"mq-raw-omitted\">已忽略原始行内 HTML</span>");
        }
        Inline::SoftBreak { .. } => html.push(' '),
        Inline::LineBreak { .. } => html.push_str("<br/>"),
        Inline::Other { .. } => {}
    }
}

// The renderer is a struct method in the hot path; this helper keeps the
// recursive inline signature small without a second renderer.
fn assets_placeholder() -> &'static AssetBundle {
    AssetBundle::get()
}

fn align_attr(align: TableAlign) -> &'static str {
    match align {
        TableAlign::None => "",
        TableAlign::Left => " style=\"text-align:left\"",
        TableAlign::Center => " style=\"text-align:center\"",
        TableAlign::Right => " style=\"text-align:right\"",
    }
}

fn icon(path: &str) -> String {
    format!(
        "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path d=\"{path}\"/></svg>"
    )
}

fn source_id_id(model: &PresentationModel, span: SourceSpan) -> String {
    escape_attr(&source_id(
        &model.document.identity.module_id,
        span,
        "heading",
    ))
}

fn escape_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 8);
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_attr(raw: &str) -> String {
    escape_text(raw)
}

fn csp_policy() -> &'static str {
    // `frame-ancestors` is ignored in <meta>; the HTTP header (server.rs) adds
    // the framing restrictions. Keep the meta policy identical otherwise.
    "default-src 'none'; style-src 'self'; script-src 'self' file:; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; object-src 'none'"
}

fn percent_encode_path(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        let unreserved =
            byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~' | b'/');
        if unreserved {
            out.push(*byte as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Sanitize one Markdown link destination. Returns `None` for schemes the
/// reader must never expose (`javascript:`, `data:`, `file:`, ...) and for
/// console-control-looking payloads.
fn resolve_link(
    raw: &str,
    current: &SafeRelativePath,
    model: &PresentationModel,
    policy: &LinkPolicy,
) -> Option<String> {
    let url = raw.trim();
    if url.is_empty()
        || url.chars().any(char::is_control)
        || url.contains('\\')
        || url.starts_with('/')
    {
        return None;
    }
    let (path_with_query, fragment) = url
        .split_once('#')
        .map_or((url, None), |(path, fragment)| (path, Some(fragment)));
    let path = path_with_query.split('?').next()?;
    if let Some((scheme, _)) = path.split_once(':') {
        return matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        )
        .then(|| url.to_string());
    }
    let fragment = fragment
        .map(super::links::decode)
        .unwrap_or_else(|| Some(String::new()))?;
    let suffix = if fragment.is_empty() {
        String::new()
    } else {
        format!("#{}", percent_encode_path(&fragment))
    };
    if path.is_empty() {
        return Some(suffix);
    }
    let target = super::links::relative_url(current, path)?;
    if target.as_str().ends_with(".mq.md") {
        if !model.navigation.documents.contains(&target) {
            return None;
        }
        return Some(format!(
            "{}{suffix}",
            policy.document_href(current, &target)
        ));
    }
    Some(url.to_string())
}

fn render_math(literal: &str, display: bool) -> String {
    super::math::render(literal, display).unwrap_or_else(|| {
        format!(
            "<code title=\"公式无法排版\">{}</code>",
            escape_text(literal)
        )
    })
}

fn render_run_chips(html: &mut String, model: &PresentationModel, statement_span: SourceSpan) {
    let Some(run) = &model.run else {
        return;
    };
    let Some(summary) = run.anchor_summary(statement_span) else {
        return;
    };
    let suffix = if summary.count > 1 {
        format!(" ×{}", summary.count)
    } else {
        String::new()
    };
    let _ = write!(
        html,
        "<span class=\"mq-run-chip\" data-run-seq=\"{seq}\" data-run-kind=\"{kinds}\" title=\"真实 Trace 事件：{count} 条（{kinds}）\">{detail}{suffix}</span>",
        seq = summary.last_seq,
        kinds = escape_attr(&summary.kinds.join(",")),
        count = summary.count,
        detail = escape_text(&summary.detail),
        suffix = escape_text(&suffix)
    );
}

fn function_params_json(function: &crate::protocol::analysis_v2::SemanticFunction) -> String {
    serde_json::json!(function
        .signature
        .params
        .iter()
        .map(|param| serde_json::json!({
            "name": param.name,
            "type": param.ty,
            "required": param.required,
            "default": param.default,
        }))
        .collect::<Vec<_>>())
    .to_string()
}

fn render_run_form_dialog(model: &PresentationModel, ctx: &RenderContext) -> String {
    let path = &model.document.identity.relative_path;
    format!(
        r##"<dialog class="mq-dialog mq-run-dialog" id="mq-dialog-run" aria-labelledby="mq-run-dialog-title" data-path="{path}" data-timeout="{timeout}">
  <div class="mq-dialog-head"><h2 id="mq-run-dialog-title">显式运行</h2><button type="button" class="mq-icon-btn mq-close" data-action="close-dialog" aria-label="取消运行">×</button></div>
  <p class="mq-run-target"><span id="mq-run-target-label">入口 main</span> · <span id="mq-run-target-mode">pure</span></p>
  <div id="mq-run-params" class="mq-run-params"></div>
  <fieldset class="mq-run-mode">
    <legend>执行模式</legend>
    <label><input type="radio" name="mq-mode" value="pure" checked/> pure（默认：无文件/网络/环境/进程权限）</label>
    <label><input type="radio" name="mq-mode" value="project"/> project（受控项目权限，需逐项确认）</label>
  </fieldset>
  <div id="mq-run-grants" class="mq-run-grants" hidden>
    <p class="mq-run-warning" role="note">project 模式不是 OS 沙箱。只允许相对项目根的 fs read/write glob；网络、进程、环境、插件等权限不在首版 UI 中，必须改用终端 CLI。</p>
    <label>允许读取（read glob，每行一个）<textarea id="mq-grant-read" rows="2" spellcheck="false" placeholder="data/**"></textarea></label>
    <label>允许写入（write glob，每行一个）<textarea id="mq-grant-write" rows="2" spellcheck="false" placeholder="out/*.txt"></textarea></label>
  </div>
  <p class="mq-colophon">等价命令（点击“确认执行”后才会运行；<code>&lt;参数值&gt;</code> 由下方输入原样作为常量表达式提交）：</p>
  <pre class="mq-run-command" id="mq-run-command" aria-label="命令预览"></pre>
  <p class="mq-colophon">墙钟上限 {timeout} 秒；结果通过 SSE 推送，不轮询。打开/刷新/导出不会执行。</p>
  <div class="mq-dialog-actions">
    <button type="button" class="mq-secondary" data-action="close-dialog">取消</button>
    <button type="button" class="mq-primary" data-action="run-confirm">确认执行</button>
  </div>
</dialog>
"##,
        path = escape_attr(path),
        timeout = ctx.run_timeout_secs
    )
}
