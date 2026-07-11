use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::app::AppError;
use crate::{checksum, ledger, paths, state};

const SCHEMA_VERSION: u32 = 1;
const MAX_SEEDED_FILES: usize = 256;
const MAX_INDEXED_FILE_BYTES: u64 = 1024 * 1024;
const SOURCE_POINTER_NONE: &str = "none";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OntologyExportFormat {
    Json,
    Jsonl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologySeedOutcome {
    pub store: PathBuf,
    pub schema: PathBuf,
    pub records_added: usize,
    pub current_records: usize,
    pub layer_a_records: usize,
    pub layer_b_records: usize,
    pub event_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContextRecord {
    pub id: String,
    pub layer: String,
    pub kind: String,
    pub label: String,
    pub source_pointer: String,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContextSelection {
    pub current_records: usize,
    pub selected: Vec<RuntimeContextRecord>,
    pub stale_rejected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSourceRead {
    pub relative_path: String,
    pub stable_ref: String,
    pub source_hash: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OntologyRecord {
    id: String,
    layer: String,
    kind: String,
    label: String,
    status: String,
    claim_state: String,
    confidence: String,
    source_pointer: String,
    source_hash: String,
    evidence: String,
    supersedes: String,
    current: bool,
    event_id: String,
    created_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OntologyProjection {
    total_records: usize,
    current_records: Vec<OntologyRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OntologyDiagnostics {
    total_records: usize,
    current_records: usize,
    layer_a_records: usize,
    layer_b_records: usize,
    stale_layer_a: usize,
    sourceless_confirmed_layer_b: usize,
    open_questions: usize,
}

pub fn ensure_seeded() -> Result<OntologySeedOutcome, AppError> {
    ensure_layout()?;
    let projection = load_projection()?;
    let candidates = seed_candidates()?;
    let existing_by_id = projection
        .current_records
        .iter()
        .map(|record| (record.id.clone(), record.clone()))
        .collect::<HashMap<_, _>>();

    let mut records_to_append = Vec::new();
    for mut candidate in candidates {
        match existing_by_id.get(&candidate.id) {
            Some(existing) if seeded_record_changed(existing, &candidate) => {
                candidate.supersedes = record_revision_pointer(existing);
                candidate.created_at_ms = now_ms();
                records_to_append.push(candidate);
            }
            Some(_) => {}
            None => {
                candidate.created_at_ms = now_ms();
                records_to_append.push(candidate);
            }
        }
    }

    let event_type = if records_to_append.is_empty() {
        "ontology.seed.noop"
    } else {
        "ontology.seed"
    };
    let event_id = state::record_event(
        event_type,
        "ontology Layer A seed",
        &format!(
            "store={} added_records={} canonical=typed-graph-jsonl",
            paths::project_ontology_store_file().display(),
            records_to_append.len()
        ),
    )?;

    for record in &mut records_to_append {
        record.event_id = event_id.clone();
    }
    append_records(&records_to_append)?;

    let projection = load_projection()?;
    let diagnostics = diagnostics_from_projection(&projection);

    Ok(OntologySeedOutcome {
        store: paths::project_ontology_store_file(),
        schema: paths::project_ontology_schema_file(),
        records_added: records_to_append.len(),
        current_records: diagnostics.current_records,
        layer_a_records: diagnostics.layer_a_records,
        layer_b_records: diagnostics.layer_b_records,
        event_id,
    })
}

pub fn seed_report() -> Result<String, AppError> {
    let outcome = ensure_seeded()?;
    Ok(format!(
        "ontology seed 결과\n- store: {}\n- schema: {}\n- added records: {}\n- current records: {}\n- layer A facts: {}\n- layer B claims: {}\n- ledger event: {}\n- canonical: runtime typed graph JSONL\n- boundary: raw source text는 store에 장기 저장하지 않고 source pointer와 hash만 저장합니다.",
        outcome.store.display(),
        outcome.schema.display(),
        outcome.records_added,
        outcome.current_records,
        outcome.layer_a_records,
        outcome.layer_b_records,
        outcome.event_id
    ))
}

pub fn status_report() -> Result<String, AppError> {
    ensure_layout()?;
    let diagnostics = diagnostics_from_projection(&load_projection()?);
    Ok(format!(
        "ontology status\n- store: {}\n- schema: {}\n- total records: {}\n- current projection: {}\n- layer A deterministic facts: {}\n- layer B semantic claims: {}\n- stale Layer A source hashes: {}\n- sourceless confirmed Layer B claims: {}\n- open questions: {}\n- compact context: `rpotato ontology context --query <text>`\n- source reread: `rpotato ontology reread <source-pointer>`\n- export views: json, jsonl\n- boundary: JSON/YAML/RDF/OWL은 inspection/export view이며 runtime source of truth는 이 typed graph store입니다.",
        paths::project_ontology_store_file().display(),
        paths::project_ontology_schema_file().display(),
        diagnostics.total_records,
        diagnostics.current_records,
        diagnostics.layer_a_records,
        diagnostics.layer_b_records,
        diagnostics.stale_layer_a,
        diagnostics.sourceless_confirmed_layer_b,
        diagnostics.open_questions
    ))
}

pub fn inspect_report() -> Result<String, AppError> {
    ensure_layout()?;
    let projection = load_projection()?;
    let diagnostics = diagnostics_from_projection(&projection);
    let rows = projection
        .current_records
        .iter()
        .take(20)
        .map(format_record_row)
        .collect::<Vec<_>>()
        .join("\n");
    let rows = if rows.is_empty() {
        "- records: 없음; `rpotato ontology seed`를 실행하세요.".to_string()
    } else {
        rows
    };

    Ok(format!(
        "ontology inspect\n- current projection: {}\n- stale Layer A source hashes: {}\n- sourceless confirmed Layer B claims: {}\n{}\n- rule: compact view는 source pointer를 먼저 보여주며, patch 전에는 반드시 `ontology reread`로 원문을 다시 읽어야 합니다.",
        diagnostics.current_records,
        diagnostics.stale_layer_a,
        diagnostics.sourceless_confirmed_layer_b,
        rows
    ))
}

pub fn context_report(query: &str) -> Result<String, AppError> {
    if query.trim().is_empty() {
        return Err(AppError::usage(
            "ontology context에는 --query <text> 값이 필요합니다.",
        ));
    }

    ensure_layout()?;
    let projection = load_projection()?;
    let selected = select_context_records(&projection.current_records, query, 12);
    let rows = selected
        .iter()
        .map(format_context_row)
        .collect::<Vec<_>>()
        .join("\n");
    let rows = if rows.is_empty() {
        "- selected: 없음; 먼저 `rpotato ontology seed`로 Layer A fact를 생성하세요.".to_string()
    } else {
        rows
    };

    Ok(format!(
        "ontology context view\n- query: {}\n- selected records: {}\n- prompt rule: source-pointer-first, original-file reread before edits\n- raw source text stored: false\n{}\n- boundary: 이 출력은 small-model prompt용 compact view이며 canonical store 자체가 아닙니다.",
        query,
        selected.len(),
        rows
    ))
}

pub fn runtime_context(query: &str, limit: usize) -> Result<RuntimeContextSelection, AppError> {
    ensure_layout()?;
    let projection = load_projection()?;
    let mut selected = select_context_records(&projection.current_records, query, limit);
    if selected.is_empty() {
        selected = projection
            .current_records
            .iter()
            .filter(|record| {
                record.layer == "A"
                    && matches!(
                        record.kind.as_str(),
                        "entrypoint" | "package-manager" | "file"
                    )
            })
            .take(limit)
            .cloned()
            .collect();
    }

    let mut stale_rejected = 0;
    let selected = selected
        .into_iter()
        .filter_map(|record| {
            if record_source_is_stale(&record) {
                stale_rejected += 1;
                return None;
            }
            Some(RuntimeContextRecord {
                id: record.id,
                layer: record.layer,
                kind: record.kind,
                label: record.label,
                source_pointer: record.source_pointer,
                source_hash: record.source_hash,
            })
        })
        .collect();

    Ok(RuntimeContextSelection {
        current_records: projection.current_records.len(),
        selected,
        stale_rejected,
    })
}

pub fn reread_runtime_source(
    pointer: &str,
    expected_hash: &str,
) -> Result<RuntimeSourceRead, AppError> {
    let source = resolve_source_pointer(pointer)?;
    let current_hash = checksum::sha256_file(&source.path)?;
    if current_hash != expected_hash {
        return Err(AppError::blocked(format!(
            "ontology source reread 차단\n- source pointer: {pointer}\n- 이유: graph source hash와 현재 원문 hash가 다릅니다.\n- 동작: ontology seed를 갱신한 뒤 다시 시도하세요."
        )));
    }
    let contents = fs::read_to_string(&source.path).map_err(|err| {
        AppError::runtime(format!(
            "ontology source 원문을 읽지 못했습니다: {} ({err})",
            source.path.display()
        ))
    })?;
    let root = canonical_project_root()?;
    let relative_path = relative_to_root(&source.path, &root)
        .ok_or_else(|| AppError::blocked("ontology source가 project boundary를 벗어났습니다."))?;
    Ok(RuntimeSourceRead {
        relative_path,
        stable_ref: pointer.to_string(),
        source_hash: current_hash,
        contents,
    })
}

pub fn reread_report(pointer: &str) -> Result<String, AppError> {
    let source = resolve_source_pointer(pointer)?;
    let contents = fs::read_to_string(&source.path).map_err(|err| {
        AppError::runtime(format!(
            "source pointer 원문을 읽지 못했습니다: {} ({err})",
            source.path.display()
        ))
    })?;
    let hash = checksum::sha256_file(&source.path)?;
    let excerpt = contents
        .lines()
        .nth(source.line.saturating_sub(1))
        .unwrap_or("");

    Ok(format!(
        "ontology reread 결과\n- source pointer: {}\n- file: {}\n- line: {}\n- current sha256: {}\n- excerpt:\n  {} | {}\n- rule: 이 원문이 authoritative source입니다. Ontology snippet만 근거로 patch하지 않습니다.",
        pointer,
        source.path.display(),
        source.line,
        hash,
        source.line,
        excerpt
    ))
}

pub fn export_report(format: OntologyExportFormat) -> Result<String, AppError> {
    ensure_layout()?;
    match format {
        OntologyExportFormat::Jsonl => {
            let contents =
                fs::read_to_string(paths::project_ontology_store_file()).map_err(|err| {
                    AppError::runtime(format!(
                        "ontology store를 읽지 못했습니다: {} ({err})",
                        paths::project_ontology_store_file().display()
                    ))
                })?;
            Ok(contents)
        }
        OntologyExportFormat::Json => {
            let projection = load_projection()?;
            let records = projection
                .current_records
                .iter()
                .map(|record| format!("    {}", record.to_json_line()))
                .collect::<Vec<_>>()
                .join(",\n");
            Ok(format!(
                "{{\n  \"schemaVersion\": {},\n  \"viewAuthority\": \"inspection-only\",\n  \"canonicalStore\": \"{}\",\n  \"records\": [\n{}\n  ]\n}}\n",
                SCHEMA_VERSION,
                ledger::json_string(&paths::project_ontology_store_file().display().to_string()),
                records
            ))
        }
    }
}

pub fn import_report(path: &str, dry_run: bool) -> Result<String, AppError> {
    if !dry_run {
        return Err(AppError::blocked(
            "ontology import는 현재 --dry-run만 허용합니다. 외부 view를 canonical store로 바로 승격하지 않습니다.",
        ));
    }

    let path = resolve_project_relative_file(path)?;
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "ontology import file을 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let validation = validate_import_text(&contents)?;

    Ok(format!(
        "ontology import dry-run 결과\n- file: {}\n- schemaVersion: {}\n- inspected records: {}\n- sourceless confirmed Layer B claims: 0\n- mutation: 없음\n- boundary: import file은 inspection/migration 후보이며 canonical store로 자동 승격하지 않습니다.",
        path.display(),
        SCHEMA_VERSION,
        validation.records
    ))
}

pub fn doctor_summary() -> String {
    match status_summary() {
        Ok(summary) => summary,
        Err(err) => format!("ontology 진단 실패: {}", err.message),
    }
}

fn status_summary() -> Result<String, AppError> {
    ensure_layout()?;
    let diagnostics = diagnostics_from_projection(&load_projection()?);
    Ok(format!(
        "ontology store {}, current {}, stale Layer A {}, sourceless confirmed Layer B {}",
        paths::project_ontology_store_file().display(),
        diagnostics.current_records,
        diagnostics.stale_layer_a,
        diagnostics.sourceless_confirmed_layer_b
    ))
}

fn ensure_layout() -> Result<(), AppError> {
    fs::create_dir_all(paths::project_ontology_dir()).map_err(|err| {
        AppError::runtime(format!(
            "ontology 디렉터리를 만들지 못했습니다: {} ({err})",
            paths::project_ontology_dir().display()
        ))
    })?;

    if !paths::project_ontology_store_file().exists() {
        fs::write(paths::project_ontology_store_file(), "").map_err(|err| {
            AppError::runtime(format!(
                "ontology store를 만들지 못했습니다: {} ({err})",
                paths::project_ontology_store_file().display()
            ))
        })?;
    }

    if !paths::project_ontology_schema_file().exists() {
        fs::write(paths::project_ontology_schema_file(), schema_body()).map_err(|err| {
            AppError::runtime(format!(
                "ontology schema file을 만들지 못했습니다: {} ({err})",
                paths::project_ontology_schema_file().display()
            ))
        })?;
    }

    Ok(())
}

fn schema_body() -> String {
    format!(
        "{{\n  \"schemaVersion\": {},\n  \"canonical\": \"runtime-typed-graph-jsonl\",\n  \"layers\": [\"A\", \"B\"],\n  \"claimStates\": [\"confirmed\", \"proposed\", \"weak\", \"superseded\", \"rejected\", \"open_question\"],\n  \"requiredSourceForConfirmedSemanticClaims\": true,\n  \"rawSourceRetention\": \"source-pointer-and-hash-only\"\n}}\n",
        SCHEMA_VERSION
    )
}

fn seed_candidates() -> Result<Vec<OntologyRecord>, AppError> {
    let root = canonical_project_root()?;
    let mut files = Vec::new();
    collect_indexable_files(&root, &root, 0, &mut files)?;
    files.sort();
    files.truncate(MAX_SEEDED_FILES);

    let mut records = Vec::new();
    let mut seen_ids = HashSet::new();
    for path in &files {
        let Some(relative) = relative_to_root(path, &root) else {
            continue;
        };
        let hash = checksum::sha256_file(path)?;
        let record = layer_a_record(
            "file",
            &format!("file {relative}"),
            &relative,
            &hash,
            &format!("indexed-file-bytes:{hash}"),
        );
        push_unique(&mut records, &mut seen_ids, record);
    }

    for path in package_manifest_paths(&root) {
        if !path.exists() {
            continue;
        }
        let Some(relative) = relative_to_root(&path, &root) else {
            continue;
        };
        let hash = checksum::sha256_file(&path)?;
        push_unique(
            &mut records,
            &mut seen_ids,
            layer_a_record(
                "package-manager",
                &format!("package manifest {relative}"),
                &relative,
                &hash,
                &format!("detected-package-manifest:{relative}"),
            ),
        );
    }

    for path in entrypoint_paths(&root) {
        if !path.exists() {
            continue;
        }
        let Some(relative) = relative_to_root(&path, &root) else {
            continue;
        };
        let hash = checksum::sha256_file(&path)?;
        push_unique(
            &mut records,
            &mut seen_ids,
            layer_a_record(
                "entrypoint",
                &format!("runtime entrypoint {relative}"),
                &relative,
                &hash,
                &format!("detected-entrypoint:{relative}"),
            ),
        );
    }

    let gitignore = root.join(".gitignore");
    if gitignore.exists() {
        let hash = checksum::sha256_file(&gitignore)?;
        push_unique(
            &mut records,
            &mut seen_ids,
            layer_a_record(
                "generated-exclusion",
                "generated exclusion rules .gitignore",
                ".gitignore",
                &hash,
                "detected-generated-exclusion-rules",
            ),
        );
    }

    Ok(records)
}

fn layer_a_record(
    kind: &str,
    label: &str,
    relative_path: &str,
    source_hash: &str,
    evidence: &str,
) -> OntologyRecord {
    OntologyRecord {
        id: format!("a:{kind}:{}", stable_id(relative_path)),
        layer: "A".to_string(),
        kind: kind.to_string(),
        label: label.to_string(),
        status: "confirmed".to_string(),
        claim_state: "confirmed".to_string(),
        confidence: "1.00".to_string(),
        source_pointer: format!("{relative_path}:1"),
        source_hash: source_hash.to_string(),
        evidence: evidence.to_string(),
        supersedes: String::new(),
        current: true,
        event_id: "pending".to_string(),
        created_at_ms: 0,
    }
}

fn push_unique(
    records: &mut Vec<OntologyRecord>,
    seen_ids: &mut HashSet<String>,
    record: OntologyRecord,
) {
    if seen_ids.insert(record.id.clone()) {
        records.push(record);
    }
}

fn collect_indexable_files(
    root: &Path,
    current: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    if depth > 8 || files.len() >= MAX_SEEDED_FILES {
        return Ok(());
    }

    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(err) => {
            return Err(AppError::runtime(format!(
                "ontology seed 대상 디렉터리를 읽지 못했습니다: {} ({err})",
                current.display()
            )));
        }
    };

    for entry in entries {
        if files.len() >= MAX_SEEDED_FILES {
            break;
        }
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "ontology seed 대상 항목을 읽지 못했습니다: {} ({err})",
                current.display()
            ))
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|err| {
            AppError::runtime(format!(
                "ontology seed 대상 항목 타입을 읽지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;
        let Some(relative) = relative_to_root(&path, root) else {
            continue;
        };

        if file_type.is_dir() {
            if should_skip_dir(&relative) {
                continue;
            }
            collect_indexable_files(root, &path, depth + 1, files)?;
        } else if file_type.is_file() && should_index_file(&path, &relative)? {
            files.push(path);
        }
    }

    Ok(())
}

fn should_skip_dir(relative: &str) -> bool {
    relative.split('/').any(|part| {
        matches!(
            part,
            ".git" | ".rpotato" | "target" | "node_modules" | ".next" | "dist" | "build"
        )
    })
}

fn should_index_file(path: &Path, relative: &str) -> Result<bool, AppError> {
    if relative.starts_with(".rpotato/") || relative.starts_with(".git/") {
        return Ok(false);
    }
    let metadata = fs::metadata(path).map_err(|err| {
        AppError::runtime(format!(
            "ontology seed 대상 파일 metadata를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if metadata.len() > MAX_INDEXED_FILE_BYTES {
        return Ok(false);
    }
    if matches!(
        relative,
        "Cargo.lock" | "Cargo.toml" | "README.md" | "README.ko.md" | "AGENTS.md"
    ) {
        return Ok(true);
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    Ok(matches!(
        extension,
        "rs" | "toml" | "md" | "yml" | "yaml" | "json" | "sh" | "txt"
    ))
}

fn package_manifest_paths(root: &Path) -> Vec<PathBuf> {
    ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"]
        .iter()
        .map(|path| root.join(path))
        .collect()
}

fn entrypoint_paths(root: &Path) -> Vec<PathBuf> {
    ["src/main.rs", "src/lib.rs", "main.py", "package.json"]
        .iter()
        .map(|path| root.join(path))
        .collect()
}

fn append_records(records: &[OntologyRecord]) -> Result<(), AppError> {
    if records.is_empty() {
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::project_ontology_store_file())
        .map_err(|err| {
            AppError::runtime(format!(
                "ontology store를 열지 못했습니다: {} ({err})",
                paths::project_ontology_store_file().display()
            ))
        })?;

    for record in records {
        writeln!(file, "{}", record.to_json_line()).map_err(|err| {
            AppError::runtime(format!(
                "ontology record를 기록하지 못했습니다: {} ({err})",
                paths::project_ontology_store_file().display()
            ))
        })?;
    }

    Ok(())
}

fn load_projection() -> Result<OntologyProjection, AppError> {
    let path = paths::project_ontology_store_file();
    if !path.exists() {
        return Ok(OntologyProjection {
            total_records: 0,
            current_records: Vec::new(),
        });
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "ontology store를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let mut latest_by_id = HashMap::new();
    let mut total_records = 0;
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        total_records += 1;
        if let Some(record) = OntologyRecord::parse(line) {
            latest_by_id.insert(record.id.clone(), record);
        }
    }

    let mut current_records = latest_by_id
        .into_values()
        .filter(|record| record.current)
        .collect::<Vec<_>>();
    current_records.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(OntologyProjection {
        total_records,
        current_records,
    })
}

fn seeded_record_changed(existing: &OntologyRecord, candidate: &OntologyRecord) -> bool {
    existing.layer != candidate.layer
        || existing.kind != candidate.kind
        || existing.label != candidate.label
        || existing.status != candidate.status
        || existing.claim_state != candidate.claim_state
        || existing.source_pointer != candidate.source_pointer
        || existing.source_hash != candidate.source_hash
        || existing.evidence != candidate.evidence
}

fn record_revision_pointer(record: &OntologyRecord) -> String {
    format!(
        "{}@{}",
        record.id,
        if record.event_id.is_empty() {
            record.created_at_ms.to_string()
        } else {
            record.event_id.clone()
        }
    )
}

fn diagnostics_from_projection(projection: &OntologyProjection) -> OntologyDiagnostics {
    let layer_a_records = projection
        .current_records
        .iter()
        .filter(|record| record.layer == "A")
        .count();
    let layer_b_records = projection
        .current_records
        .iter()
        .filter(|record| record.layer == "B")
        .count();
    let stale_layer_a = projection
        .current_records
        .iter()
        .filter(|record| record.layer == "A" && record_source_is_stale(record))
        .count();
    let sourceless_confirmed_layer_b = projection
        .current_records
        .iter()
        .filter(|record| semantic_claim_is_sourceless_confirmed(record))
        .count();
    let open_questions = projection
        .current_records
        .iter()
        .filter(|record| record.status == "open_question" || record.claim_state == "open_question")
        .count();

    OntologyDiagnostics {
        total_records: projection.total_records,
        current_records: projection.current_records.len(),
        layer_a_records,
        layer_b_records,
        stale_layer_a,
        sourceless_confirmed_layer_b,
        open_questions,
    }
}

fn record_source_is_stale(record: &OntologyRecord) -> bool {
    let Ok(source) = resolve_source_pointer(&record.source_pointer) else {
        return true;
    };
    checksum::sha256_file(&source.path)
        .map(|current| current != record.source_hash)
        .unwrap_or(true)
}

fn semantic_claim_is_sourceless_confirmed(record: &OntologyRecord) -> bool {
    if record.layer != "B" {
        return false;
    }
    if record.status != "confirmed" && record.claim_state != "confirmed" {
        return false;
    }
    record.source_pointer.trim().is_empty()
        || record.source_pointer == SOURCE_POINTER_NONE
        || record.source_hash.trim().is_empty()
}

fn select_context_records(
    records: &[OntologyRecord],
    query: &str,
    limit: usize,
) -> Vec<OntologyRecord> {
    let terms = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    let mut scored = records
        .iter()
        .map(|record| {
            let haystack = format!(
                "{} {} {} {} {}",
                record.id, record.kind, record.label, record.evidence, record.source_pointer
            )
            .to_ascii_lowercase();
            let score = terms
                .iter()
                .filter(|term| haystack.contains(term.as_str()))
                .count();
            (score, record)
        })
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();
    scored.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.layer.cmp(&right.layer))
            .then_with(|| left.id.cmp(&right.id))
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(_, record)| record.clone())
        .collect()
}

fn format_record_row(record: &OntologyRecord) -> String {
    format!(
        "- [{}:{}:{}] {} | source {} | hash {} | id {}",
        record.layer,
        record.kind,
        record.claim_state,
        record.label,
        record.source_pointer,
        short_hash(&record.source_hash),
        record.id
    )
}

fn format_context_row(record: &OntologyRecord) -> String {
    format!(
        "- source={} | {}:{}:{} | {} | id={}",
        record.source_pointer,
        record.layer,
        record.kind,
        record.claim_state,
        record.label,
        record.id
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourcePointer {
    path: PathBuf,
    line: usize,
}

fn resolve_source_pointer(pointer: &str) -> Result<SourcePointer, AppError> {
    if pointer.trim().is_empty() || pointer == SOURCE_POINTER_NONE {
        return Err(AppError::usage(
            "source pointer가 필요합니다. 예: src/main.rs:1",
        ));
    }
    if pointer.contains("://") {
        return Err(AppError::blocked(
            "source pointer는 remote URL을 허용하지 않습니다.",
        ));
    }
    let Some((relative, line)) = pointer.rsplit_once(':') else {
        return Err(AppError::usage(
            "source pointer는 <project-relative-path>:<line> 형식이어야 합니다.",
        ));
    };
    let line = line
        .parse::<usize>()
        .map_err(|_| AppError::usage("source pointer line은 양의 정수여야 합니다."))?;
    if line == 0 {
        return Err(AppError::usage(
            "source pointer line은 1 이상이어야 합니다.",
        ));
    }

    let path = resolve_project_relative_file(relative)?;
    Ok(SourcePointer { path, line })
}

fn resolve_project_relative_file(relative: &str) -> Result<PathBuf, AppError> {
    if relative.trim().is_empty() {
        return Err(AppError::usage("project-relative path가 필요합니다."));
    }
    if relative.contains("://") {
        return Err(AppError::blocked("remote path는 허용하지 않습니다."));
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() {
        return Err(AppError::blocked(
            "project-relative path만 허용합니다. absolute path는 거부됩니다.",
        ));
    }
    if relative_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::blocked(
            "project-relative path는 상위 경로(..)를 포함할 수 없습니다.",
        ));
    }

    let root = canonical_project_root()?;
    let candidate = root.join(relative_path);
    if !candidate.exists() {
        return Err(AppError::usage(format!(
            "project file이 존재하지 않습니다: {}",
            candidate.display()
        )));
    }
    let canonical = fs::canonicalize(&candidate).map_err(|err| {
        AppError::runtime(format!(
            "project file을 canonicalize하지 못했습니다: {} ({err})",
            candidate.display()
        ))
    })?;
    if !canonical.starts_with(&root) {
        return Err(AppError::blocked(format!(
            "project boundary를 벗어난 path입니다: {}",
            canonical.display()
        )));
    }
    if !canonical.is_file() {
        return Err(AppError::usage(format!(
            "project file path가 파일이 아닙니다: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn validate_import_text(text: &str) -> Result<ImportValidation, AppError> {
    let schema_version = extract_json_u64_tolerant(text, "schemaVersion").ok_or_else(|| {
        AppError::usage("ontology import file에는 schemaVersion: 1이 필요합니다.")
    })?;
    if schema_version != u64::from(SCHEMA_VERSION) {
        return Err(AppError::usage(format!(
            "ontology import schemaVersion은 {}이어야 합니다: {}",
            SCHEMA_VERSION, schema_version
        )));
    }

    let mut records = 0;
    for line in text.lines().filter(|line| line.contains("\"id\"")) {
        records += 1;
        let layer = extract_json_string_tolerant(line, "layer").unwrap_or_default();
        let status = extract_json_string_tolerant(line, "status").unwrap_or_default();
        let claim_state = extract_json_string_tolerant(line, "claimState").unwrap_or_default();
        let source_pointer =
            extract_json_string_tolerant(line, "sourcePointer").unwrap_or_default();
        let source_hash = extract_json_string_tolerant(line, "sourceHash").unwrap_or_default();
        if layer == "B"
            && (status == "confirmed" || claim_state == "confirmed")
            && (source_pointer.trim().is_empty()
                || source_pointer == SOURCE_POINTER_NONE
                || source_hash.trim().is_empty())
        {
            return Err(AppError::blocked(
                "ontology import 차단: confirmed Layer B semantic claim에는 sourcePointer와 sourceHash가 필요합니다.",
            ));
        }
    }

    if records == 0 {
        records = text.matches("\"schemaVersion\"").count().saturating_sub(1);
    }

    Ok(ImportValidation { records })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportValidation {
    records: usize,
}

impl OntologyRecord {
    fn to_json_line(&self) -> String {
        format!(
            "{{\"schemaVersion\":{},\"id\":\"{}\",\"layer\":\"{}\",\"kind\":\"{}\",\"label\":\"{}\",\"status\":\"{}\",\"claimState\":\"{}\",\"confidence\":\"{}\",\"sourcePointer\":\"{}\",\"sourceHash\":\"{}\",\"evidence\":\"{}\",\"supersedes\":\"{}\",\"current\":{},\"eventId\":\"{}\",\"createdAtMs\":{}}}",
            SCHEMA_VERSION,
            ledger::json_string(&self.id),
            ledger::json_string(&self.layer),
            ledger::json_string(&self.kind),
            ledger::json_string(&self.label),
            ledger::json_string(&self.status),
            ledger::json_string(&self.claim_state),
            ledger::json_string(&self.confidence),
            ledger::json_string(&self.source_pointer),
            ledger::json_string(&self.source_hash),
            ledger::json_string(&self.evidence),
            ledger::json_string(&self.supersedes),
            self.current,
            ledger::json_string(&self.event_id),
            self.created_at_ms
        )
    }

    fn parse(line: &str) -> Option<Self> {
        let schema_version = extract_json_u64(line, "schemaVersion")?;
        if schema_version != u64::from(SCHEMA_VERSION) {
            return None;
        }
        Some(Self {
            id: extract_json_string(line, "id")?,
            layer: extract_json_string(line, "layer")?,
            kind: extract_json_string(line, "kind")?,
            label: extract_json_string(line, "label")?,
            status: extract_json_string(line, "status")?,
            claim_state: extract_json_string(line, "claimState")?,
            confidence: extract_json_string(line, "confidence")?,
            source_pointer: extract_json_string(line, "sourcePointer")?,
            source_hash: extract_json_string(line, "sourceHash")?,
            evidence: extract_json_string(line, "evidence")?,
            supersedes: extract_json_string(line, "supersedes").unwrap_or_default(),
            current: extract_json_bool(line, "current").unwrap_or(true),
            event_id: extract_json_string(line, "eventId").unwrap_or_default(),
            created_at_ms: extract_json_u128(line, "createdAtMs").unwrap_or_default(),
        })
    }
}

fn stable_id(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    bytes_to_hex(&hasher.finalize())[..16].to_string()
}

fn short_hash(value: &str) -> String {
    if value.len() <= 12 {
        value.to_string()
    } else {
        value[..12].to_string()
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn canonical_project_root() -> Result<PathBuf, AppError> {
    let root = paths::project_root();
    fs::create_dir_all(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 만들지 못했습니다: {} ({err})",
            root.display()
        ))
    })?;
    fs::canonicalize(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 canonicalize하지 못했습니다: {} ({err})",
            root.display()
        ))
    })
}

fn relative_to_root(path: &Path, root: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn extract_json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = text.find(&needle)? + needle.len();
    parse_json_string_tail(&text[start..])
}

fn extract_json_string_tolerant(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    parse_json_string_tail(rest)
}

fn parse_json_string_tail(text: &str) -> Option<String> {
    let mut value = String::new();
    let mut escaped = false;
    for ch in text.chars() {
        if escaped {
            match ch {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => value.push(other),
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}

fn extract_json_u64(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn extract_json_u64_tolerant(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start().strip_prefix(':')?.trim_start();
    rest.chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn extract_json_u128(text: &str, key: &str) -> Option<u128> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn extract_json_bool(text: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    if text[start..].starts_with("true") {
        Some(true)
    } else if text[start..].starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_temp_project(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("rpotato-ontology-{name}-{}", std::process::id()));
        let project = root.join("project");
        let data = root.join("data");
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        fs::write(project.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(project.join(".gitignore"), "target/\n.rpotato/\n").unwrap();
        project
    }

    fn clear_env() {
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
    }

    #[test]
    fn seed_creates_store_and_context_view() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let _project = with_temp_project("seed");

        let seed = ensure_seeded().unwrap();
        let context = context_report("main").unwrap();
        let status = status_report().unwrap();

        clear_env();

        assert!(seed.records_added >= 2);
        assert!(seed.store.exists());
        assert!(context.contains("source=src/main.rs:1"));
        assert!(status.contains("sourceless confirmed Layer B claims: 0"));
    }

    #[test]
    fn reread_rejects_parent_path_escape() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let _project = with_temp_project("reread-escape");

        let err = reread_report("../secret.txt:1").unwrap_err();

        clear_env();

        assert_eq!(err.code, 3);
    }

    #[test]
    fn changed_layer_a_seed_appends_superseding_revision() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let project = with_temp_project("supersedes");

        ensure_seeded().unwrap();
        fs::write(
            project.join("src").join("main.rs"),
            "fn main() { println!(\"hi\"); }\n",
        )
        .unwrap();
        let seed = ensure_seeded().unwrap();
        let store = fs::read_to_string(paths::project_ontology_store_file()).unwrap();

        clear_env();

        assert_eq!(seed.records_added, 2);
        assert!(store.contains("\"supersedes\":\"a:file:"));
        assert!(store.contains("\"supersedes\":\"a:entrypoint:"));
    }

    #[test]
    fn runtime_context_binds_reread_to_graph_hash() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let project = with_temp_project("runtime-context");
        ensure_seeded().unwrap();

        let selection = runtime_context("main", 4).unwrap();
        let record = selection
            .selected
            .iter()
            .find(|record| record.source_pointer == "src/main.rs:1")
            .unwrap();
        let source = reread_runtime_source(&record.source_pointer, &record.source_hash).unwrap();
        assert_eq!(source.relative_path, "src/main.rs");
        assert_eq!(source.contents, "fn main() {}\n");

        fs::write(project.join("src/main.rs"), "fn main() { panic!(); }\n").unwrap();
        let err = reread_runtime_source(&record.source_pointer, &record.source_hash).unwrap_err();
        clear_env();

        assert_eq!(err.code, 3);
        assert!(err.message.contains("graph source hash"));
    }

    #[test]
    fn import_blocks_confirmed_semantic_claim_without_source() {
        let text = r#"{"schemaVersion":1,"id":"b:one","layer":"B","kind":"invariant","label":"must be true","status":"confirmed","claimState":"confirmed","sourcePointer":"none","sourceHash":""}"#;

        let err = validate_import_text(text).unwrap_err();

        assert_eq!(err.code, 3);
        assert!(err.message.contains("confirmed Layer B"));
    }

    #[test]
    fn import_accepts_source_backed_confirmed_semantic_claim() {
        let text = r#"{"schemaVersion":1,"id":"b:one","layer":"B","kind":"invariant","label":"must be true","status":"confirmed","claimState":"confirmed","sourcePointer":"docs/design.md:10","sourceHash":"abc"}"#;

        let validation = validate_import_text(text).unwrap();

        assert_eq!(validation.records, 1);
    }
}
