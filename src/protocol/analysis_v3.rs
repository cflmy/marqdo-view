//! Public Analysis v3 consumer (`marqdo.analysis`, ADR-0069).
//!
//! This module is the only consumer of the published `check --emit` artifact.
//! The artifact carries the exact source, the Markdown AST, heading scopes,
//! anchors, functions, imports, calls, resources, reference states and
//! diagnostics from one static preparation. There is exactly one accepted
//! revision-3 shape: a retired revision-3 artifact (missing `source`,
//! `document`, `anchors`, `entry_module_id`, ...) is rejected with an
//! actionable error, never decoded through a fallback.
//!
//! Validation happens on the serialized bytes: schema identity, closed record
//! shapes, source digests, UTF-8 half-open spans, unit tree invariants,
//! function/unit links, reference state/nullability, call targets, resource
//! digests and diagnostic ownership. The projection keeps only data the
//! reader/renderer/execution layers consume; it contains no CLI argv details
//! and no version branch.

use crate::error::{ViewError, ViewResult};
use crate::reader::source::{sha256_hex, sha256_prefixed, SourceSpan};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
}

impl UnitKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "root" => Some(UnitKind::Root),
            "namespace" => Some(UnitKind::Namespace),
            "function" => Some(UnitKind::Function),
            "knowledge" => Some(UnitKind::Knowledge),
            "section" => Some(UnitKind::Section),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Exec,
    Knowledge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeOrigin {
    Default,
    Explicit,
    Inherited,
}

#[derive(Debug, Clone)]
pub struct SemanticUnit {
    pub id: String,
    /// Canonical structured heading path; the root has `[]`.
    pub path: Vec<String>,
    pub parent_id: Option<String>,
    pub name: String,
    pub level: u8,
    pub kind: UnitKind,
    pub mode: Mode,
    pub mode_origin: ModeOrigin,
    pub mode_source_unit_id: String,
    /// Original heading AST span; `None` only for the root.
    pub heading_span: Option<SourceSpan>,
    /// Complete section range: heading start to the next same-or-higher
    /// heading or EOF. The root covers the whole source.
    pub span: SourceSpan,
    pub function: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Anchor {
    pub anchor: String,
    pub unit_id: String,
}

#[derive(Debug, Clone)]
pub struct FunctionParam {
    pub name: String,
    pub ty: Option<String>,
    pub required: bool,
    pub default: Option<Value>,
    pub span: SourceSpan,
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
    Break,
    Continue,
}

impl StatementKind {
    pub fn label(&self) -> &'static str {
        match self {
            StatementKind::Assign => "赋值",
            StatementKind::Expression => "表达式",
            StatementKind::Branch => "条件分支",
            StatementKind::Loop => "循环",
            StatementKind::Return => "返回",
            StatementKind::Break => "跳出循环",
            StatementKind::Continue => "继续循环",
        }
    }

    pub fn data_value(&self) -> &'static str {
        match self {
            StatementKind::Assign => "assign",
            StatementKind::Expression => "expression",
            StatementKind::Branch => "branch",
            StatementKind::Loop => "loop",
            StatementKind::Return => "return",
            StatementKind::Break => "break",
            StatementKind::Continue => "continue",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SemanticStatement {
    pub kind: StatementKind,
    /// Header span for control flow, payload span for simple statements.
    pub span: SourceSpan,
    /// For `if` arms and `while` conditions: the recorded condition span.
    pub condition_span: Option<SourceSpan>,
    pub arms: Vec<SemanticArm>,
    pub body: Vec<SemanticStatement>,
    /// Assign target display label (a local name or `索引赋值`).
    pub target: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SemanticArm {
    pub span: SourceSpan,
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

/// Closed Markdown AST node produced by the public projector.
#[derive(Debug, Clone)]
pub struct DocumentNode {
    pub kind: String,
    pub span: SourceSpan,
    pub children: Vec<DocumentNode>,
    pub literal: Option<String>,
    pub level: Option<u8>,
    pub setext: Option<bool>,
    pub ordered: Option<bool>,
    pub start: Option<u64>,
    pub tight: Option<bool>,
    pub task: Option<bool>,
    pub ticks: Option<u64>,
    pub fenced: Option<bool>,
    pub info: Option<String>,
    pub display: Option<bool>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub alignments: Option<Vec<String>>,
    pub header: Option<bool>,
    pub tag: Option<String>,
}

impl DocumentNode {
    /// Depth-first iterator over this node and its descendants.
    pub fn walk(&self) -> Vec<&DocumentNode> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect<'a>(&'a self, out: &mut Vec<&'a DocumentNode>) {
        out.push(self);
        for child in &self.children {
            child.collect(out);
        }
    }
}

#[derive(Debug, Clone)]
pub struct StaticResource {
    pub id: String,
    pub module_id: String,
    pub unit_id: String,
    pub kind: String,
    pub span: SourceSpan,
    pub content_digest: String,
    pub language: Option<String>,
    /// `Some` for formulas, `None` for code.
    pub display: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct StaticImport {
    pub alias: String,
    pub target: String,
    pub resolved: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceState {
    Resolved,
    NotLoaded,
    Missing,
    Ambiguous,
    External,
    OutsideScope,
    Invalid,
}

impl ReferenceState {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "resolved" => Some(ReferenceState::Resolved),
            "not_loaded" => Some(ReferenceState::NotLoaded),
            "missing" => Some(ReferenceState::Missing),
            "ambiguous" => Some(ReferenceState::Ambiguous),
            "external" => Some(ReferenceState::External),
            "outside_scope" => Some(ReferenceState::OutsideScope),
            "invalid" => Some(ReferenceState::Invalid),
            _ => None,
        }
    }

    pub fn data_value(&self) -> &'static str {
        match self {
            ReferenceState::Resolved => "resolved",
            ReferenceState::NotLoaded => "not_loaded",
            ReferenceState::Missing => "missing",
            ReferenceState::Ambiguous => "ambiguous",
            ReferenceState::External => "external",
            ReferenceState::OutsideScope => "outside_scope",
            ReferenceState::Invalid => "invalid",
        }
    }

    /// Only these states are actionable errors for the reader.
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            ReferenceState::Missing
                | ReferenceState::Ambiguous
                | ReferenceState::OutsideScope
                | ReferenceState::Invalid
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceTargetKind {
    Module,
    File,
}

#[derive(Debug, Clone)]
pub struct StaticReference {
    pub source: String,
    pub source_unit: Option<String>,
    pub url: String,
    pub span: SourceSpan,
    pub state: ReferenceState,
    pub target_kind: Option<ReferenceTargetKind>,
    pub target_module: Option<String>,
    pub target_unit: Option<String>,
    pub candidate_package_instance_id: Option<String>,
    pub candidate_path: Option<String>,
    pub fragment: Option<String>,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallState {
    Function,
    Builtin,
    Plugin,
    Unresolved,
}

impl CallState {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "function" => Some(CallState::Function),
            "builtin" => Some(CallState::Builtin),
            "plugin" => Some(CallState::Plugin),
            "unresolved" => Some(CallState::Unresolved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CallTargetOwner {
    pub kind: String,
    pub package: String,
    pub digest: String,
}

#[derive(Debug, Clone)]
pub struct StaticCall {
    pub function: String,
    pub span: SourceSpan,
    pub callee: String,
    pub state: CallState,
    pub target: Option<String>,
    pub target_kind: Option<String>,
    pub target_owner: Option<CallTargetOwner>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CoreDiagnostic {
    pub module: String,
    pub relative_path: String,
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
    /// Exact source text carried by the artifact, so no consumer reopens a
    /// package-cache or checkout path to reconstruct spans.
    pub source: String,
}

impl ModuleSummary {
    pub fn source_len(&self) -> usize {
        self.source.len()
    }
}

#[derive(Debug, Clone)]
pub struct ExtensionIdentity {
    pub kind: String,
    pub name: String,
    pub version: String,
    pub logical_owner: String,
    pub source: String,
    pub manifest_digest: String,
    pub artifact_digest: String,
    pub selected_target: String,
}

#[derive(Debug, Clone)]
pub struct SemanticProjection {
    pub schema_version: u32,
    pub analysis_sha256: String,
    pub analysis_digest: String,
    pub producer_version: String,
    pub markdown_profile: String,
    pub project_root_id: String,
    pub identity_kind: String,
    pub entry_module_id: String,
    pub lock_digest: String,
    pub manifest_digest: String,
    pub stdlib_version: Option<String>,
    pub stdlib_digest: Option<String>,
    pub stdlib_instance_id: Option<String>,
    pub extensions: Vec<ExtensionIdentity>,
    pub module_id: String,
    pub package_instance_id: String,
    pub relative_path: String,
    pub source_digest: String,
    /// Exact entry module source from the artifact.
    pub source: String,
    pub document: DocumentNode,
    pub anchors: Vec<Anchor>,
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

impl SemanticProjection {
    pub fn module(&self, id: &str) -> Option<&ModuleSummary> {
        self.modules.iter().find(|module| module.id == id)
    }

    pub fn unit(&self, id: &str) -> Option<&SemanticUnit> {
        self.units.iter().find(|unit| unit.id == id)
    }

    pub fn function(&self, id: &str) -> Option<&SemanticFunction> {
        self.functions.iter().find(|function| function.id == id)
    }

    pub fn unit_for_heading(&self, span: SourceSpan, level: u8) -> Option<&SemanticUnit> {
        self.units
            .iter()
            .filter(|unit| unit.level == level && unit.heading_span == Some(span))
            .min_by_key(|unit| unit.span.len())
    }

    pub fn function_by_heading(&self, span: SourceSpan) -> Option<&SemanticFunction> {
        self.functions
            .iter()
            .find(|function| function.heading_span == span)
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

    pub fn entry_module(&self) -> Option<&ModuleSummary> {
        self.module(&self.entry_module_id)
    }
}

#[derive(Debug)]
pub struct VerifiedAnalysis {
    pub projection: SemanticProjection,
}

// ---------------------------------------------------------------------------
// Raw protocol types. Every record is closed (`deny_unknown_fields`); nullable
// fields are read as `Value` so a *missing* required key is an error while an
// explicit `null` is accepted where the schema allows it.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnalysis {
    schema: String,
    schema_version: u32,
    producer: RawProducer,
    project: RawProject,
    modules: Vec<RawModule>,
    diagnostics: Vec<RawDiagnostic>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProducer {
    name: String,
    version: String,
    markdown_profile: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    root_id: String,
    identity_kind: String,
    entry_module_id: String,
    lock_digest: String,
    manifest_digest: String,
    stdlib: Value,
    extensions: Vec<RawExtension>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExtension {
    kind: String,
    name: String,
    version: String,
    logical_owner: String,
    source: String,
    manifest_digest: String,
    artifact_digest: String,
    selected_target: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStdlib {
    version: String,
    digest: String,
    instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModule {
    id: String,
    package_instance_id: String,
    relative_path: String,
    source_digest: String,
    source: String,
    document: RawNode,
    units: Vec<RawUnit>,
    anchors: Vec<RawAnchor>,
    functions: Vec<RawFunction>,
    imports: Vec<RawImport>,
    references: Vec<RawReference>,
    calls: Vec<RawCall>,
    resources: Vec<RawResource>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSpan {
    start: u64,
    end: u64,
}

impl RawSpan {
    fn to_span(&self) -> SourceSpan {
        SourceSpan::new(self.start, self.end)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNode {
    kind: String,
    span: RawSpan,
    children: Vec<RawNode>,
    level: Option<u8>,
    setext: Option<bool>,
    ordered: Option<bool>,
    start: Option<u64>,
    tight: Option<bool>,
    task: Option<bool>,
    ticks: Option<u64>,
    fenced: Option<bool>,
    info: Option<String>,
    display: Option<bool>,
    url: Option<String>,
    title: Option<String>,
    alignments: Option<Vec<String>>,
    header: Option<bool>,
    tag: Option<String>,
    literal: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUnit {
    id: String,
    path: Vec<String>,
    parent_id: Value,
    kind: String,
    name: String,
    level: u8,
    mode: String,
    mode_origin: String,
    mode_source_unit_id: String,
    heading_span: Value,
    span: RawSpan,
    function: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnchor {
    anchor: String,
    unit_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFunction {
    id: String,
    unit_id: String,
    local_name: String,
    namespace: Value,
    span: RawSpan,
    heading_span: RawSpan,
    signature: RawSignature,
    effects: Vec<String>,
    body: Vec<RawStatement>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSignature {
    kind: String,
    params: Vec<RawParam>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawParam {
    name: String,
    required: bool,
    #[serde(rename = "type")]
    ty: Value,
    default: Value,
    span: RawSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStatement {
    kind: String,
    span: RawSpan,
    #[serde(default)]
    condition_span: Option<RawSpan>,
    #[serde(default)]
    target: Option<Value>,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    condition: Option<Value>,
    #[serde(default)]
    arms: Vec<RawArm>,
    #[serde(default)]
    body: Vec<RawStatement>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArm {
    condition: Value,
    condition_span: RawSpan,
    body: Vec<RawStatement>,
    span: RawSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExpression {
    kind: String,
    span: RawSpan,
    #[serde(rename = "type")]
    ty: Option<String>,
    value: Option<Value>,
    name: Option<String>,
    items: Option<Vec<RawExpression>>,
    entries: Option<Vec<RawEntry>>,
    op: Option<String>,
    operand: Option<Box<RawExpression>>,
    left: Option<Box<RawExpression>>,
    right: Option<Box<RawExpression>>,
    base: Option<Box<RawExpression>>,
    field: Option<String>,
    index: Option<Box<RawExpression>>,
    callee: Option<Box<RawExpression>>,
    args: Option<Vec<RawArg>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    key: String,
    key_span: RawSpan,
    value: RawExpression,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArg {
    name: Option<String>,
    value: RawExpression,
    span: RawSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCallee {
    kind: String,
    name: Option<String>,
    base: Option<Box<RawCallee>>,
    field: Option<String>,
    span: Option<RawSpan>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTargetOwner {
    kind: String,
    package: String,
    digest: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResource {
    id: String,
    module_id: String,
    unit_id: String,
    kind: String,
    span: RawSpan,
    content_digest: String,
    language: Value,
    display: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawImport {
    alias: String,
    target: String,
    resolved: Value,
    span: RawSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReference {
    source: String,
    source_unit: String,
    url: String,
    span: RawSpan,
    state: String,
    target_kind: Value,
    target_module: Value,
    target_unit: Value,
    candidate_package_instance_id: Value,
    candidate_path: Value,
    fragment: Value,
    candidates: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCall {
    function: String,
    span: RawSpan,
    callee: RawCallee,
    state: String,
    target: Value,
    target_kind: Value,
    target_owner: Value,
    reason: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiagnostic {
    module: String,
    relative_path: String,
    span: Value,
    line: Value,
    column: Value,
    severity: String,
    code: String,
    message: String,
    hints: Vec<String>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Verify the exact `check --emit` bytes and project the static facts.
///
/// `expected_entry_relative` is the package-relative path View asked the Core
/// to check; `expected_disk_source` is the source View read before the check.
/// When supplied, a digest mismatch is a `view.source_mismatch` (the page must
/// not attach semantics to a changed file). The artifact source itself is the
/// only source used to interpret spans.
pub fn verify_analysis(
    analysis_bytes: &[u8],
    expected_entry_relative: &str,
    expected_disk_source: Option<&str>,
) -> ViewResult<VerifiedAnalysis> {
    let raw: RawAnalysis =
        serde_json::from_slice(analysis_bytes).map_err(|err| ViewError::ProtocolIncompatible {
            detail: format!(
                "Analysis is not the current marqdo.analysis revision 3 shape: {err}. \
                 Regenerate it with the current `marqdo check --emit`."
            ),
        })?;
    if raw.schema != "marqdo.analysis" {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!(
                "unexpected Analysis schema {:?}; this View consumes `marqdo.analysis` only",
                raw.schema
            ),
        });
    }
    if raw.schema_version != 3 {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!(
                "Analysis schema_version {} is not supported (this View implements revision 3 only); \
                 upgrade or reinstall the Marqdo core",
                raw.schema_version
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
    validate_sha_prefixed("project.lock_digest", &raw.project.lock_digest, true)?;
    validate_sha_prefixed(
        "project.manifest_digest",
        &raw.project.manifest_digest,
        true,
    )?;
    if !matches!(
        raw.project.identity_kind.as_str(),
        "manifest" | "workspace_snapshot"
    ) {
        return Err(ViewError::AnalysisFailed {
            detail: format!(
                "Analysis project.identity_kind {:?} is not `manifest` or `workspace_snapshot`",
                raw.project.identity_kind
            ),
        });
    }
    if raw.project.root_id.is_empty() {
        return Err(ViewError::AnalysisFailed {
            detail: "Analysis project.root_id is empty".to_string(),
        });
    }
    let stdlib = parse_stdlib(&raw.project.stdlib)?;
    let extensions = validate_extensions(&raw.project.extensions)?;
    if raw.modules.is_empty() {
        return Err(ViewError::AnalysisFailed {
            detail: "Analysis has no modules".to_string(),
        });
    }

    // Modules are sorted by canonical id and ids are unique.
    let mut previous: Option<&str> = None;
    let mut module_ids = HashSet::new();
    for module in &raw.modules {
        if let Some(prev) = previous {
            if prev >= module.id.as_str() {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis modules are not sorted by id ({prev:?} before {:?})",
                        module.id
                    ),
                });
            }
        }
        previous = Some(&module.id);
        if !module_ids.insert(module.id.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!("duplicate Analysis module id {:?}", module.id),
            });
        }
        if !module.id.contains("::") {
            return Err(ViewError::AnalysisFailed {
                detail: format!("Analysis module id {:?} is not canonical", module.id),
            });
        }
        if module.package_instance_id.is_empty() || module.package_instance_id.contains("::") {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis module {:?} has an invalid package_instance_id",
                    module.id
                ),
            });
        }
        validate_relative_path("module.relative_path", &module.relative_path)?;
        if module.source_digest != sha256_hex(module.source.as_bytes()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis module {:?} source_digest does not match its source bytes; \
                     regenerate the artifact with the current core",
                    module.id
                ),
            });
        }
        if !module
            .id
            .starts_with(&format!("{}::", module.package_instance_id))
        {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis module id {:?} is not scoped by package instance {:?}",
                    module.id, module.package_instance_id
                ),
            });
        }
    }
    let mut modules: Vec<ModuleSummary> = raw
        .modules
        .iter()
        .map(|module| ModuleSummary {
            id: module.id.clone(),
            package_instance_id: module.package_instance_id.clone(),
            relative_path: module.relative_path.clone(),
            source_digest: module.source_digest.clone(),
            source: module.source.clone(),
        })
        .collect();
    modules.sort_by(|a, b| a.id.cmp(&b.id));

    let entry = raw
        .modules
        .iter()
        .find(|module| module.id == raw.project.entry_module_id)
        .ok_or_else(|| ViewError::AnalysisFailed {
            detail: format!(
                "Analysis project.entry_module_id {:?} does not occur in modules",
                raw.project.entry_module_id
            ),
        })?;
    if entry.relative_path != expected_entry_relative {
        return Err(ViewError::AnalysisFailed {
            detail: format!(
                "Analysis entry module path {:?} does not match the checked document {:?}",
                entry.relative_path, expected_entry_relative
            ),
        });
    }
    let expected_digest = sha256_prefixed(analysis_bytes);
    let source_index = crate::reader::source::SourceIndex::new(&entry.source);
    let source_bytes = entry.source.as_bytes();

    if let Some(disk) = expected_disk_source {
        let disk_digest = sha256_hex(disk.as_bytes());
        if disk_digest != entry.source_digest {
            return Err(ViewError::SourceMismatch {
                expected: entry.source_digest.clone(),
                actual: disk_digest,
            });
        }
    }

    let document = convert_node(&entry.document, &source_index, source_bytes)?;
    let units = convert_units(entry, &source_index, source_bytes)?;
    let anchors = convert_anchors(entry)?;
    let functions = convert_functions(entry, &source_index, source_bytes)?;
    let imports = convert_imports(entry, &module_ids)?;
    let references = convert_references(entry, &module_ids)?;
    let calls = convert_calls(entry, &functions, &unit_ids(entry))?;
    let resources = convert_resources(entry, &source_index, source_bytes, &document)?;
    let diagnostics = convert_diagnostics(entry, &raw.diagnostics, &source_index, source_bytes)?;

    // Cross-record invariants that need the whole entry module.
    validate_unit_tree(&units)?;
    validate_functions(&functions, &units)?;
    validate_references(&references, &units)?;
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error")
    {
        return Err(ViewError::AnalysisFailed {
            detail: "a successful Analysis must not contain error diagnostics".to_string(),
        });
    }

    Ok(VerifiedAnalysis {
        projection: SemanticProjection {
            schema_version: raw.schema_version,
            analysis_sha256: sha256_hex(analysis_bytes),
            analysis_digest: expected_digest,
            producer_version: raw.producer.version,
            markdown_profile: raw.producer.markdown_profile,
            project_root_id: raw.project.root_id,
            identity_kind: raw.project.identity_kind,
            entry_module_id: raw.project.entry_module_id,
            lock_digest: raw.project.lock_digest,
            manifest_digest: raw.project.manifest_digest,
            stdlib_version: stdlib.as_ref().map(|value| value.version.clone()),
            stdlib_digest: stdlib.as_ref().map(|value| value.digest.clone()),
            stdlib_instance_id: stdlib.as_ref().map(|value| value.instance_id.clone()),
            extensions,
            module_id: entry.id.clone(),
            package_instance_id: entry.package_instance_id.clone(),
            relative_path: entry.relative_path.clone(),
            source_digest: entry.source_digest.clone(),
            source: entry.source.clone(),
            document,
            anchors,
            units,
            functions,
            resources,
            imports,
            references,
            calls,
            diagnostics,
            warnings: Vec::new(),
            modules,
        },
    })
}

// ---------------------------------------------------------------------------
// Conversion + validation
// ---------------------------------------------------------------------------

fn opt_string(field: &str, value: &Value) -> ViewResult<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        _ => Err(ViewError::AnalysisFailed {
            detail: format!("Analysis {field} must be a string or null"),
        }),
    }
}

fn opt_span(
    field: &str,
    value: &Value,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
) -> ViewResult<Option<SourceSpan>> {
    match value {
        Value::Null => Ok(None),
        Value::Object(_) => {
            let raw: RawSpan =
                serde_json::from_value(value.clone()).map_err(|err| ViewError::AnalysisFailed {
                    detail: format!("Analysis {field} is not a valid span: {err}"),
                })?;
            let span = raw.to_span();
            if !source_index.validate_span_bytes(source_bytes, span) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis {field} span {}..{} is outside the source or not on a UTF-8 boundary",
                        span.start, span.end
                    ),
                });
            }
            Ok(Some(span))
        }
        _ => Err(ViewError::AnalysisFailed {
            detail: format!("Analysis {field} must be a span object or null"),
        }),
    }
}

fn valid_span(
    field: &str,
    raw: &RawSpan,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
) -> ViewResult<SourceSpan> {
    let span = raw.to_span();
    if !source_index.validate_span_bytes(source_bytes, span) {
        return Err(ViewError::AnalysisFailed {
            detail: format!(
                "Analysis {field} span {}..{} is outside the source or not on a UTF-8 boundary",
                span.start, span.end
            ),
        });
    }
    Ok(span)
}

fn validate_sha_hex(field: &str, value: &str) -> ViewResult<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ViewError::AnalysisFailed {
            detail: format!("Analysis {field} is not a lowercase hex SHA-256"),
        })
    }
}

fn validate_sha_prefixed(field: &str, value: &str, allow_empty: bool) -> ViewResult<()> {
    if value.is_empty() {
        if allow_empty {
            return Ok(());
        }
        return Err(ViewError::AnalysisFailed {
            detail: format!("Analysis {field} is empty"),
        });
    }
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| ViewError::AnalysisFailed {
            detail: format!("Analysis {field} must start with `sha256:`"),
        })?;
    validate_sha_hex(field, hex)
}

fn validate_relative_path(field: &str, path: &str) -> ViewResult<()> {
    let valid = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && !path.contains("://")
        && !path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        && !(path.len() >= 2 && path.as_bytes()[1] == b':')
        && !path.starts_with("//");
    if valid {
        Ok(())
    } else {
        Err(ViewError::AnalysisFailed {
            detail: format!("Analysis {field} {path:?} is not a safe package-relative path"),
        })
    }
}

fn parse_stdlib(value: &Value) -> ViewResult<Option<RawStdlib>> {
    match value {
        Value::Null => Ok(None),
        Value::Object(_) => {
            let raw: RawStdlib =
                serde_json::from_value(value.clone()).map_err(|err| ViewError::AnalysisFailed {
                    detail: format!("Analysis project.stdlib is invalid: {err}"),
                })?;
            validate_sha_prefixed("project.stdlib.digest", &raw.digest, false)?;
            if raw.version.is_empty() || raw.instance_id.is_empty() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis project.stdlib version/instance_id must not be empty"
                        .to_string(),
                });
            }
            Ok(Some(raw))
        }
        _ => Err(ViewError::AnalysisFailed {
            detail: "Analysis project.stdlib must be an object or null".to_string(),
        }),
    }
}

fn validate_extensions(raw: &[RawExtension]) -> ViewResult<Vec<ExtensionIdentity>> {
    let mut out = Vec::with_capacity(raw.len());
    let mut keys = Vec::new();
    for extension in raw {
        if !matches!(extension.kind.as_str(), "plugin" | "kernel" | "provider") {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis project.extensions[].kind {:?} is not plugin/kernel/provider",
                    extension.kind
                ),
            });
        }
        validate_sha_prefixed(
            "project.extensions[].manifest_digest",
            &extension.manifest_digest,
            false,
        )?;
        validate_sha_prefixed(
            "project.extensions[].artifact_digest",
            &extension.artifact_digest,
            false,
        )?;
        if extension.name.is_empty() || extension.selected_target.is_empty() {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis extension name/selected_target must not be empty".to_string(),
            });
        }
        keys.push((
            extension.kind.clone(),
            extension.name.clone(),
            extension.logical_owner.clone(),
            extension.manifest_digest.clone(),
        ));
        out.push(ExtensionIdentity {
            kind: extension.kind.clone(),
            name: extension.name.clone(),
            version: extension.version.clone(),
            logical_owner: extension.logical_owner.clone(),
            source: extension.source.clone(),
            manifest_digest: extension.manifest_digest.clone(),
            artifact_digest: extension.artifact_digest.clone(),
            selected_target: extension.selected_target.clone(),
        });
    }
    if keys.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ViewError::AnalysisFailed {
            detail: "Analysis project.extensions is not sorted by (kind, name, logical_owner, manifest_digest)"
                .to_string(),
        });
    }
    Ok(out)
}

fn convert_node(
    raw: &RawNode,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
) -> ViewResult<DocumentNode> {
    let span = valid_span("document", &raw.span, source_index, source_bytes)?;
    let kind = raw.kind.as_str();
    let leaf = !matches!(
        kind,
        "document"
            | "block_quote"
            | "list"
            | "item"
            | "heading"
            | "paragraph"
            | "strong"
            | "emph"
            | "strikethrough"
            | "table"
            | "table_row"
            | "table_cell"
            | "link"
            | "image"
            | "other"
    );
    // Conditional attribute rules from the published Markdown profile schema.
    let require_literal = matches!(
        kind,
        "front_matter" | "text" | "math_block" | "html_block" | "html_inline"
    );
    if require_literal && raw.literal.is_none() {
        return Err(ViewError::AnalysisFailed {
            detail: format!("Analysis document {kind} node requires `literal`"),
        });
    }
    match kind {
        "document" | "block_quote" | "paragraph" | "strong" | "emph" | "strikethrough"
        | "table_cell" | "thematic_break" | "soft_break" | "line_break" => {}
        "front_matter" | "text" | "math_block" | "html_block" | "html_inline" => {}
        "list" => {
            if raw.ordered.is_none() || raw.tight.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis list node requires ordered/tight".to_string(),
                });
            }
        }
        // `task` is a required-but-nullable attribute: null means "not a task".
        "item" => {}

        "heading" => {
            let level = raw.level.ok_or_else(|| ViewError::AnalysisFailed {
                detail: "Analysis heading node requires level".to_string(),
            })?;
            if !(1..=6).contains(&level) || raw.setext.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis heading node requires level 1-6 and setext".to_string(),
                });
            }
        }
        "code" => {
            if raw.literal.is_none() || raw.ticks.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis code node requires literal/ticks".to_string(),
                });
            }
        }
        "code_block" => {
            if raw.literal.is_none() || raw.fenced.is_none() || raw.info.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis code_block node requires literal/fenced/info".to_string(),
                });
            }
        }
        "math" => {
            if raw.literal.is_none() || raw.display.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis math node requires literal/display".to_string(),
                });
            }
        }
        "link" | "image" => {
            if raw.url.is_none() || raw.title.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis {kind} node requires url/title"),
                });
            }
        }
        "table" => {
            if raw.alignments.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis table node requires alignments".to_string(),
                });
            }
            if let Some(alignments) = &raw.alignments {
                if alignments
                    .iter()
                    .any(|value| !matches!(value.as_str(), "none" | "left" | "center" | "right"))
                {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis table alignments contain an unknown value".to_string(),
                    });
                }
            }
        }
        "table_row" => {
            if raw.header.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis table_row node requires header".to_string(),
                });
            }
        }
        "other" => {
            if raw.tag.as_deref().is_none_or(str::is_empty) {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis other node requires a non-empty tag".to_string(),
                });
            }
        }
        other => {
            return Err(ViewError::AnalysisFailed {
                detail: format!("Analysis document node kind {other:?} is not in marqdo.md.v1"),
            })
        }
    }
    if leaf && !raw.children.is_empty() {
        return Err(ViewError::AnalysisFailed {
            detail: format!("Analysis document leaf {kind} must not have children"),
        });
    }
    let mut children = Vec::with_capacity(raw.children.len());
    let mut previous_end = span.start;
    for child in &raw.children {
        let node = convert_node(child, source_index, source_bytes)?;
        if node.span.start < previous_end {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis document {kind} children are not in source order at {}",
                    node.span.start
                ),
            });
        }
        previous_end = node.span.end;
        children.push(node);
    }
    Ok(DocumentNode {
        kind: raw.kind.clone(),
        span,
        children,
        literal: raw.literal.clone(),
        level: raw.level,
        setext: raw.setext,
        ordered: raw.ordered,
        start: raw.start,
        tight: raw.tight,
        task: raw.task,
        ticks: raw.ticks,
        fenced: raw.fenced,
        info: raw.info.clone(),
        display: raw.display,
        url: raw.url.clone(),
        title: raw.title.clone(),
        alignments: raw.alignments.clone(),
        header: raw.header,
        tag: raw.tag.clone(),
    })
}

fn convert_units(
    module: &RawModule,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
) -> ViewResult<Vec<SemanticUnit>> {
    let mut units = Vec::with_capacity(module.units.len());
    for raw in &module.units {
        let kind = UnitKind::parse(&raw.kind).ok_or_else(|| ViewError::AnalysisFailed {
            detail: format!("Analysis unit kind {:?} is not in revision 3", raw.kind),
        })?;
        let mode = match raw.mode.as_str() {
            "exec" => Mode::Exec,
            "knowledge" => Mode::Knowledge,
            other => {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis unit mode {other:?} is not exec/knowledge"),
                })
            }
        };
        let mode_origin = match raw.mode_origin.as_str() {
            "default" => ModeOrigin::Default,
            "explicit" => ModeOrigin::Explicit,
            "inherited" => ModeOrigin::Inherited,
            other => {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis unit mode_origin {other:?} is not valid"),
                })
            }
        };
        let span = valid_span("unit.span", &raw.span, source_index, source_bytes)?;
        let heading_span = opt_span(
            "unit.heading_span",
            &raw.heading_span,
            source_index,
            source_bytes,
        )?;
        let parent_id = opt_string("unit.parent_id", &raw.parent_id)?;
        let function = opt_string("unit.function", &raw.function)?;
        if raw.id.is_empty() || raw.mode_source_unit_id.is_empty() {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis unit id/mode_source_unit_id must not be empty".to_string(),
            });
        }
        if raw.level > 6 {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis unit level is above 6".to_string(),
            });
        }
        if kind == UnitKind::Root {
            if raw.level != 0 || !raw.path.is_empty() || parent_id.is_some() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis root unit must have level 0, empty path and null parent"
                        .to_string(),
                });
            }
            if heading_span.is_some() || function.is_some() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis root unit must have no heading span or function".to_string(),
                });
            }
            if mode != Mode::Knowledge || mode_origin != ModeOrigin::Default {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis root unit must be knowledge/default".to_string(),
                });
            }
        } else {
            if raw.level == 0 || heading_span.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis unit {:?} must have a level and heading_span",
                        raw.id
                    ),
                });
            }
            let heading = heading_span.expect("checked");
            if !span.contains(heading) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis unit {:?} heading_span is not inside its complete span",
                        raw.id
                    ),
                });
            }
        }
        if kind == UnitKind::Function && function.is_none() {
            return Err(ViewError::AnalysisFailed {
                detail: format!("Analysis function unit {:?} has no function id", raw.id),
            });
        }
        if kind != UnitKind::Function && function.is_some() {
            // Non-Exec nested sections may point at the owning function; only
            // root/namespace/knowledge units must not.
            if matches!(
                kind,
                UnitKind::Root | UnitKind::Namespace | UnitKind::Knowledge
            ) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis {:?} unit {:?} must not name a function",
                        raw.kind, raw.id
                    ),
                });
            }
        }
        units.push(SemanticUnit {
            id: raw.id.clone(),
            path: raw.path.clone(),
            parent_id,
            name: raw.name.clone(),
            level: raw.level,
            kind,
            mode,
            mode_origin,
            mode_source_unit_id: raw.mode_source_unit_id.clone(),
            heading_span,
            span,
            function,
        });
    }
    let mut seen = HashSet::new();
    for unit in &units {
        if !seen.insert(unit.id.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!("duplicate Analysis unit id {:?}", unit.id),
            });
        }
    }
    Ok(units)
}

fn validate_unit_tree(units: &[SemanticUnit]) -> ViewResult<()> {
    let roots: Vec<&SemanticUnit> = units
        .iter()
        .filter(|unit| unit.kind == UnitKind::Root)
        .collect();
    if roots.len() != 1 {
        return Err(ViewError::AnalysisFailed {
            detail: format!(
                "Analysis must have exactly one root unit, found {}",
                roots.len()
            ),
        });
    }
    if !roots[0].path.is_empty() {
        return Err(ViewError::AnalysisFailed {
            detail: "Analysis root path must be empty".to_string(),
        });
    }
    let by_id: HashMap<&str, &SemanticUnit> =
        units.iter().map(|unit| (unit.id.as_str(), unit)).collect();
    for unit in units {
        if unit.kind == UnitKind::Root {
            continue;
        }
        let parent_id = unit
            .parent_id
            .as_deref()
            .ok_or_else(|| ViewError::AnalysisFailed {
                detail: format!("Analysis unit {:?} has no parent", unit.id),
            })?;
        let parent = by_id
            .get(parent_id)
            .ok_or_else(|| ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis unit {:?} names unknown parent {parent_id:?}",
                    unit.id
                ),
            })?;
        let mut expected = parent.path.clone();
        expected.push(unit.name.clone());
        if unit.path != expected {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis unit {:?} path is not its parent path plus name",
                    unit.id
                ),
            });
        }
        if !parent.span.contains(unit.span) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis unit {:?} span is not nested in its parent",
                    unit.id
                ),
            });
        }
        let source = by_id
            .get(unit.mode_source_unit_id.as_str())
            .ok_or_else(|| ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis unit {:?} names unknown mode_source_unit_id {:?}",
                    unit.id, unit.mode_source_unit_id
                ),
            })?;
        // The mode source is the unit itself or one of its ancestors.
        let mut cursor = Some(unit);
        let mut found = false;
        while let Some(current) = cursor {
            if current.id == source.id {
                found = true;
                break;
            }
            cursor = current
                .parent_id
                .as_deref()
                .and_then(|id| by_id.get(id).copied());
        }
        if !found {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis unit {:?} mode_source_unit_id is not self-or-ancestor",
                    unit.id
                ),
            });
        }
    }
    // No cycles beyond a bounded tree depth.
    for unit in units {
        let mut cursor = Some(unit);
        let mut depth = 0;
        while let Some(current) = cursor {
            depth += 1;
            if depth > 64 {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis unit {:?} parent chain has a cycle", unit.id),
                });
            }
            cursor = current
                .parent_id
                .as_deref()
                .and_then(|id| by_id.get(id).copied());
        }
    }
    Ok(())
}

fn unit_ids(module: &RawModule) -> HashSet<String> {
    module.units.iter().map(|unit| unit.id.clone()).collect()
}

fn convert_anchors(module: &RawModule) -> ViewResult<Vec<Anchor>> {
    let mut anchors = Vec::with_capacity(module.anchors.len());
    let units: HashSet<&str> = module.units.iter().map(|unit| unit.id.as_str()).collect();
    let mut previous: Option<&str> = None;
    for raw in &module.anchors {
        if raw.anchor.is_empty() {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis anchor name must not be empty".to_string(),
            });
        }
        if !units.contains(raw.unit_id.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis anchor {:?} names unknown unit {:?}",
                    raw.anchor, raw.unit_id
                ),
            });
        }
        if let Some(prev) = previous {
            if prev > raw.anchor.as_str() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis anchors are not sorted by anchor".to_string(),
                });
            }
        }
        previous = Some(&raw.anchor);
        anchors.push(Anchor {
            anchor: raw.anchor.clone(),
            unit_id: raw.unit_id.clone(),
        });
    }
    Ok(anchors)
}

fn convert_functions(
    module: &RawModule,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
) -> ViewResult<Vec<SemanticFunction>> {
    let units: HashMap<&str, &RawUnit> = module
        .units
        .iter()
        .map(|unit| (unit.id.as_str(), unit))
        .collect();
    let mut functions = Vec::with_capacity(module.functions.len());
    let mut ids = HashSet::new();
    for raw in &module.functions {
        if !ids.insert(raw.id.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!("duplicate Analysis function id {:?}", raw.id),
            });
        }
        let span = valid_span("function.span", &raw.span, source_index, source_bytes)?;
        let heading_span = valid_span(
            "function.heading_span",
            &raw.heading_span,
            source_index,
            source_bytes,
        )?;
        let unit = units
            .get(raw.unit_id.as_str())
            .ok_or_else(|| ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis function {:?} names unknown unit {:?}",
                    raw.id, raw.unit_id
                ),
            })?;
        if unit.kind != "function" {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis function {:?} unit {:?} is not an Exec H2 function unit",
                    raw.id, raw.unit_id
                ),
            });
        }
        if unit.function.as_str() != Some(raw.id.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis function {:?} does not own unit {:?}",
                    raw.id, raw.unit_id
                ),
            });
        }
        // The function span covers the executable body; the unit span is the
        // complete section (including trailing child headings/EOF). The function
        // span must stay inside its unit and start at the heading.
        if !unit.span.to_span().contains(span) || span.start != heading_span.start {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis function {:?} span is not the executable part of its unit section",
                    raw.id
                ),
            });
        }
        if raw.local_name.is_empty() {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis function local_name must not be empty".to_string(),
            });
        }
        if !matches!(raw.signature.kind.as_str(), "explicit" | "inferred") {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis function {:?} signature kind {:?} is not explicit/inferred",
                    raw.id, raw.signature.kind
                ),
            });
        }
        let namespace = opt_string("function.namespace", &raw.namespace)?;
        let mut params = Vec::with_capacity(raw.signature.params.len());
        let mut param_names = HashSet::new();
        for param in &raw.signature.params {
            if param.name.is_empty() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis function parameter name must not be empty".to_string(),
                });
            }
            if !param_names.insert(param.name.as_str()) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis function {:?} has duplicate parameter {:?}",
                        raw.id, param.name
                    ),
                });
            }
            let param_span = valid_span(
                "function.param.span",
                &param.span,
                source_index,
                source_bytes,
            )?;
            let ty = opt_string("function.param.type", &param.ty)?;
            let default = match &param.default {
                Value::Null => None,
                Value::Object(_) => {
                    validate_expression_value(
                        "function.param.default",
                        &param.default,
                        source_index,
                        source_bytes,
                    )?;
                    Some(param.default.clone())
                }
                _ => {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis function parameter default must be an expression or null"
                            .to_string(),
                    })
                }
            };
            params.push(FunctionParam {
                name: param.name.clone(),
                ty,
                required: param.required,
                default,
                span: param_span,
            });
        }
        let mut statements = Vec::with_capacity(raw.body.len());
        for statement in &raw.body {
            statements.push(convert_statement(statement, source_index, source_bytes)?);
        }
        functions.push(SemanticFunction {
            id: raw.id.clone(),
            unit_id: raw.unit_id.clone(),
            local_name: raw.local_name.clone(),
            namespace,
            span,
            heading_span,
            signature: FunctionSignature {
                kind: raw.signature.kind.clone(),
                params,
            },
            effects: raw.effects.clone(),
            statements,
        });
    }
    // Functions are deterministic: sort by id is not required by the schema
    // (document order is), so only enforce uniqueness here.
    Ok(functions)
}

fn validate_functions(functions: &[SemanticFunction], units: &[SemanticUnit]) -> ViewResult<()> {
    let unit_by_id: HashMap<&str, &SemanticUnit> =
        units.iter().map(|unit| (unit.id.as_str(), unit)).collect();
    let mut function_ids = HashSet::new();
    for function in functions {
        if !function_ids.insert(function.id.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!("duplicate Analysis function id {:?}", function.id),
            });
        }
        let unit =
            unit_by_id
                .get(function.unit_id.as_str())
                .ok_or_else(|| ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis function {:?} names unknown unit {:?}",
                        function.id, function.unit_id
                    ),
                })?;
        if unit.function.as_deref() != Some(function.id.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis unit {:?} does not point back at function {:?}",
                    unit.id, function.id
                ),
            });
        }
        if unit.heading_span != Some(function.heading_span) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis function {:?} heading_span disagrees with its unit",
                    function.id
                ),
            });
        }
    }
    Ok(())
}

fn convert_statement(
    raw: &RawStatement,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
) -> ViewResult<SemanticStatement> {
    let span = valid_span("statement.span", &raw.span, source_index, source_bytes)?;
    let kind = match raw.kind.as_str() {
        "assign" => {
            let target = raw
                .target
                .as_ref()
                .ok_or_else(|| ViewError::AnalysisFailed {
                    detail: "Analysis assign statement requires target".to_string(),
                })?;
            let target = match target {
                Value::String(name) => name.clone(),
                Value::Object(_) => {
                    let kind = target.get("kind").and_then(Value::as_str);
                    if kind != Some("index") {
                        return Err(ViewError::AnalysisFailed {
                            detail: "Analysis assign target place must be a string or index place"
                                .to_string(),
                        });
                    }
                    "索引赋值".to_string()
                }
                _ => {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis assign target must be a string or place".to_string(),
                    })
                }
            };
            let value = raw
                .value
                .as_ref()
                .ok_or_else(|| ViewError::AnalysisFailed {
                    detail: "Analysis assign statement requires value".to_string(),
                })?;
            validate_expression_value("statement.value", value, source_index, source_bytes)?;
            return Ok(SemanticStatement {
                kind: StatementKind::Assign,
                span,
                condition_span: None,
                arms: Vec::new(),
                body: Vec::new(),
                target: Some(target),
            });
        }
        "expression" => {
            let value = raw
                .value
                .as_ref()
                .ok_or_else(|| ViewError::AnalysisFailed {
                    detail: "Analysis expression statement requires value".to_string(),
                })?;
            validate_expression_value("statement.value", value, source_index, source_bytes)?;
            StatementKind::Expression
        }
        "branch" => {
            if raw.arms.is_empty() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis branch statement requires arms".to_string(),
                });
            }
            let mut arms = Vec::with_capacity(raw.arms.len());
            for arm in &raw.arms {
                validate_expression_value(
                    "branch.arm.condition",
                    &arm.condition,
                    source_index,
                    source_bytes,
                )?;
                let arm_span =
                    valid_span("branch.arm.span", &arm.span, source_index, source_bytes)?;
                let condition_span = valid_span(
                    "branch.arm.condition_span",
                    &arm.condition_span,
                    source_index,
                    source_bytes,
                )?;
                let mut body = Vec::with_capacity(arm.body.len());
                for child in &arm.body {
                    body.push(convert_statement(child, source_index, source_bytes)?);
                }
                arms.push(SemanticArm {
                    span: arm_span,
                    condition_span: Some(condition_span),
                    body,
                });
            }
            return Ok(SemanticStatement {
                kind: StatementKind::Branch,
                span,
                condition_span: None,
                arms,
                body: Vec::new(),
                target: None,
            });
        }
        "loop" => {
            let condition = raw
                .condition
                .as_ref()
                .ok_or_else(|| ViewError::AnalysisFailed {
                    detail: "Analysis loop statement requires condition".to_string(),
                })?;
            validate_expression_value("loop.condition", condition, source_index, source_bytes)?;
            let condition_span = raw
                .condition_span
                .as_ref()
                .map(|span| valid_span("loop.condition_span", span, source_index, source_bytes))
                .transpose()?;
            let mut body = Vec::with_capacity(raw.body.len());
            for child in &raw.body {
                body.push(convert_statement(child, source_index, source_bytes)?);
            }
            return Ok(SemanticStatement {
                kind: StatementKind::Loop,
                span,
                condition_span,
                arms: Vec::new(),
                body,
                target: None,
            });
        }
        "return" => {
            let value = raw
                .value
                .as_ref()
                .ok_or_else(|| ViewError::AnalysisFailed {
                    detail: "Analysis return statement requires value".to_string(),
                })?;
            validate_expression_value("return.value", value, source_index, source_bytes)?;
            StatementKind::Return
        }
        "break" => StatementKind::Break,
        "continue" => StatementKind::Continue,
        other => {
            return Err(ViewError::AnalysisFailed {
                detail: format!("Analysis statement kind {other:?} is not supported"),
            })
        }
    };
    Ok(SemanticStatement {
        kind,
        span,
        condition_span: None,
        arms: Vec::new(),
        body: Vec::new(),
        target: None,
    })
}

fn validate_expression_value(
    field: &str,
    value: &Value,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
) -> ViewResult<()> {
    let raw: RawExpression =
        serde_json::from_value(value.clone()).map_err(|err| ViewError::AnalysisFailed {
            detail: format!("Analysis {field} is not a valid expression: {err}"),
        })?;
    validate_expression(field, &raw, source_index, source_bytes, 0)
}

fn validate_expression(
    field: &str,
    raw: &RawExpression,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
    depth: usize,
) -> ViewResult<()> {
    if depth > 64 {
        return Err(ViewError::AnalysisFailed {
            detail: format!("Analysis {field} expression nesting is too deep"),
        });
    }
    valid_span(field, &raw.span, source_index, source_bytes)?;
    let child = |name: &str, node: &RawExpression| {
        validate_expression(name, node, source_index, source_bytes, depth + 1)
    };
    match raw.kind.as_str() {
        "literal" => {
            let ty = raw.ty.as_deref().ok_or_else(|| ViewError::AnalysisFailed {
                detail: format!("Analysis {field} literal requires type"),
            })?;
            match ty {
                "int" => {
                    let text = raw.value.as_ref().and_then(Value::as_str);
                    if text.is_none_or(|text| {
                        let digits = text.strip_prefix('-').unwrap_or(text);
                        digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit())
                    }) {
                        return Err(ViewError::AnalysisFailed {
                            detail: format!(
                                "Analysis {field} int literal value is not a decimal string"
                            ),
                        });
                    }
                }
                "float" => {
                    if !raw
                        .value
                        .as_ref()
                        .is_some_and(|value| value.as_f64().is_some_and(f64::is_finite))
                    {
                        return Err(ViewError::AnalysisFailed {
                            detail: format!(
                                "Analysis {field} float literal is not a finite number"
                            ),
                        });
                    }
                }
                "text" => {
                    if !raw.value.as_ref().is_some_and(Value::is_string) {
                        return Err(ViewError::AnalysisFailed {
                            detail: format!("Analysis {field} text literal is not a string"),
                        });
                    }
                }
                "bool" => {
                    if !raw.value.as_ref().is_some_and(Value::is_boolean) {
                        return Err(ViewError::AnalysisFailed {
                            detail: format!("Analysis {field} bool literal is not a boolean"),
                        });
                    }
                }
                "none" => {}
                other => {
                    return Err(ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} literal type {other:?} is unknown"),
                    })
                }
            }
        }
        "variable" => {
            if raw.name.as_deref().is_none_or(str::is_empty) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis {field} variable requires name"),
                });
            }
        }
        "list" => {
            for item in raw.items.as_deref().unwrap_or_default() {
                child(field, item)?;
            }
        }
        "record" => {
            for entry in raw.entries.as_deref().unwrap_or_default() {
                if entry.key.is_empty() {
                    return Err(ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} record key must not be empty"),
                    });
                }
                valid_span(field, &entry.key_span, source_index, source_bytes)?;
                child(field, &entry.value)?;
            }
        }
        "unary" => {
            let op = raw.op.as_deref().unwrap_or_default();
            if !matches!(op, "not" | "neg" | "pos") {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis {field} unary op {op:?} is unknown"),
                });
            }
            child(
                field,
                raw.operand
                    .as_deref()
                    .ok_or_else(|| ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} unary requires operand"),
                    })?,
            )?;
        }
        "binary" => {
            let op = raw.op.as_deref().unwrap_or_default();
            if !matches!(
                op,
                "or" | "and"
                    | "eq"
                    | "ne"
                    | "lt"
                    | "le"
                    | "gt"
                    | "ge"
                    | "add"
                    | "sub"
                    | "mul"
                    | "div"
                    | "floor_div"
                    | "mod"
                    | "pow"
            ) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis {field} binary op {op:?} is unknown"),
                });
            }
            child(
                field,
                raw.left
                    .as_deref()
                    .ok_or_else(|| ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} binary requires left"),
                    })?,
            )?;
            child(
                field,
                raw.right
                    .as_deref()
                    .ok_or_else(|| ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} binary requires right"),
                    })?,
            )?;
        }
        "member" => {
            child(
                field,
                raw.base
                    .as_deref()
                    .ok_or_else(|| ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} member requires base"),
                    })?,
            )?;
            if raw.field.as_deref().is_none_or(str::is_empty) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis {field} member requires field"),
                });
            }
        }
        "index" => {
            child(
                field,
                raw.base
                    .as_deref()
                    .ok_or_else(|| ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} index requires base"),
                    })?,
            )?;
            child(
                field,
                raw.index
                    .as_deref()
                    .ok_or_else(|| ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} index requires index"),
                    })?,
            )?;
        }
        "call" => {
            let callee = raw
                .callee
                .as_deref()
                .ok_or_else(|| ViewError::AnalysisFailed {
                    detail: format!("Analysis {field} call requires callee"),
                })?;
            child(field, callee)?;
            for arg in raw.args.as_deref().unwrap_or_default() {
                if arg.name.as_deref().is_some_and(str::is_empty) {
                    return Err(ViewError::AnalysisFailed {
                        detail: format!("Analysis {field} call argument name must not be empty"),
                    });
                }
                valid_span(field, &arg.span, source_index, source_bytes)?;
                child(field, &arg.value)?;
            }
        }
        other => {
            return Err(ViewError::AnalysisFailed {
                detail: format!("Analysis {field} expression kind {other:?} is unknown"),
            })
        }
    }
    Ok(())
}

fn convert_imports(
    module: &RawModule,
    module_ids: &HashSet<&str>,
) -> ViewResult<Vec<StaticImport>> {
    let mut imports = Vec::with_capacity(module.imports.len());
    for raw in &module.imports {
        if raw.alias.is_empty() || raw.target.is_empty() {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis import alias/target must not be empty".to_string(),
            });
        }
        // Import spans are package-relative to this module's source.
        let span = raw.span.to_span();
        let resolved = opt_string("import.resolved", &raw.resolved)?;
        if let Some(target) = &resolved {
            if !module_ids.contains(target.as_str()) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis import {:?} resolves to unknown module {target:?}",
                        raw.alias
                    ),
                });
            }
        }
        imports.push(StaticImport {
            alias: raw.alias.clone(),
            target: raw.target.clone(),
            resolved,
            span,
        });
    }
    Ok(imports)
}

fn convert_references(
    module: &RawModule,
    module_ids: &HashSet<&str>,
) -> ViewResult<Vec<StaticReference>> {
    let unit_ids: HashSet<&str> = module.units.iter().map(|unit| unit.id.as_str()).collect();
    let mut references = Vec::with_capacity(module.references.len());
    for raw in &module.references {
        let state = ReferenceState::parse(&raw.state).ok_or_else(|| ViewError::AnalysisFailed {
            detail: format!(
                "Analysis reference state {:?} is not one of the current resolution states",
                raw.state
            ),
        })?;
        if raw.source != module.id {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis reference source {:?} does not belong to module {:?}",
                    raw.source, module.id
                ),
            });
        }
        if !unit_ids.contains(raw.source_unit.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis reference source_unit {:?} is not a unit of this module",
                    raw.source_unit
                ),
            });
        }
        let target_kind = match opt_string("reference.target_kind", &raw.target_kind)?.as_deref() {
            None => None,
            Some("module") => Some(ReferenceTargetKind::Module),
            Some("file") => Some(ReferenceTargetKind::File),
            Some(other) => {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis reference target_kind {other:?} is unknown"),
                })
            }
        };
        let target_module = opt_string("reference.target_module", &raw.target_module)?;
        let target_unit = opt_string("reference.target_unit", &raw.target_unit)?;
        let candidate_package_instance_id = opt_string(
            "reference.candidate_package_instance_id",
            &raw.candidate_package_instance_id,
        )?;
        let candidate_path = opt_string("reference.candidate_path", &raw.candidate_path)?;
        let fragment = opt_string("reference.fragment", &raw.fragment)?;
        let span = raw.span.to_span();
        match state {
            ReferenceState::Resolved => {
                match target_kind {
                    Some(ReferenceTargetKind::Module) => {
                        let target =
                            target_module
                                .as_deref()
                                .ok_or_else(|| ViewError::AnalysisFailed {
                                    detail:
                                        "Analysis resolved module reference has no target_module"
                                            .to_string(),
                                })?;
                        if !module_ids.contains(target) {
                            return Err(ViewError::AnalysisFailed {
                                detail: format!(
                                    "Analysis resolved reference names unknown module {target:?}"
                                ),
                            });
                        }
                        if fragment.is_some() && target_unit.is_none() {
                            return Err(ViewError::AnalysisFailed {
                                detail:
                                    "Analysis resolved reference with a fragment has no target_unit"
                                        .to_string(),
                            });
                        }
                        if let Some(unit) = &target_unit {
                            if !unit_ids.contains(unit.as_str()) {
                                // The target unit may belong to another loaded module.
                                let mut found = false;
                                if let Some(target) = &target_module {
                                    if target != &module.id {
                                        found = true; // validated through module existence
                                    }
                                }
                                if !found {
                                    return Err(ViewError::AnalysisFailed {
                                        detail: format!(
                                            "Analysis resolved reference target_unit {unit:?} is unknown"
                                        ),
                                    });
                                }
                            }
                        }
                        if fragment.is_none() && target_unit.is_some() {
                            return Err(ViewError::AnalysisFailed {
                                detail: "Analysis resolved reference without a fragment must have target_unit null"
                                    .to_string(),
                            });
                        }
                    }
                    Some(ReferenceTargetKind::File) => {
                        if fragment.is_some() {
                            return Err(ViewError::AnalysisFailed {
                                detail: "Analysis resolved file reference must not have a fragment"
                                    .to_string(),
                            });
                        }
                    }
                    None => {
                        return Err(ViewError::AnalysisFailed {
                            detail: "Analysis resolved reference needs target_kind module/file"
                                .to_string(),
                        })
                    }
                }
            }
            ReferenceState::Ambiguous => {
                if raw.candidates.len() < 2 || target_unit.is_some() {
                    return Err(ViewError::AnalysisFailed {
                        detail:
                            "Analysis ambiguous reference must list at least two candidates and null target_unit"
                                .to_string(),
                    });
                }
            }
            ReferenceState::Missing
            | ReferenceState::NotLoaded
            | ReferenceState::External
            | ReferenceState::OutsideScope
            | ReferenceState::Invalid => {
                if target_unit.is_some() {
                    return Err(ViewError::AnalysisFailed {
                        detail: format!(
                            "Analysis {} reference must have target_unit null",
                            state.data_value()
                        ),
                    });
                }
            }
        }
        references.push(StaticReference {
            source: raw.source.clone(),
            source_unit: Some(raw.source_unit.clone()),
            url: raw.url.clone(),
            span,
            state,
            target_kind,
            target_module,
            target_unit,
            candidate_package_instance_id,
            candidate_path,
            fragment,
            candidates: raw.candidates.clone(),
        });
    }
    Ok(references)
}

fn validate_references(references: &[StaticReference], units: &[SemanticUnit]) -> ViewResult<()> {
    let unit_ids: HashSet<&str> = units.iter().map(|unit| unit.id.as_str()).collect();
    for reference in references {
        if let Some(unit) = &reference.source_unit {
            if !unit_ids.contains(unit.as_str()) {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis reference source_unit {unit:?} is unknown"),
                });
            }
        }
    }
    Ok(())
}

fn convert_calls(
    module: &RawModule,
    functions: &[SemanticFunction],
    unit_ids: &HashSet<String>,
) -> ViewResult<Vec<StaticCall>> {
    let function_ids: HashSet<&str> = functions
        .iter()
        .map(|function| function.id.as_str())
        .collect();
    let mut calls = Vec::with_capacity(module.calls.len());
    for raw in &module.calls {
        if !function_ids.contains(raw.function.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis call belongs to unknown function {:?}",
                    raw.function
                ),
            });
        }
        let state = CallState::parse(&raw.state).ok_or_else(|| ViewError::AnalysisFailed {
            detail: format!("Analysis call state {:?} is unknown", raw.state),
        })?;
        let target = opt_string("call.target", &raw.target)?;
        let target_kind = opt_string("call.target_kind", &raw.target_kind)?;
        let reason = opt_string("call.reason", &raw.reason)?;
        let target_owner = match &raw.target_owner {
            Value::Null => None,
            Value::Object(_) => {
                let owner: RawTargetOwner = serde_json::from_value(raw.target_owner.clone())
                    .map_err(|err| ViewError::AnalysisFailed {
                        detail: format!("Analysis call target_owner is invalid: {err}"),
                    })?;
                if owner.kind != "plugin" {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis call target_owner kind must be `plugin`".to_string(),
                    });
                }
                validate_sha_prefixed("call.target_owner.digest", &owner.digest, false)?;
                Some(CallTargetOwner {
                    kind: owner.kind,
                    package: owner.package,
                    digest: owner.digest,
                })
            }
            _ => {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis call target_owner must be an object or null".to_string(),
                })
            }
        };
        match state {
            CallState::Function => {
                if target_kind.as_deref() != Some("function") {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis function call must have target_kind function".to_string(),
                    });
                }
                let target = target.as_deref().ok_or_else(|| ViewError::AnalysisFailed {
                    detail: "Analysis function call has no target".to_string(),
                })?;
                if !function_ids.contains(target) {
                    return Err(ViewError::AnalysisFailed {
                        detail: format!(
                            "Analysis function call target {target:?} is not a loaded function"
                        ),
                    });
                }
            }
            CallState::Builtin => {
                if target_kind.as_deref() != Some("builtin") || target.is_none() {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis builtin call must name a builtin target".to_string(),
                    });
                }
            }
            CallState::Plugin => {
                if target_kind.as_deref() != Some("plugin") || target_owner.is_none() {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis plugin call must name a plugin target owner".to_string(),
                    });
                }
            }
            CallState::Unresolved => {
                if target.is_some()
                    || target_kind.is_some()
                    || target_owner.is_some()
                    || !matches!(reason.as_deref(), Some("dynamic" | "unbound"))
                {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis unresolved call must have null target and reason dynamic/unbound"
                            .to_string(),
                    });
                }
            }
        }
        if state != CallState::Plugin && target_owner.is_some() {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis non-plugin call must not carry target_owner".to_string(),
            });
        }
        // Callee must be a legal static shape.
        match raw.callee.kind.as_str() {
            "ident" => {
                if raw.callee.name.as_deref().is_none_or(str::is_empty) {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis ident callee requires a name".to_string(),
                    });
                }
            }
            "member" => {
                if raw.callee.field.as_deref().is_none_or(str::is_empty)
                    || raw.callee.base.is_none()
                {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis member callee requires base/field".to_string(),
                    });
                }
            }
            "expression" => {
                if raw.callee.span.is_none() {
                    return Err(ViewError::AnalysisFailed {
                        detail: "Analysis expression callee requires a span".to_string(),
                    });
                }
            }
            other => {
                return Err(ViewError::AnalysisFailed {
                    detail: format!("Analysis callee kind {other:?} is unknown"),
                })
            }
        }
        let callee = match raw.callee.kind.as_str() {
            "ident" => raw.callee.name.clone().unwrap_or_default(),
            "member" => format!(
                "{}.{}",
                raw.callee
                    .base
                    .as_ref()
                    .and_then(|base| base.name.clone())
                    .unwrap_or_default(),
                raw.callee.field.clone().unwrap_or_default()
            ),
            _ => "表达式".to_string(),
        };
        let _ = unit_ids;
        calls.push(StaticCall {
            function: raw.function.clone(),
            span: raw.span.to_span(),
            callee,
            state,
            target,
            target_kind,
            target_owner,
            reason,
        });
    }
    Ok(calls)
}

fn convert_resources(
    module: &RawModule,
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
    document: &DocumentNode,
) -> ViewResult<Vec<StaticResource>> {
    let units: HashSet<&str> = module.units.iter().map(|unit| unit.id.as_str()).collect();
    let mut resources = Vec::with_capacity(module.resources.len());
    for raw in &module.resources {
        if !matches!(raw.kind.as_str(), "formula" | "code") {
            return Err(ViewError::AnalysisFailed {
                detail: format!("Analysis resource kind {:?} is not formula/code", raw.kind),
            });
        }
        if raw.module_id != module.id {
            return Err(ViewError::AnalysisFailed {
                detail: format!("Analysis resource {:?} names another module", raw.id),
            });
        }
        if !units.contains(raw.unit_id.as_str()) {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis resource {:?} names unknown unit {:?}",
                    raw.id, raw.unit_id
                ),
            });
        }
        let span = valid_span("resource.span", &raw.span, source_index, source_bytes)?;
        validate_sha_hex("resource.content_digest", &raw.content_digest)?;
        let language = opt_string("resource.language", &raw.language)?;
        let display = match &raw.display {
            Value::Null => None,
            Value::Bool(value) => Some(*value),
            _ => {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis resource display must be a boolean or null".to_string(),
                })
            }
        };
        if raw.kind == "code" {
            if display.is_some() || language.is_none() {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis code resource {:?} must have display null and a language",
                        raw.id
                    ),
                });
            }
        } else if display.is_none() || language.is_some() {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis formula resource {:?} must have a boolean display and null language",
                    raw.id
                ),
            });
        }
        if !raw.id.starts_with(&format!("{}#resource:", module.id)) {
            return Err(ViewError::AnalysisFailed {
                detail: format!("Analysis resource id {:?} is not module-local", raw.id),
            });
        }
        // The content digest is the SHA-256 of the AST literal at this span.
        let expected_kinds: &[&str] = match raw.kind.as_str() {
            "formula" => {
                if display == Some(true) {
                    &["math_block"]
                } else {
                    &["math"]
                }
            }
            "code" => &["code_block"],
            _ => &[],
        };
        let literal = document
            .walk()
            .into_iter()
            .find(|node| node.span == span && expected_kinds.contains(&node.kind.as_str()))
            .and_then(|node| node.literal.clone())
            .ok_or_else(|| ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis resource {:?} has no matching AST literal at its span",
                    raw.id
                ),
            })?;
        if sha256_hex(literal.as_bytes()) != raw.content_digest {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis resource {:?} content_digest does not match its AST literal",
                    raw.id
                ),
            });
        }
        resources.push(StaticResource {
            id: raw.id.clone(),
            module_id: raw.module_id.clone(),
            unit_id: raw.unit_id.clone(),
            kind: raw.kind.clone(),
            span,
            content_digest: raw.content_digest.clone(),
            language,
            display,
        });
    }
    Ok(resources)
}

fn convert_diagnostics(
    module: &RawModule,
    diagnostics: &[RawDiagnostic],
    source_index: &crate::reader::source::SourceIndex,
    source_bytes: &[u8],
) -> ViewResult<Vec<CoreDiagnostic>> {
    let mut out = Vec::with_capacity(diagnostics.len());
    let module_ids: HashSet<&str> = module.units.iter().map(|unit| unit.id.as_str()).collect();
    let _ = module_ids;
    for raw in diagnostics {
        if raw.code.is_empty() || raw.message.is_empty() {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis diagnostic code/message must not be empty".to_string(),
            });
        }
        if !matches!(raw.severity.as_str(), "error" | "warning") {
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "Analysis diagnostic severity {:?} is not error/warning",
                    raw.severity
                ),
            });
        }
        if raw.module != module.id {
            // Diagnostics may belong to another loaded module; we keep them
            // but only validate spans for this module's diagnostics.
            if !raw.module.contains("::") {
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Analysis diagnostic module {:?} is not canonical",
                        raw.module
                    ),
                });
            }
        }
        let (span, line, column) = if raw.module == module.id {
            let span = opt_span("diagnostic.span", &raw.span, source_index, source_bytes)?;
            let line = opt_u64("diagnostic.line", &raw.line)?;
            let column = opt_u64("diagnostic.column", &raw.column)?;
            if let Some(span) = span {
                let (expected_line, expected_column) = source_index.line_col(span.start as usize);
                if let Some(line) = line {
                    if line != expected_line as u64 {
                        return Err(ViewError::AnalysisFailed {
                            detail: "Analysis diagnostic line does not match its span".to_string(),
                        });
                    }
                }
                if let Some(column) = column {
                    if column != expected_column as u64 {
                        return Err(ViewError::AnalysisFailed {
                            detail: "Analysis diagnostic column does not match its span"
                                .to_string(),
                        });
                    }
                }
            } else if line.is_some() || column.is_some() {
                return Err(ViewError::AnalysisFailed {
                    detail: "Analysis diagnostic without a span must have null line/column"
                        .to_string(),
                });
            }
            (span, line, column)
        } else {
            (None, None, None)
        };
        if raw.relative_path.is_empty() {
            return Err(ViewError::AnalysisFailed {
                detail: "Analysis diagnostic relative_path must not be empty".to_string(),
            });
        }
        out.push(CoreDiagnostic {
            module: raw.module.clone(),
            relative_path: raw.relative_path.clone(),
            span,
            line,
            column,
            severity: raw.severity.clone(),
            code: raw.code.clone(),
            message: raw.message.clone(),
            hints: raw.hints.clone(),
        });
    }
    Ok(out)
}

fn opt_u64(field: &str, value: &Value) -> ViewResult<Option<u64>> {
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => {
            number
                .as_u64()
                .map(Some)
                .ok_or_else(|| ViewError::AnalysisFailed {
                    detail: format!("Analysis {field} must be a non-negative integer"),
                })
        }
        _ => Err(ViewError::AnalysisFailed {
            detail: format!("Analysis {field} must be a non-negative integer or null"),
        }),
    }
}
