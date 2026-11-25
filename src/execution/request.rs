//! Validation of the two-phase explicit run request (RFC-VIEW-0002 §6.3).
//!
//! The browser never sends a command: it sends a document, a target selected
//! from Analysis v3, constant parameter expressions, the exact Analysis digest
//! it rendered, a mode and (for project mode) explicit fs globs. Everything is
//! re-validated here and finally by the Core itself; the client cannot inject
//! cwd, flags, a binary path or shell text.

use crate::core_client::Grants;
use crate::error::{ViewError, ViewResult};
use crate::protocol::analysis_v3::SemanticProjection;
use crate::reader::source::SafeRelativePath;
use serde::Deserialize;

/// Bounded request body size accepted before JSON parsing (RFC §6.8).
pub const MAX_RUN_BODY_BYTES: usize = 64 * 1024;
pub const MAX_PARAMETERS: usize = 32;
pub const MAX_EXPRESSION_CHARS: usize = 4096;
pub const MAX_GRANT_GLOBS: usize = 16;
pub const MAX_GRANT_CHARS: usize = 256;
pub const MAX_CLIENT_REQUEST_ID_CHARS: usize = 128;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub document: String,
    pub target: RunTarget,
    #[serde(default)]
    pub parameters: Vec<ParameterInput>,
    pub analysis_digest: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub grants: GrantRequest,
    pub client_request_id: String,
}

fn default_mode() -> String {
    "pure".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum RunTarget {
    #[serde(rename = "entry")]
    Entry,
    #[serde(rename = "function")]
    Function { function_id: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterInput {
    pub name: String,
    pub expression: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantRequest {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Pure,
    Project,
}

impl RunMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunMode::Pure => "pure",
            RunMode::Project => "project",
        }
    }
}

/// A request that passed schema/size/path/mode/grant validation, before the
/// read-only `check` and Analysis-digest step performed by the controller.
#[derive(Debug, Clone)]
pub struct ValidatedRequest {
    pub document: SafeRelativePath,
    pub target: RunTarget,
    pub parameters: Vec<ParameterInput>,
    pub analysis_digest: String,
    pub mode: RunMode,
    pub grants: Grants,
    pub client_request_id: String,
}

/// The target resolved against the entry module's verified Analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionTarget {
    /// `marqdo run FILE` (entry `main`).
    Entry,
    /// `marqdo call FILE NAME --arg ...`.
    Function {
        /// Canonical or unique local name accepted by the frozen CLI.
        cli_name: String,
        /// Display label from Analysis (`namespace.local_name` when present).
        label: String,
    },
}

/// One target/parameter plan for the Core argv builder.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub document: SafeRelativePath,
    pub target: ExecutionTarget,
    pub function_id: String,
    pub arguments: Vec<(String, String)>,
    pub mode: RunMode,
    pub grants: Grants,
    pub analysis_digest: String,
    pub client_request_id: String,
}

impl ValidatedRequest {
    /// Resolve the target and parameter names against the verified entry-module
    /// Analysis. Never accepts a dependency-module function id (RFC §6.3).
    pub fn plan_against(self, projection: &SemanticProjection) -> ViewResult<ExecutionPlan> {
        let (target, function_id) = match &self.target {
            RunTarget::Entry => {
                let Some(function) = resolve_main(projection) else {
                    return Err(ViewError::NoEntryFunction {
                        detail: format!(
                            "{} 没有可调用的 main；请从函数面板选择一个 H2 Exec 函数",
                            self.document
                        ),
                    });
                };
                (ExecutionTarget::Entry, function.id.clone())
            }
            RunTarget::Function { function_id } => {
                let function = projection
                    .functions
                    .iter()
                    .find(|function| &function.id == function_id)
                    .ok_or_else(|| ViewError::RunInvalid {
                        detail: format!(
                            "function_id {function_id:?} 不在当前入口模块可调用函数中；依赖模块函数不能作为 call 目标"
                        ),
                    })?;
                let canonical = canonical_name(&function.id, &projection.module_id);
                let cli_name = if unique_local_name(projection, &function.local_name, &function.id)
                {
                    function.local_name.clone()
                } else {
                    canonical.clone()
                };
                let label = canonical;
                validate_parameters(&self.parameters, projection, &function.id)?;
                (
                    ExecutionTarget::Function { cli_name, label },
                    function.id.clone(),
                )
            }
        };
        Ok(ExecutionPlan {
            document: self.document,
            target,
            function_id,
            arguments: self
                .parameters
                .into_iter()
                .map(|parameter| (parameter.name, parameter.expression))
                .collect(),
            mode: self.mode,
            grants: self.grants,
            analysis_digest: self.analysis_digest,
            client_request_id: self.client_request_id,
        })
    }
}

/// Parse and validate the raw request against fixed limits. Does not touch the
/// file system or the Core; the controller performs the read-only check next.
pub fn validate_request(body: &[u8]) -> ViewResult<ValidatedRequest> {
    if body.len() > MAX_RUN_BODY_BYTES {
        return Err(ViewError::RunInvalid {
            detail: format!("运行请求体超过 {MAX_RUN_BODY_BYTES} 字节上限"),
        });
    }
    let request: RunRequest =
        serde_json::from_slice(body).map_err(|err| ViewError::RunInvalid {
            detail: format!("运行请求不是合法的 marqdo.view-run-request v1 JSON: {err}"),
        })?;

    validate_client_request_id(&request.client_request_id)?;
    validate_digest(&request.analysis_digest)?;
    let document = SafeRelativePath::parse(&request.document)?;
    if !document.as_str().ends_with(".mq.md") {
        return Err(ViewError::RunInvalid {
            detail: "运行目标必须是受控项目根内的 .mq.md 文档".to_string(),
        });
    }
    match &request.target {
        RunTarget::Entry => {}
        RunTarget::Function { function_id } => {
            if function_id.is_empty()
                || function_id.len() > 512
                || function_id.chars().any(|ch| ch.is_control() || ch == '\0')
            {
                return Err(ViewError::RunInvalid {
                    detail: "function_id 必须来自 Analysis 且长度为 1..=512".to_string(),
                });
            }
        }
    }
    if request.parameters.len() > MAX_PARAMETERS {
        return Err(ViewError::RunInvalid {
            detail: format!("参数数量超过 {MAX_PARAMETERS} 个上限"),
        });
    }
    for parameter in &request.parameters {
        validate_parameter_name(&parameter.name)?;
        validate_expression(&parameter.expression)?;
    }

    let (mode, grants) = validate_mode_and_grants(&request.mode, &request.grants)?;

    Ok(ValidatedRequest {
        document,
        target: request.target,
        parameters: request.parameters,
        analysis_digest: request.analysis_digest,
        mode,
        grants,
        client_request_id: request.client_request_id,
    })
}

fn validate_client_request_id(value: &str) -> ViewResult<()> {
    if value.is_empty()
        || value.chars().count() > MAX_CLIENT_REQUEST_ID_CHARS
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(ViewError::RunInvalid {
            detail: format!(
                "client_request_id 必须是不透明的 ASCII 标识（1..={MAX_CLIENT_REQUEST_ID_CHARS}，字母/数字/-_ .:）"
            ),
        });
    }
    Ok(())
}

fn validate_digest(value: &str) -> ViewResult<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ViewError::RunInvalid {
            detail: "analysis_digest 必须以 sha256: 开头".to_string(),
        });
    };
    if hex.len() != 64 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(ViewError::RunInvalid {
            detail: "analysis_digest 必须是 sha256:<64 hex>".to_string(),
        });
    }
    Ok(())
}

fn validate_parameter_name(name: &str) -> ViewResult<()> {
    if name.is_empty()
        || name.chars().count() > 128
        || name
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || ch == '=')
    {
        return Err(ViewError::RunInvalid {
            detail: "参数名必须非空且不包含控制字符、空白或 '='".to_string(),
        });
    }
    Ok(())
}

fn validate_expression(expression: &str) -> ViewResult<()> {
    if expression.trim().is_empty() {
        return Err(ViewError::RunInvalid {
            detail: "参数 expression 不能为空；View 不做隐式裸文本转字符串".to_string(),
        });
    }
    if expression.chars().count() > MAX_EXPRESSION_CHARS {
        return Err(ViewError::RunInvalid {
            detail: format!("参数 expression 超过 {MAX_EXPRESSION_CHARS} 字符上限"),
        });
    }
    if expression.chars().any(|ch| ch == '\0' || ch.is_control()) {
        return Err(ViewError::RunInvalid {
            detail: "参数 expression 不能包含 NUL/控制字符".to_string(),
        });
    }
    Ok(())
}

fn validate_mode_and_grants(mode: &str, grants: &GrantRequest) -> ViewResult<(RunMode, Grants)> {
    match mode {
        "pure" => {
            if !grants.read.is_empty() || !grants.write.is_empty() {
                return Err(ViewError::RunInvalid {
                    detail: "pure 模式不能携带 fs grants；需要文件访问时请显式选择 project 模式"
                        .to_string(),
                });
            }
            Ok((RunMode::Pure, Grants::default()))
        }
        "project" => {
            if grants.read.len() > MAX_GRANT_GLOBS || grants.write.len() > MAX_GRANT_GLOBS {
                return Err(ViewError::RunInvalid {
                    detail: format!("每个 fs grant 列表最多 {MAX_GRANT_GLOBS} 项"),
                });
            }
            let mut seen = std::collections::BTreeSet::new();
            let mut validated = Grants::default();
            for glob in &grants.read {
                validate_glob(glob)?;
                if seen.insert(glob.clone()) {
                    validated.allow_read.push(glob.clone());
                }
            }
            let mut seen_write = std::collections::BTreeSet::new();
            for glob in &grants.write {
                validate_glob(glob)?;
                if seen_write.insert(glob.clone()) {
                    validated.allow_write.push(glob.clone());
                }
            }
            Ok((RunMode::Project, validated))
        }
        "strict" => Err(ViewError::RunDenied {
            detail: "strict 模式不在 View 首版提供；请改用 pure/project 或直接在终端运行 CLI"
                .to_string(),
        }),
        other => Err(ViewError::RunDenied {
            detail: format!(
                "模式 {other:?} 不在首版支持范围；仅允许 pure 或受控 project（fs read/write）"
            ),
        }),
    }
}

fn validate_glob(glob: &str) -> ViewResult<()> {
    if glob.is_empty() || glob.chars().count() > MAX_GRANT_CHARS {
        return Err(ViewError::RunInvalid {
            detail: format!("fs grant glob 必须为 1..={MAX_GRANT_CHARS} 字符"),
        });
    }
    if glob.starts_with('/')
        || glob.starts_with('\\')
        || glob.starts_with('~')
        || glob.contains('\\')
        || glob.contains('\0')
        || glob.chars().any(|ch| ch.is_control())
        || glob.split('/').any(|segment| segment == "..")
        || glob.contains(':')
    {
        return Err(ViewError::RunInvalid {
            detail: format!(
                "fs grant {glob:?} 必须是项目根相对 glob（禁止绝对路径、..、反斜杠、盘符/控制字符）"
            ),
        });
    }
    Ok(())
}

/// Resolve `main` exactly like the frozen CLI does for `run`: canonical name
/// first, then a unique local name. No fallback to any other function.
fn resolve_main(
    projection: &SemanticProjection,
) -> Option<&crate::protocol::analysis_v3::SemanticFunction> {
    projection
        .functions
        .iter()
        .find(|function| canonical_name(&function.id, &projection.module_id) == "main")
        .or_else(|| {
            let mut candidates = projection
                .functions
                .iter()
                .filter(|function| function.local_name == "main");
            let first = candidates.next()?;
            candidates.next().is_none().then_some(first)
        })
}

fn unique_local_name(projection: &SemanticProjection, local_name: &str, id: &str) -> bool {
    let matches = projection
        .functions
        .iter()
        .filter(|function| function.local_name == local_name)
        .count();
    if matches == 1 {
        return true;
    }
    // If several namespaces define the same local name, only the canonical
    // qualified name is unambiguous; the CLI resolver has the same rule.
    let canonical = canonical_name(id, &projection.module_id);
    canonical == local_name
}

/// Analysis v3 function ids are `<module id>::<canonical name>`.
pub fn canonical_name(function_id: &str, module_id: &str) -> String {
    function_id
        .strip_prefix(&format!("{module_id}::"))
        .unwrap_or(function_id)
        .to_string()
}

fn validate_parameters(
    parameters: &[ParameterInput],
    projection: &SemanticProjection,
    function_id: &str,
) -> ViewResult<()> {
    let function = projection
        .functions
        .iter()
        .find(|function| function.id == function_id)
        .ok_or_else(|| ViewError::RunInvalid {
            detail: "函数在选择后消失，请刷新页面重新选择".to_string(),
        })?;
    let mut seen = std::collections::BTreeSet::new();
    for parameter in parameters {
        if !function
            .signature
            .params
            .iter()
            .any(|param| param.name == parameter.name)
        {
            return Err(ViewError::RunInvalid {
                detail: format!(
                    "参数 {:?} 不在函数 {} 的签名中",
                    parameter.name, function.local_name
                ),
            });
        }
        if !seen.insert(parameter.name.clone()) {
            return Err(ViewError::RunInvalid {
                detail: format!("参数 {:?} 重复提交", parameter.name),
            });
        }
    }
    Ok(())
}

/// Stable digest of the exact request bytes, used for `client_request_id`
/// idempotency (same id + same bytes => same run; different bytes => 409).
pub fn payload_digest(bytes: &[u8]) -> String {
    crate::reader::source::sha256_hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection() -> SemanticProjection {
        use crate::protocol::analysis_v3::{
            DocumentNode, FunctionParam, FunctionSignature, ModuleSummary, SemanticFunction,
        };
        use crate::reader::source::{sha256_hex, SourceSpan};

        let module_id = "root:t@0.1.0::index.mq.md";
        let source = "x".repeat(200);
        let source_digest = sha256_hex(source.as_bytes());
        SemanticProjection {
            schema_version: 3,
            analysis_sha256: "a".repeat(64),
            analysis_digest: format!("sha256:{}", "a".repeat(64)),
            producer_version: "3.0.0-rc.4".to_string(),
            markdown_profile: "marqdo.md.v1".to_string(),
            project_root_id: "root:t@0.1.0".to_string(),
            identity_kind: "manifest".to_string(),
            entry_module_id: module_id.to_string(),
            lock_digest: String::new(),
            manifest_digest: String::new(),
            stdlib_version: None,
            stdlib_digest: None,
            stdlib_instance_id: None,
            extensions: Vec::new(),
            module_id: module_id.to_string(),
            package_instance_id: "root:t@0.1.0".to_string(),
            relative_path: "index.mq.md".to_string(),
            source_digest: source_digest.clone(),
            source: source.clone(),
            document: DocumentNode {
                kind: "document".to_string(),
                span: SourceSpan::new(0, source.len() as u64),
                children: Vec::new(),
                literal: None,
                level: None,
                setext: None,
                ordered: None,
                start: None,
                tight: None,
                task: None,
                ticks: None,
                fenced: None,
                info: None,
                display: None,
                url: None,
                title: None,
                alignments: None,
                header: None,
                tag: None,
            },
            anchors: Vec::new(),
            units: Vec::new(),
            functions: vec![
                SemanticFunction {
                    id: format!("{module_id}::main"),
                    unit_id: "u1".to_string(),
                    local_name: "main".to_string(),
                    namespace: None,
                    span: SourceSpan::new(0, 10),
                    heading_span: SourceSpan::new(0, 10),
                    signature: FunctionSignature {
                        kind: "inferred".to_string(),
                        params: Vec::new(),
                    },
                    effects: vec!["unknown".to_string()],
                    statements: Vec::new(),
                },
                SemanticFunction {
                    id: format!("{module_id}::N.f"),
                    unit_id: "u2".to_string(),
                    local_name: "f".to_string(),
                    namespace: Some("N".to_string()),
                    span: SourceSpan::new(20, 30),
                    heading_span: SourceSpan::new(20, 30),
                    signature: FunctionSignature {
                        kind: "explicit".to_string(),
                        params: vec![FunctionParam {
                            name: "n".to_string(),
                            ty: Some("Int".to_string()),
                            required: true,
                            default: None,
                            span: SourceSpan::new(24, 25),
                        }],
                    },
                    effects: vec!["unknown".to_string()],
                    statements: Vec::new(),
                },
            ],
            resources: Vec::new(),
            imports: Vec::new(),
            references: Vec::new(),
            calls: Vec::new(),
            diagnostics: Vec::new(),
            warnings: Vec::new(),
            modules: vec![ModuleSummary {
                id: module_id.to_string(),
                package_instance_id: "root:t@0.1.0".to_string(),
                relative_path: "index.mq.md".to_string(),
                source_digest,
                source,
            }],
        }
    }

    #[test]
    fn rejects_unknown_fields_and_bad_modes() {
        let base = serde_json::json!({
            "document": "index.mq.md",
            "target": {"kind": "entry"},
            "analysis_digest": format!("sha256:{}", "a".repeat(64)),
            "mode": "pure",
            "client_request_id": "req-1"
        });
        let mut with_unknown = base.clone();
        with_unknown["cwd"] = serde_json::json!("/tmp");
        assert!(validate_request(with_unknown.to_string().as_bytes()).is_err());
        let mut strict = base.clone();
        strict["mode"] = serde_json::json!("strict");
        assert_eq!(
            validate_request(strict.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "view.run_denied"
        );
        let mut project_without_globs = base;
        project_without_globs["mode"] = serde_json::json!("project");
        assert!(validate_request(project_without_globs.to_string().as_bytes()).is_ok());
    }

    #[test]
    fn rejects_absolute_and_parent_globs() {
        let request = serde_json::json!({
            "document": "index.mq.md",
            "target": {"kind": "entry"},
            "analysis_digest": format!("sha256:{}", "a".repeat(64)),
            "mode": "project",
            "grants": {"read": ["../secret"], "write": []},
            "client_request_id": "req-1"
        });
        assert!(validate_request(request.to_string().as_bytes()).is_err());
        let request = serde_json::json!({
            "document": "index.mq.md",
            "target": {"kind": "entry"},
            "analysis_digest": format!("sha256:{}", "a".repeat(64)),
            "mode": "project",
            "grants": {"read": ["/etc/passwd"], "write": []},
            "client_request_id": "req-1"
        });
        assert!(validate_request(request.to_string().as_bytes()).is_err());
    }

    #[test]
    fn canonical_names_and_targets_resolve() {
        let projection = projection();
        let request = serde_json::json!({
            "document": "index.mq.md",
            "target": {"kind": "function", "function_id": "root:t@0.1.0::index.mq.md::N.f"},
            "parameters": [{"name": "n", "expression": "5"}],
            "analysis_digest": format!("sha256:{}", projection.analysis_sha256),
            "mode": "pure",
            "client_request_id": "req-2"
        });
        let validated = validate_request(request.to_string().as_bytes()).expect("valid");
        let plan = validated.plan_against(&projection).expect("plan");
        match plan.target {
            ExecutionTarget::Function { cli_name, .. } => assert_eq!(cli_name, "f"),
            ExecutionTarget::Entry => panic!("expected function"),
        }
        assert_eq!(plan.arguments, vec![("n".to_string(), "5".to_string())]);
    }

    #[test]
    fn entry_without_main_is_explicitly_rejected() {
        let mut projection = projection();
        projection
            .functions
            .retain(|function| function.local_name != "main");
        let request = serde_json::json!({
            "document": "index.mq.md",
            "target": {"kind": "entry"},
            "analysis_digest": format!("sha256:{}", projection.analysis_sha256),
            "mode": "pure",
            "client_request_id": "req-3"
        });
        let validated = validate_request(request.to_string().as_bytes()).expect("valid");
        assert_eq!(
            validated.plan_against(&projection).unwrap_err().code(),
            "view.no_entry_function"
        );
    }

    #[test]
    fn unknown_parameter_names_are_rejected_before_execution() {
        let projection = projection();
        let request = serde_json::json!({
            "document": "index.mq.md",
            "target": {"kind": "function", "function_id": "root:t@0.1.0::index.mq.md::N.f"},
            "parameters": [{"name": "bogus", "expression": "5"}],
            "analysis_digest": format!("sha256:{}", projection.analysis_sha256),
            "mode": "pure",
            "client_request_id": "req-4"
        });
        let validated = validate_request(request.to_string().as_bytes()).expect("valid");
        assert_eq!(
            validated.plan_against(&projection).unwrap_err().code(),
            "view.run_invalid"
        );
    }
}
