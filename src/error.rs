//! Stable View-owned error kinds.
//!
//! Every failure that reaches the user has exactly one stable machine code
//! (`view.*`), an exit code class and a `source` field so Core diagnostics can
//! never be mistaken for View diagnostics. See RFC-VIEW-0001 §10.2.

use std::fmt;

/// Exit code 0: success. Exit code 1: runtime/I/O failure. Exit code 2:
/// argument, protocol or permission failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitClass {
    Runtime,
    Argument,
}

#[derive(Debug)]
pub enum ViewError {
    /// `MARQDO_BIN` unset, missing, or not executable.
    CoreMissing { detail: String },
    /// Tool/Analysis/Trace protocol or product range mismatch.
    ProtocolIncompatible { detail: String },
    /// Core `check` failed; prose stays readable but semantics are unusable.
    AnalysisFailed { detail: String },
    /// Analysis `source_digest` does not match the current source bytes.
    SourceMismatch { expected: String, actual: String },
    /// A Core byte span is out of range or not on a UTF-8 boundary.
    SourceSpanInvalid { detail: String },
    /// Trace does not bind to the current Analysis bytes.
    TraceStale { detail: String },
    /// The user did not confirm, or the requested mode/grants are not allowed.
    RunDenied { detail: String },
    /// Path escapes the allowed project root or is otherwise forbidden.
    PathForbidden { detail: String },
    /// Refused a bind address that is not explicitly allowed.
    BindDenied { detail: String },
    /// Embedded asset digest mismatch (build/package integrity).
    AssetIntegrity { detail: String },
    /// Export target is unsafe or the write failed.
    ExportFailed { detail: String },
    /// A configured resource limit was exceeded.
    ResourceLimit { detail: String },
    /// Core subprocess could not be spawned.
    CoreSpawn { detail: String },
    /// Core subprocess exceeded the View-level timeout and was killed.
    CoreTimeout { detail: String },
    /// Local file system / HTTP I/O failure.
    Io { detail: String },
    /// Feature intentionally not implemented in this build (honest status).
    Unsupported { detail: String },
}

impl ViewError {
    pub fn code(&self) -> &'static str {
        match self {
            ViewError::CoreMissing { .. } => "view.core_missing",
            ViewError::ProtocolIncompatible { .. } => "view.protocol_incompatible",
            ViewError::AnalysisFailed { .. } => "view.analysis_failed",
            ViewError::SourceMismatch { .. } => "view.source_mismatch",
            ViewError::SourceSpanInvalid { .. } => "view.source_span_invalid",
            ViewError::TraceStale { .. } => "view.trace_stale",
            ViewError::RunDenied { .. } => "view.run_denied",
            ViewError::PathForbidden { .. } => "view.path_forbidden",
            ViewError::BindDenied { .. } => "view.bind_denied",
            ViewError::AssetIntegrity { .. } => "view.asset_integrity",
            ViewError::ExportFailed { .. } => "view.export_failed",
            ViewError::ResourceLimit { .. } => "view.resource_limit",
            ViewError::CoreSpawn { .. } => "view.core_spawn",
            ViewError::CoreTimeout { .. } => "view.core_timeout",
            ViewError::Io { .. } => "view.io",
            ViewError::Unsupported { .. } => "view.unsupported",
        }
    }

    pub fn exit_class(&self) -> ExitClass {
        match self {
            ViewError::CoreMissing { .. }
            | ViewError::ProtocolIncompatible { .. }
            | ViewError::RunDenied { .. }
            | ViewError::PathForbidden { .. }
            | ViewError::BindDenied { .. }
            | ViewError::SourceSpanInvalid { .. }
            | ViewError::Unsupported { .. } => ExitClass::Argument,
            _ => ExitClass::Runtime,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.exit_class() {
            ExitClass::Runtime => 1,
            ExitClass::Argument => 2,
        }
    }

    /// `core` is used only for diagnostics that came from the frozen CLI.
    pub fn source(&self) -> &'static str {
        "view"
    }

    /// JSON diagnostic with `source:"view"` so it can never masquerade as a
    /// Core diagnostic. Contains no absolute machine paths beyond what the
    /// caller already supplied.
    /// Public, detail-free message for pages/JSON served to the browser. Never
    /// contains absolute machine paths, scratch paths or raw subprocess text.
    pub fn public_message(&self) -> &'static str {
        match self.code() {
            "view.core_missing" => "未找到可用的冻结核心（MARQDO_BIN）。",
            "view.protocol_incompatible" => "核心或协议版本不兼容。",
            "view.analysis_failed" => "Core 静态检查失败；正文仍可阅读，语义标注已撤销。",
            "view.source_mismatch" => "当前源码与 Analysis 字节摘要不一致。",
            "view.source_span_invalid" => "语义位置超出源码或不在 UTF-8 边界。",
            "view.trace_stale" => "运行结果与当前 Analysis 不匹配，已标记为过期。",
            "view.run_denied" => "运行请求未获授权或被拒绝。",
            "view.path_forbidden" => "请求的路径超出允许的项目根目录。",
            "view.bind_denied" => "拒绝绑定该网络地址。",
            "view.asset_integrity" => "内嵌资源摘要不一致。",
            "view.export_failed" => "静态导出失败（输出目录不安全或写入失败）。",
            "view.resource_limit" => "超过配置的资源上限。",
            "view.core_spawn" => "无法启动冻结核心。",
            "view.core_timeout" => "冻结核心执行超时，已终止。",
            "view.io" => "本地 I/O 失败。",
            "view.unsupported" => "当前版本不支持该功能。",
            _ => "View 内部错误。",
        }
    }

    /// Browser-safe JSON diagnostic: stable code + public message only.
    pub fn to_public_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": "marqdo.view-error",
            "schema_version": 1,
            "code": self.code(),
            "source": "view",
            "message": self.public_message(),
            "exit_code": self.exit_code(),
        })
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": "marqdo.view-error",
            "schema_version": 1,
            "code": self.code(),
            "source": self.source(),
            "message": self.detail(),
            "exit_code": self.exit_code(),
        })
    }

    pub fn detail(&self) -> &str {
        match self {
            ViewError::CoreMissing { detail }
            | ViewError::ProtocolIncompatible { detail }
            | ViewError::AnalysisFailed { detail }
            | ViewError::SourceSpanInvalid { detail }
            | ViewError::TraceStale { detail }
            | ViewError::RunDenied { detail }
            | ViewError::PathForbidden { detail }
            | ViewError::BindDenied { detail }
            | ViewError::AssetIntegrity { detail }
            | ViewError::ExportFailed { detail }
            | ViewError::ResourceLimit { detail }
            | ViewError::CoreSpawn { detail }
            | ViewError::CoreTimeout { detail }
            | ViewError::Io { detail }
            | ViewError::Unsupported { detail } => detail,
            ViewError::SourceMismatch { expected, actual } => {
                // Cannot return a formatted &str; handled in Display instead.
                let _ = (expected, actual);
                "source digest mismatch"
            }
        }
    }
}

impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViewError::SourceMismatch { expected, actual } => write!(
                f,
                "{}: source digest mismatch (expected {expected}, actual {actual})",
                self.code()
            ),
            other => write!(f, "{}: {}", other.code(), other.detail()),
        }
    }
}

impl std::error::Error for ViewError {}

impl From<std::io::Error> for ViewError {
    fn from(err: std::io::Error) -> Self {
        ViewError::Io {
            detail: err.to_string(),
        }
    }
}

pub type ViewResult<T> = Result<T, ViewError>;

impl ViewError {
    pub fn io(context: &str, err: std::io::Error) -> Self {
        ViewError::Io {
            detail: format!("{context}: {err}"),
        }
    }
}
