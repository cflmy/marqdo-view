//! Analysis v2 validation and static semantic projection.
//!
//! This module is the only consumer of the public Analysis JSON. It verifies
//! protocol identity first, then projects functions/units/statements/diagnostics
//! into View-owned types with **validated** source spans. A span that is out of
//! range or not on a UTF-8 boundary is dropped with an explicit View warning
//! instead of being pointed at nearby text (RFC-VIEW-0001 §4.3, §8.2).

use crate::error::{ViewError, ViewResult};
use crate::reader::source::{sha256_hex, sha256_prefixed, SourceIndex, SourceSpan};
use serde::Deserialize;
use serde_json::Value;

/// Stable DOM identity for one source-bound annotation, local to this document
/// and run. It is not a cross-edit identity.
pub fn source_id(module_id: &str, span: SourceSpan, kind: &str) -> String {
    let material = format!("{module_id}\0{}:{}:{}", span.start, span.end, kind);
    sha256_hex(material.as_bytes())[..16].to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Root,
    Namespace,
    Function,
    Knowledge,
    Section,
    Other,
}

#[derive(Debug, Clone)]
pub struct SemanticUnit {
    pub id: String,
    pub name: String,
    pub level: u8,
    pub kind: UnitKind,
    pub function: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct FunctionParam {
    pub name: String,
    pub ty: String,
    pub required: bool,
    pub default: Option<String>,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub kind: String,
    pub params: Vec<FunctionParam>,
}

#[derive(Debug, Clone)]
pub struct SemanticFunction {
    pub id: String,
    pub unit_id: String,
    pub local_name: String,
    pub namespace: Option<String>,
    pub span: SourceSpan,
    pub heading_span: SourceSpan,
    pub signature: FunctionSignature,
    pub effects: Vec<String>,
    pub statements: Vec<SemanticStatement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Assign,
    Expression,
    Branch,
    Loop,
    Return,
    Other,
}

impl StatementKind {
    pub fn label(&self) -> &'static str {
        match self {
            StatementKind::Assign => "赋值",
            StatementKind::Expression => "表达式",
            StatementKind::Branch => "条件分支",
            StatementKind::Loop => "循环",
            StatementKind::Return => "返回",
            StatementKind::Other => "语句",
        }
    }

    pub fn data_value(&self) -> &'static str {
        match self {
            StatementKind::Assign => "assign",
            StatementKind::Expression => "expression",
            StatementKind::Branch => "branch",
            StatementKind::Loop => "loop",
            StatementKind::Return => "return",
            StatementKind::Other => "other",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SemanticStatement {
    pub kind: StatementKind,
    /// Header span for control flow, payload span for simple statements.
    pub span: SourceSpan,
    /// For `if` arms and `while` conditions: the condition text span as the
    /// compiler recorded it.
    pub condition_span: Option<SourceSpan>,
    pub arms: Vec<SemanticArm>,
    pub body: Vec<SemanticStatement>,
    /// Target name for assignments (local name or `index` form), for static
    /// outline display only. Never a runtime value.
    pub target: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SemanticArm {
    pub span: Option<SourceSpan>,
    pub condition_span: Option<SourceSpan>,
    pub body: Vec<SemanticStatement>,
}

impl SemanticFunction {
    /// Depth-first statement walk in source order.
    pub fn flattened_statements(&self) -> Vec<&SemanticStatement> {
        let mut out = Vec::new();
        for statement in &self.statements {
            collect_statements(statement, &mut out);
        }
        out
    }
}

fn collect_statements<'a>(statement: &'a SemanticStatement, out: &mut Vec<&'a SemanticStatement>) {
    out.push(statement);
    for arm in &statement.arms {
        for child in &arm.body {
            collect_statements(child, out);
        }
    }
    for child in &statement.body {
        collect_statements(child, out);
    }
}

#[derive(Debug, Clone)]
pub struct StaticResource {
    pub id: String,
    pub kind: String,
    pub span: SourceSpan,
    pub content_digest: String,
    pub language: Option<String>,
    pub display: bool,
}

#[derive(Debug, Clone)]
pub struct StaticImport {
    pub alias: String,
    pub target: String,
    pub resolved: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct StaticReference {
    pub url: String,
    pub span: SourceSpan,
    pub state: String,
    pub target_module: Option<String>,
    pub target_unit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StaticCall {
    pub state: String,
    pub target: Option<String>,
    pub function: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct CoreDiagnostic {
    pub module: Option<String>,
    pub relative_path: Option<String>,
    pub span: Option<SourceSpan>,
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub severity: String,
    pub code: String,
    pub message: String,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ViewWarning {
    pub code: String,
    pub message: String,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone)]
pub struct ModuleSummary {
    pub id: String,
    pub package_instance_id: String,
    pub relative_path: String,
    pub source_digest: String,
}

#[derive(Debug, Clone)]
pub struct SemanticProjection {
    pub analysis_sha256: String,
    pub analysis_digest: String,
    pub producer_version: String,
    pub markdown_profile: String,
    pub project_root_id: String,
    pub identity_kind: String,
    pub lock_digest: String,
    pub stdlib_version: Option<String>,
    pub module_id: String,
    pub package_instance_id: String,
    pub relative_path: String,
    pub source_digest: String,
    pub units: Vec<SemanticUnit>,
    pub functions: Vec<SemanticFunction>,
    pub resources: Vec<StaticResource>,
    pub imports: Vec<StaticImport>,
    pub references: Vec<StaticReference>,
    pub calls: Vec<StaticCall>,
    pub diagnostics: Vec<CoreDiagnostic>,
    pub warnings: Vec<ViewWarning>,
    pub modules: Vec<ModuleSummary>,
}

#[derive(Debug)]
pub struct VerifiedAnalysis {
    pub projection: SemanticProjection,
}

impl SemanticProjection {
    pub fn unit_for_heading(&self, span: SourceSpan, level: u8) -> Option<&SemanticUnit> {
        self.units
            .iter()
            .filter(|unit| unit.level == level && unit.span.contains(span))
            .min_by_key(|unit| unit.span.len())
    }

    pub fn function_by_heading(&self, span: SourceSpan) -> Option<&SemanticFunction> {
        self.functions
            .iter()
            .find(|function| function.heading_span == span)
            .or_else(|| {
                self.functions
                    .iter()
                    .find(|function| function.span.contains(span))
            })
    }

    /// All statements of all functions, flattened in deterministic order.
    pub fn all_statements(&self) -> Vec<(&SemanticFunction, &SemanticStatement)> {
        let mut out = Vec::new();
        for function in &self.functions {
            for statement in function.flattened_statements() {
                out.push((function, statement));
            }
        }
        out
    }

    /// The innermost statement whose span is contained in `inline_span`.
    pub fn statement_for_inline(
        &self,
        inline_span: SourceSpan,
    ) -> Option<(&SemanticFunction, &SemanticStatement)> {
        self.all_statements()
            .into_iter()
            .filter(|(_, statement)| {
                statement.span.start >= inline_span.start
                    && statement.span.end <= inline_span.end
                    && !statement.span.is_empty()
            })
            .max_by_key(|(_, statement)| statement.span.start)
    }

    pub fn resource_for_span(&self, span: SourceSpan) -> Option<&StaticResource> {
        self.resources
            .iter()
            .find(|resource| resource.span == span || resource.span.contains(span))
    }

    pub fn reference_for_span(&self, span: SourceSpan) -> Option<&StaticReference> {
        self.references
            .iter()
            .find(|reference| reference.span == span || reference.span.contains(span))
    }
}

// ---------------------------------------------------------------------------
// Raw protocol types (additive fields are tolerated and ignored).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawAnalysis {
    schema: String,
    schema_version: u32,
    producer: RawProducer,
    project: RawProject,
    modules: Vec<RawModule>,
    #[serde(default)]
    diagnostics: Vec<RawDiagnostic>,
}

#[derive(Debug, Deserialize)]
struct RawProducer {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    markdown_profile: String,
}

#[derive(Debug, Deserialize)]
struct RawProject {
    #[serde(default)]
    root_id: String,
    #[serde(default)]
    identity_kind: String,
    #[serde(default)]
    lock_digest: String,
    #[serde(default)]
    stdlib: Option<RawStdlib>,
}

#[derive(Debug, Deserialize)]
struct RawStdlib {
    #[serde(default)]
    version: String,
}

#[derive(Debug, Deserialize)]
struct RawModule {
    #[serde(default)]
    id: String,
    #[serde(default)]
    package_instance_id: String,
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    source_digest: String,
    #[serde(default)]
    units: Vec<RawUnit>,
    #[serde(default)]
    functions: Vec<RawFunction>,
    #[serde(default)]
    imports: Vec<RawImport>,
    #[serde(default)]
    references: Vec<RawReference>,
    #[serde(default)]
    calls: Vec<RawCall>,
    #[serde(default)]
    resources: Vec<RawResource>,
}

#[derive(Debug, Deserialize)]
struct RawUnit {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    level: u8,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    function: Option<String>,
    #[serde(default)]
    span: Option<SourceSpan>,
}

#[derive(Debug, Deserialize)]
struct RawFunction {
    #[serde(default)]
    id: String,
    #[serde(default)]
    unit_id: String,
    #[serde(default)]
    local_name: String,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    span: Option<SourceSpan>,
    #[serde(default)]
    heading_span: Option<SourceSpan>,
    #[serde(default)]
    signature: Option<RawSignature>,
    #[serde(default)]
    effects: Vec<String>,
    #[serde(default)]
    body: Vec<RawStatement>,
}

#[derive(Debug, Deserialize)]
struct RawSignature {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    params: Vec<RawParam>,
}

#[derive(Debug, Deserialize)]
struct RawParam {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    ty: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<Value>,
    #[serde(default)]
    span: Option<SourceSpan>,
}

#[derive(Debug, Deserialize)]
struct RawStatement {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    span: Option<SourceSpan>,
    #[serde(default)]
    condition_span: Option<SourceSpan>,
    #[serde(default)]
    arms: Vec<RawArm>,
    #[serde(default)]
    body: Vec<RawStatement>,
    #[serde(default)]
    target: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawArm {
    #[serde(default)]
    span: Option<SourceSpan>,
    #[serde(default)]
    condition_span: Option<SourceSpan>,
    #[serde(default)]
    body: Vec<RawStatement>,
}

#[derive(Debug, Deserialize)]
struct RawResource {
    #[serde(default)]
    id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    span: Option<SourceSpan>,
    #[serde(default)]
    content_digest: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    display: bool,
}

#[derive(Debug, Deserialize)]
struct RawImport {
    #[serde(default)]
    alias: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    resolved: Option<String>,
    #[serde(default)]
    span: Option<SourceSpan>,
}

#[derive(Debug, Deserialize)]
struct RawReference {
    #[serde(default)]
    url: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    target_module: Option<String>,
    #[serde(default)]
    target_unit: Option<String>,
    #[serde(default)]
    span: Option<SourceSpan>,
}

#[derive(Debug, Deserialize)]
struct RawCall {
    #[serde(default)]
    state: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    function: Option<String>,
    #[serde(default)]
    span: Option<SourceSpan>,
}

#[derive(Debug, Deserialize)]
struct RawDiagnostic {
    #[serde(default)]
    module: Option<String>,
    #[serde(default)]
    relative_path: Option<String>,
    #[serde(default)]
    span: Option<SourceSpan>,
    #[serde(default)]
    line: Option<u64>,
    #[serde(default)]
    column: Option<u64>,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    hints: Vec<String>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Verify Analysis bytes against the exact source and project the static facts.
///
/// Rejects protocol incompatibility and source-digest mismatch as errors; drops
/// individual invalid spans as warnings so the prose stays readable.
pub fn verify_analysis(
    analysis_bytes: &[u8],
    source: &str,
    source_index: &SourceIndex,
    expected_relative_path: &str,
) -> ViewResult<VerifiedAnalysis> {
    let raw: RawAnalysis =
        serde_json::from_slice(analysis_bytes).map_err(|err| ViewError::AnalysisFailed {
            detail: format!("Analysis v2 is not valid JSON: {err}"),
        })?;
    if raw.schema != "marqdo.analysis" {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!("unexpected Analysis schema {:?}", raw.schema),
        });
    }
    if raw.schema_version != crate::protocol::tool_v1::ANALYSIS_PROTOCOL {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!(
                "Analysis schema_version {} is not supported (View implements v{})",
                raw.schema_version,
                crate::protocol::tool_v1::ANALYSIS_PROTOCOL
            ),
        });
    }
    if raw.producer.name != "marqdo" {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!(
                "Analysis producer.name {:?} is not `marqdo`",
                raw.producer.name
            ),
        });
    }
    if raw.producer.markdown_profile != "marqdo.md.v1" {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!(
                "unexpected Markdown profile {:?}; this View targets marqdo.md.v1",
                raw.producer.markdown_profile
            ),
        });
    }
    if raw.project.root_id.is_empty() {
        return Err(ViewError::AnalysisFailed {
            detail: "Analysis project.root_id is empty".to_string(),
        });
    }

    let actual_digest = sha256_hex(source.as_bytes());
    let module = raw
        .modules
        .iter()
        .find(|module| module.relative_path == expected_relative_path)
        .ok_or_else(|| ViewError::AnalysisFailed {
            detail: format!("Analysis has no module for {expected_relative_path}"),
        })?;
    if module.source_digest != actual_digest {
        return Err(ViewError::SourceMismatch {
            expected: module.source_digest.clone(),
            actual: actual_digest,
        });
    }

    let mut warnings = Vec::new();
    let mut valid_span = |span: Option<SourceSpan>, what: &str| -> Option<SourceSpan> {
        let span = span?;
        if source_index.validate_span_bytes(source.as_bytes(), span) {
            Some(span)
        } else {
            warnings.push(ViewWarning {
                code: "view.source_span_invalid".to_string(),
                message: format!("{what} 的字节区间超出源码或不在 UTF-8 边界，已跳过该标注"),
                span: Some(span),
            });
            None
        }
    };

    let units: Vec<SemanticUnit> = module
        .units
        .iter()
        .filter_map(|unit| {
            let span = valid_span(unit.span, "语义单元")?;
            Some(SemanticUnit {
                id: unit.id.clone(),
                name: unit.name.clone(),
                level: unit.level,
                kind: match unit.kind.as_str() {
                    "root" => UnitKind::Root,
                    "namespace" => UnitKind::Namespace,
                    "function" => UnitKind::Function,
                    "knowledge" => UnitKind::Knowledge,
                    "section" => UnitKind::Section,
                    _ => UnitKind::Other,
                },
                function: unit.function.clone(),
                span,
            })
        })
        .collect();

    let functions: Vec<SemanticFunction> = module
        .functions
        .iter()
        .filter_map(|function| {
            let span = valid_span(function.span, "函数范围")?;
            let heading_span = valid_span(function.heading_span, "函数标题")?;
            let statements = function
                .body
                .iter()
                .filter_map(|statement| convert_statement(statement, &mut valid_span))
                .collect();
            Some(SemanticFunction {
                id: function.id.clone(),
                unit_id: function.unit_id.clone(),
                local_name: function.local_name.clone(),
                namespace: function.namespace.clone(),
                span,
                heading_span,
                signature: FunctionSignature {
                    kind: function
                        .signature
                        .as_ref()
                        .map(|signature| signature.kind.clone())
                        .unwrap_or_default(),
                    params: function
                        .signature
                        .as_ref()
                        .map(|signature| {
                            signature
                                .params
                                .iter()
                                .map(|param| FunctionParam {
                                    name: param.name.clone(),
                                    ty: param.ty.clone().unwrap_or_default(),
                                    required: param.required,
                                    default: param.default.as_ref().map(compact_value),
                                    span: param.span.filter(|span| {
                                        source_index.validate_span_bytes(source.as_bytes(), *span)
                                    }),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                },
                effects: function.effects.clone(),
                statements,
            })
        })
        .collect();

    let resources: Vec<StaticResource> = module
        .resources
        .iter()
        .filter(|resource| matches!(resource.kind.as_str(), "formula" | "code"))
        .filter_map(|resource| {
            let span = valid_span(resource.span, "资源")?;
            Some(StaticResource {
                id: resource.id.clone(),
                kind: resource.kind.clone(),
                span,
                content_digest: resource.content_digest.clone(),
                language: resource.language.clone(),
                display: resource.display,
            })
        })
        .collect();

    let imports: Vec<StaticImport> = module
        .imports
        .iter()
        .filter_map(|import| {
            Some(StaticImport {
                alias: import.alias.clone(),
                target: import.target.clone(),
                resolved: import.resolved.clone(),
                span: valid_span(import.span, "导入声明")?,
            })
        })
        .collect();

    let references: Vec<StaticReference> = module
        .references
        .iter()
        .filter_map(|reference| {
            Some(StaticReference {
                url: reference.url.clone(),
                state: reference.state.clone(),
                target_module: reference.target_module.clone(),
                target_unit: reference.target_unit.clone(),
                span: valid_span(reference.span, "Markdown 引用")?,
            })
        })
        .collect();

    let calls: Vec<StaticCall> = module
        .calls
        .iter()
        .filter_map(|call| {
            Some(StaticCall {
                state: call.state.clone(),
                target: call.target.clone(),
                function: call.function.clone(),
                span: valid_span(call.span, "调用点")?,
            })
        })
        .collect();

    let diagnostics: Vec<CoreDiagnostic> = raw
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let span = diagnostic
                .span
                .filter(|span| source_index.validate_span_bytes(source.as_bytes(), *span));
            CoreDiagnostic {
                module: diagnostic.module.clone(),
                relative_path: diagnostic.relative_path.clone(),
                span,
                line: diagnostic.line,
                column: diagnostic.column,
                severity: if diagnostic.severity.is_empty() {
                    "error".to_string()
                } else {
                    diagnostic.severity.clone()
                },
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                hints: diagnostic.hints.clone(),
            }
        })
        .collect();

    let modules = raw
        .modules
        .iter()
        .map(|module| ModuleSummary {
            id: module.id.clone(),
            package_instance_id: module.package_instance_id.clone(),
            relative_path: module.relative_path.clone(),
            source_digest: module.source_digest.clone(),
        })
        .collect();

    Ok(VerifiedAnalysis {
        projection: SemanticProjection {
            analysis_sha256: sha256_hex(analysis_bytes),
            analysis_digest: sha256_prefixed(analysis_bytes),
            producer_version: raw.producer.version.clone(),
            markdown_profile: raw.producer.markdown_profile.clone(),
            project_root_id: raw.project.root_id.clone(),
            identity_kind: raw.project.identity_kind.clone(),
            lock_digest: raw.project.lock_digest.clone(),
            stdlib_version: raw.project.stdlib.as_ref().map(|s| s.version.clone()),
            module_id: module.id.clone(),
            package_instance_id: module.package_instance_id.clone(),
            relative_path: module.relative_path.clone(),
            source_digest: module.source_digest.clone(),
            units,
            functions,
            resources,
            imports,
            references,
            calls,
            diagnostics,
            warnings,
            modules,
        },
    })
}

fn convert_statement(
    raw: &RawStatement,
    valid_span: &mut impl FnMut(Option<SourceSpan>, &str) -> Option<SourceSpan>,
) -> Option<SemanticStatement> {
    let span = valid_span(raw.span, "语句")?;
    let kind = match raw.kind.as_str() {
        "assign" => StatementKind::Assign,
        "expression" => StatementKind::Expression,
        "branch" => StatementKind::Branch,
        "loop" => StatementKind::Loop,
        "return" => StatementKind::Return,
        _ => StatementKind::Other,
    };
    let body = raw
        .body
        .iter()
        .filter_map(|child| convert_statement(child, valid_span))
        .collect();
    let arms = raw
        .arms
        .iter()
        .map(|arm| SemanticArm {
            span: valid_span(arm.span, "分支臂"),
            condition_span: valid_span(arm.condition_span, "分支条件"),
            body: arm
                .body
                .iter()
                .filter_map(|child| convert_statement(child, valid_span))
                .collect(),
        })
        .collect();
    Some(SemanticStatement {
        kind,
        span,
        condition_span: valid_span(raw.condition_span, "控制条件"),
        arms,
        body,
        target: raw.target.as_ref().and_then(assignment_target_label),
    })
}

fn assignment_target_label(target: &Value) -> Option<String> {
    match target {
        Value::String(name) => Some(name.clone()),
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("index") {
                Some("索引赋值".to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn compact_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => {
            let text = other.to_string();
            if text.chars().count() > 40 {
                format!("{}…", text.chars().take(40).collect::<String>())
            } else {
                text
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysis(schema_version: u32, digest: &str) -> Vec<u8> {
        serde_json::json!({
            "schema": "marqdo.analysis",
            "schema_version": schema_version,
            "producer": {"name": "marqdo", "version": "3.0.0-beta.1", "markdown_profile": "marqdo.md.v1"},
            "project": {"root_id": "root:t@0.1.0", "identity_kind": "manifest", "lock_digest": "", "stdlib": null},
            "modules": [{
                "id": "root:t@0.1.0::index.mq.md",
                "package_instance_id": "root:t@0.1.0",
                "relative_path": "index.mq.md",
                "source_digest": digest,
                "units": [],
                "functions": [],
                "imports": [],
                "references": [],
                "calls": [],
                "resources": []
            }],
            "diagnostics": []
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn rejects_unknown_schema_major_and_profile() {
        let source = "# hi\n";
        let index = SourceIndex::new(source);
        let digest = sha256_hex(source.as_bytes());
        let err =
            verify_analysis(&analysis(3, &digest), source, &index, "index.mq.md").unwrap_err();
        assert_eq!(err.code(), "view.protocol_incompatible");
    }

    #[test]
    fn rejects_digest_mismatch() {
        let source = "# hi\n";
        let index = SourceIndex::new(source);
        let err = verify_analysis(&analysis(2, &"0".repeat(64)), source, &index, "index.mq.md")
            .unwrap_err();
        assert_eq!(err.code(), "view.source_mismatch");
    }

    #[test]
    fn projects_valid_analysis() {
        let source = "# hi\n";
        let index = SourceIndex::new(source);
        let digest = sha256_hex(source.as_bytes());
        let verified =
            verify_analysis(&analysis(2, &digest), source, &index, "index.mq.md").unwrap();
        assert_eq!(verified.projection.project_root_id, "root:t@0.1.0");
        assert_eq!(verified.projection.relative_path, "index.mq.md");
    }

    #[test]
    fn source_id_is_deterministic_and_bounded() {
        let span = SourceSpan::new(1, 5);
        let id = source_id("mod", span, "statement");
        assert_eq!(id.len(), 16);
        assert_eq!(id, source_id("mod", span, "statement"));
        assert_ne!(id, source_id("mod", span, "resource"));
    }
}
