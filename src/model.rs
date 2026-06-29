use std::fs;
use std::path::PathBuf;

use crate::app::AppError;
use crate::{ledger, paths, state};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateStatus {
    Candidate,
    Unverified,
    Verified,
}

const STATUS_SCHEMA: &[CandidateStatus] = &[
    CandidateStatus::Candidate,
    CandidateStatus::Unverified,
    CandidateStatus::Verified,
];

#[derive(Debug, Clone, Copy)]
struct SourceClaim {
    claim: &'static str,
    source: &'static str,
    checked_at: &'static str,
    status: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct BenchmarkClaim {
    source: &'static str,
    checked_at: &'static str,
    claim_status: &'static str,
    harness: &'static str,
    dataset: &'static str,
    prompt: &'static str,
    scoring: &'static str,
    hardware_backend: &'static str,
    reproducibility: &'static str,
}

#[derive(Debug)]
struct ModelManifestEntry {
    id: &'static str,
    display_name: &'static str,
    status: CandidateStatus,
    role: &'static str,
    upstream_model: &'static str,
    upstream_url: &'static str,
    format: &'static str,
    backend: &'static str,
    license: SourceClaim,
    artifact_provider: Option<&'static str>,
    artifact_url: Option<&'static str>,
    artifact_terms_url: Option<&'static str>,
    artifact_name: Option<&'static str>,
    quantization: Option<&'static str>,
    sha256: Option<&'static str>,
    size_bytes: Option<u64>,
    context_length: Option<u32>,
    recommended_ram_gb: Option<u32>,
    backend_compatibility: Option<SourceClaim>,
    benchmark: BenchmarkClaim,
    install_blockers: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallValidation {
    ready: bool,
    blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryEntry {
    id: String,
    display_name: String,
    status: String,
    artifact_path: String,
    artifact_sha256: String,
}

const NO_EXTRA_BLOCKERS: &[&str] = &[];
const QWEN_9B_BLOCKERS: &[&str] = &["제품 기본값 보류", "16 GB runtime fit 미측정"];

const CANDIDATES: &[ModelManifestEntry] = &[
    ModelManifestEntry {
        id: "qwen3.5-4b",
        display_name: "Qwen3.5 4B GGUF",
        status: CandidateStatus::Candidate,
        role: "우선 평가 후보",
        upstream_model: "Qwen/Qwen3.5-4B",
        upstream_url: "https://huggingface.co/Qwen/Qwen3.5-4B",
        format: "gguf",
        backend: "llama.cpp",
        license: SourceClaim {
            claim: "Hugging Face model card license field is apache-2.0.",
            source: "https://huggingface.co/Qwen/Qwen3.5-4B",
            checked_at: "2026-06-29",
            status: "confirmed",
        },
        artifact_provider: None,
        artifact_url: None,
        artifact_terms_url: None,
        artifact_name: None,
        quantization: None,
        sha256: None,
        size_bytes: None,
        context_length: None,
        recommended_ram_gb: None,
        backend_compatibility: None,
        benchmark: BenchmarkClaim {
            source: "https://huggingface.co/Qwen/Qwen3.5-4B#benchmark-results",
            checked_at: "2026-06-29",
            claim_status: "source-listed-unreproduced",
            harness: "미확정: upstream model card의 공개 점수 조건을 local harness로 아직 고정하지 않음",
            dataset: "미확정: dataset version/license/source를 local benchmark fixture에 아직 고정하지 않음",
            prompt: "미확정: prompt/template과 sampling option을 아직 고정하지 않음",
            scoring: "미확정: local scorer와 published scorer parity를 아직 확인하지 않음",
            hardware_backend: "미확정: GGUF artifact, quantization, llama.cpp version, hardware 조건을 아직 고정하지 않음",
            reproducibility: "공개 점수는 upstream model card source로만 추적하며, GGUF artifact/backend/quantization 조건이 정해지기 전까지 local parity 미검증입니다.",
        },
        install_blockers: NO_EXTRA_BLOCKERS,
    },
    ModelManifestEntry {
        id: "gemma-4-e4b",
        display_name: "Gemma 4 E4B GGUF",
        status: CandidateStatus::Candidate,
        role: "비교 평가 후보",
        upstream_model: "google/gemma-4-E4B",
        upstream_url: "https://huggingface.co/google/gemma-4-E4B",
        format: "gguf",
        backend: "llama.cpp",
        license: SourceClaim {
            claim: "Hugging Face model card license field is apache-2.0 and Google AI for Developers publishes Gemma under Apache 2.0.",
            source: "https://huggingface.co/google/gemma-4-E4B, https://ai.google.dev/gemma/apache_2",
            checked_at: "2026-06-29",
            status: "confirmed",
        },
        artifact_provider: None,
        artifact_url: None,
        artifact_terms_url: None,
        artifact_name: None,
        quantization: None,
        sha256: None,
        size_bytes: None,
        context_length: None,
        recommended_ram_gb: None,
        backend_compatibility: None,
        benchmark: BenchmarkClaim {
            source: "https://huggingface.co/google/gemma-4-E4B#benchmark-results",
            checked_at: "2026-06-29",
            claim_status: "source-listed-unreproduced",
            harness: "미확정: upstream model card의 공개 점수 조건을 local harness로 아직 고정하지 않음",
            dataset: "미확정: dataset version/license/source를 local benchmark fixture에 아직 고정하지 않음",
            prompt: "미확정: prompt/template과 sampling option을 아직 고정하지 않음",
            scoring: "미확정: local scorer와 published scorer parity를 아직 확인하지 않음",
            hardware_backend: "미확정: GGUF artifact, quantization, llama.cpp version, hardware 조건을 아직 고정하지 않음",
            reproducibility: "공개 점수는 upstream model card source로만 추적하며, GGUF artifact/backend/quantization 조건이 정해지기 전까지 local parity 미검증입니다.",
        },
        install_blockers: NO_EXTRA_BLOCKERS,
    },
    ModelManifestEntry {
        id: "qwen3.5-9b",
        display_name: "Qwen3.5 9B GGUF",
        status: CandidateStatus::Candidate,
        role: "품질 참고 후보",
        upstream_model: "Qwen/Qwen3.5-9B",
        upstream_url: "https://huggingface.co/Qwen/Qwen3.5-9B",
        format: "gguf",
        backend: "llama.cpp",
        license: SourceClaim {
            claim: "Hugging Face model card license field is apache-2.0.",
            source: "https://huggingface.co/Qwen/Qwen3.5-9B",
            checked_at: "2026-06-29",
            status: "confirmed",
        },
        artifact_provider: None,
        artifact_url: None,
        artifact_terms_url: None,
        artifact_name: None,
        quantization: None,
        sha256: None,
        size_bytes: None,
        context_length: None,
        recommended_ram_gb: None,
        backend_compatibility: None,
        benchmark: BenchmarkClaim {
            source: "https://huggingface.co/Qwen/Qwen3.5-9B#benchmark-results",
            checked_at: "2026-06-29",
            claim_status: "source-listed-unreproduced",
            harness: "미확정: upstream model card의 공개 점수 조건을 local harness로 아직 고정하지 않음",
            dataset: "미확정: dataset version/license/source를 local benchmark fixture에 아직 고정하지 않음",
            prompt: "미확정: prompt/template과 sampling option을 아직 고정하지 않음",
            scoring: "미확정: local scorer와 published scorer parity를 아직 확인하지 않음",
            hardware_backend: "미확정: GGUF artifact, quantization, llama.cpp version, hardware 조건을 아직 고정하지 않음",
            reproducibility: "공개 점수는 upstream model card source로만 추적하며, 16 GB runtime fit과 local parity는 측정 전 미확정입니다.",
        },
        install_blockers: QWEN_9B_BLOCKERS,
    },
];

pub fn candidate_summary() -> String {
    let counts = ManifestCounts::from_candidates();
    format!(
        "{}개 후보, verified {}개, 설치 가능 {}개, artifact 검증 필요",
        counts.total,
        counts.verified,
        CANDIDATES
            .iter()
            .filter(|candidate| validate_install_ready(candidate).ready)
            .count()
    )
}

pub fn list_report() -> String {
    let counts = ManifestCounts::from_candidates();
    let registry = registry_summary();
    let mut output = format!(
        "모델 manifest\n- schema version: 1\n- 후보: {}개\n- candidate: {}개\n- unverified: {}개\n- verified: {}개\n- 설치 가능: {}개\n- local registry: {}\n\n",
        counts.total,
        counts.candidate,
        counts.unverified,
        counts.verified,
        CANDIDATES
            .iter()
            .filter(|candidate| validate_install_ready(candidate).ready)
            .count(),
        paths::model_registry_dir().display()
    );

    for candidate in CANDIDATES {
        let validation = validate_install_ready(candidate);
        let install_state = if validation.ready {
            "설치 가능"
        } else {
            "설치 차단"
        };

        output.push_str(&format!(
            "- {} ({})\n  상태: {} / {}\n  역할: {}\n  upstream model: {}\n  upstream source: {}\n  license claim: {} ({}, checked {})\n  artifact: {}\n  sha256: {}\n  public benchmark source: {} ({})\n  reproducibility: {}\n  parity conditions: harness={}, dataset={}, prompt={}, scoring={}, hardware/backend={}\n",
            candidate.id,
            candidate.display_name,
            candidate.status.label(),
            install_state,
            candidate.role,
            candidate.upstream_model,
            candidate.upstream_url,
            candidate.license.status,
            candidate.license.source,
            candidate.license.checked_at,
            candidate.artifact_url.unwrap_or("미확정"),
            candidate.sha256.unwrap_or("미확정"),
            candidate.benchmark.source,
            candidate.benchmark.claim_status,
            candidate.benchmark.reproducibility,
            candidate.benchmark.harness,
            candidate.benchmark.dataset,
            candidate.benchmark.prompt,
            candidate.benchmark.scoring,
            candidate.benchmark.hardware_backend
        ));
    }

    output.push('\n');
    output.push_str(&registry);
    output.push_str("\n\n설치 가능 상태가 되려면 GGUF URL, provider terms, SHA-256, file size, backend 호환성, RAM 근거가 source-backed manifest에 있어야 합니다.");
    output
}

pub fn manifest_report() -> String {
    format!(
        "model manifest schema\n- schemaVersion: 1\n- required status: {}\n- required source-backed fields: upstreamModel, upstreamUrl, license, licenseSource, licenseCheckedAt, artifactUrl, artifactProvider, artifactTermsUrl, sha256, sizeBytes, quantization, backendCompatibility, recommendedRamEvidence\n- benchmark ledger fields: publishedScoreSource, checkedAt, harness, dataset, scoring, backend, quantization, contextLength, localScore, parityStatus\n- install gate: verified status plus valid SHA-256 and non-empty artifact/source/license/backend fields\n- local registry: app data models/registry/<model-id>.json\n- 금지: checksum 없는 설치, license 미표기 설치, 출처 없는 RAM/backend/benchmark claim 확정",
        STATUS_SCHEMA
            .iter()
            .map(|status| status.label())
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

pub fn inspect_report(id: &str) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let validation = validate_install_ready(candidate);
    Ok(format!(
        "model inspect\n- id: {}\n- display name: {}\n- status: {}\n- install ready: {}\n- blockers: {}\n- upstream model: {}\n- upstream source: {}\n- license claim: {}\n- license source: {}\n- license checked-at: {}\n- artifact provider: {}\n- artifact URL: {}\n- artifact terms: {}\n- artifact name: {}\n- format: {}\n- backend: {}\n- quantization: {}\n- sha256: {}\n- size bytes: {}\n- context length: {}\n- recommended RAM GB: {}\n- backend compatibility: {}\n- public benchmark source: {}\n- benchmark checked-at: {}\n- benchmark claim status: {}\n- benchmark harness: {}\n- benchmark dataset: {}\n- benchmark prompt: {}\n- benchmark scoring: {}\n- benchmark hardware/backend: {}\n- reproducibility: {}",
        candidate.id,
        candidate.display_name,
        candidate.status.label(),
        if validation.ready { "yes" } else { "no" },
        display_vec(&validation.blockers),
        candidate.upstream_model,
        candidate.upstream_url,
        candidate.license.claim,
        candidate.license.source,
        candidate.license.checked_at,
        candidate.artifact_provider.unwrap_or("미확정"),
        candidate.artifact_url.unwrap_or("미확정"),
        candidate.artifact_terms_url.unwrap_or("미확정"),
        candidate.artifact_name.unwrap_or("미확정"),
        candidate.format,
        candidate.backend,
        candidate.quantization.unwrap_or("미확정"),
        candidate.sha256.unwrap_or("미확정"),
        candidate
            .size_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        candidate
            .context_length
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        candidate
            .recommended_ram_gb
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        candidate
            .backend_compatibility
            .map(|claim| format!("{} ({}, checked {})", claim.status, claim.source, claim.checked_at))
            .unwrap_or_else(|| "미확정".to_string()),
        candidate.benchmark.source,
        candidate.benchmark.checked_at,
        candidate.benchmark.claim_status,
        candidate.benchmark.harness,
        candidate.benchmark.dataset,
        candidate.benchmark.prompt,
        candidate.benchmark.scoring,
        candidate.benchmark.hardware_backend,
        candidate.benchmark.reproducibility
    ))
}

pub fn registry_report() -> String {
    registry_summary()
}

pub fn install_candidate(id: &str) -> Result<(), AppError> {
    let candidate = find_candidate(id)?;
    let validation = validate_install_ready(candidate);

    if !validation.ready {
        let event_id = state::record_event(
            "model.install.blocked",
            "미검증 model install 차단",
            &format!(
                "model_id={} status={} blockers={}",
                candidate.id,
                candidate.status.label(),
                validation.blockers.join(",")
            ),
        )?;
        return Err(AppError::blocked(format!(
            "설치를 차단했습니다: {}\n상태: {}\n이유:\n- {}\nsource: {}\nlicense source: {}\nbenchmark source: {}\nlocal registry: {}\nledger event: {}\n다음 단계: 검증된 GGUF artifact URL, provider terms, SHA-256, file size, backend 호환성, RAM 근거를 manifest에 추가해야 합니다.",
            candidate.id,
            candidate.status.label(),
            validation.blockers.join("\n- "),
            candidate.upstream_url,
            candidate.license.source,
            candidate.benchmark.source,
            paths::model_registry_dir().display(),
            event_id
        )));
    }

    persist_registry_entry(candidate)?;
    let event_id = state::record_event(
        "model.install.registered",
        "검증된 model registry 등록",
        &format!("model_id={}", candidate.id),
    )?;

    println!(
        "모델 registry 등록 완료\n- id: {}\n- registry: {}\n- ledger event: {}\n- 동작: 다운로드 실행 경로는 이어받기와 checksum 검증 phase에서 연결합니다.",
        candidate.id,
        registry_path(candidate.id).display(),
        event_id
    );
    Ok(())
}

struct ManifestCounts {
    total: usize,
    candidate: usize,
    unverified: usize,
    verified: usize,
}

impl ManifestCounts {
    fn from_candidates() -> Self {
        let mut counts = ManifestCounts {
            total: CANDIDATES.len(),
            candidate: 0,
            unverified: 0,
            verified: 0,
        };

        for candidate in CANDIDATES {
            match candidate.status {
                CandidateStatus::Candidate => counts.candidate += 1,
                CandidateStatus::Unverified => counts.unverified += 1,
                CandidateStatus::Verified => counts.verified += 1,
            }
        }

        counts
    }
}

impl CandidateStatus {
    fn label(self) -> &'static str {
        match self {
            CandidateStatus::Candidate => "candidate",
            CandidateStatus::Unverified => "unverified",
            CandidateStatus::Verified => "verified",
        }
    }
}

fn find_candidate(id: &str) -> Result<&'static ModelManifestEntry, AppError> {
    CANDIDATES
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| {
            AppError::usage(format!(
                "알 수 없는 모델 id입니다: {id}\n사용 가능 후보는 `rpotato model list`로 확인하세요."
            ))
        })
}

fn validate_install_ready(candidate: &ModelManifestEntry) -> InstallValidation {
    let mut blockers = Vec::new();

    if candidate.status != CandidateStatus::Verified {
        push_unique(
            &mut blockers,
            "manifest status가 verified가 아니므로 설치할 수 없습니다.",
        );
    }

    for blocker in candidate.install_blockers {
        push_unique(&mut blockers, blocker);
    }

    if candidate.artifact_provider.is_none() {
        push_unique(&mut blockers, "artifact provider 미확정");
    }
    if candidate.artifact_url.is_none() {
        push_unique(&mut blockers, "GGUF artifact URL 미확정");
    }
    if candidate.artifact_terms_url.is_none() {
        push_unique(&mut blockers, "artifact terms URL 미확정");
    }
    if candidate.artifact_name.is_none() {
        push_unique(&mut blockers, "artifact file name 미확정");
    }
    if candidate.quantization.is_none() {
        push_unique(&mut blockers, "quantization 미확정");
    }
    if candidate.size_bytes.is_none() {
        push_unique(&mut blockers, "file size 미확정");
    }
    if candidate.backend_compatibility.is_none() {
        push_unique(&mut blockers, "backend compatibility source 미확정");
    }
    if candidate.recommended_ram_gb.is_none() {
        push_unique(&mut blockers, "recommended RAM source 미확정");
    }

    match candidate.sha256 {
        Some(hash) if is_valid_sha256(hash) => {}
        Some(_) => push_unique(&mut blockers, "SHA-256 형식 오류"),
        None => push_unique(&mut blockers, "SHA-256 미확정"),
    }

    InstallValidation {
        ready: blockers.is_empty(),
        blockers,
    }
}

fn persist_registry_entry(candidate: &ModelManifestEntry) -> Result<(), AppError> {
    fs::create_dir_all(paths::model_registry_dir()).map_err(|err| {
        AppError::runtime(format!(
            "model registry directory를 만들지 못했습니다: {} ({err})",
            paths::model_registry_dir().display()
        ))
    })?;

    fs::write(registry_path(candidate.id), registry_entry_json(candidate)).map_err(|err| {
        AppError::runtime(format!(
            "model registry entry를 기록하지 못했습니다: {} ({err})",
            registry_path(candidate.id).display()
        ))
    })
}

fn registry_summary() -> String {
    match read_registry_entries() {
        Ok(entries) if entries.is_empty() => format!(
            "model registry\n- installed models: 0\n- registry dir: {}",
            paths::model_registry_dir().display()
        ),
        Ok(entries) => {
            let rows = entries
                .iter()
                .map(|entry| {
                    format!(
                        "- {} | status: {} | sha256: {} | path: {}",
                        entry.id, entry.status, entry.artifact_sha256, entry.artifact_path
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "model registry\n- installed models: {}\n- registry dir: {}\n{}",
                entries.len(),
                paths::model_registry_dir().display(),
                rows
            )
        }
        Err(err) => format!(
            "model registry\n- 상태: registry 읽기 실패\n- 이유: {}\n- registry dir: {}",
            err.message,
            paths::model_registry_dir().display()
        ),
    }
}

fn read_registry_entries() -> Result<Vec<RegistryEntry>, AppError> {
    let dir = paths::model_registry_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|err| {
        AppError::runtime(format!(
            "model registry directory를 읽지 못했습니다: {} ({err})",
            dir.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "model registry entry를 읽지 못했습니다: {} ({err})",
                dir.display()
            ))
        })?;

        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }

        let text = fs::read_to_string(entry.path()).map_err(|err| {
            AppError::runtime(format!(
                "model registry entry를 읽지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;

        if let Some(registry_entry) = parse_registry_entry(&text) {
            entries.push(registry_entry);
        }
    }

    entries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(entries)
}

fn parse_registry_entry(text: &str) -> Option<RegistryEntry> {
    Some(RegistryEntry {
        id: extract_json_string(text, "id")?,
        display_name: extract_json_string(text, "displayName")?,
        status: extract_json_string(text, "status")?,
        artifact_path: extract_json_string(text, "artifactPath")?,
        artifact_sha256: extract_json_string(text, "artifactSha256")?,
    })
}

fn registry_path(id: &str) -> PathBuf {
    paths::model_registry_dir().join(format!("{id}.json"))
}

fn registry_entry_json(candidate: &ModelManifestEntry) -> String {
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"status\": \"installed\",\n  \"upstreamModel\": \"{}\",\n  \"upstreamUrl\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"licenseSource\": \"{}\",\n  \"licenseCheckedAt\": \"{}\"\n}}\n",
        ledger::json_string(candidate.id),
        ledger::json_string(candidate.display_name),
        ledger::json_string(candidate.upstream_model),
        ledger::json_string(candidate.upstream_url),
        ledger::json_string(
            &paths::models_dir()
                .join(candidate.artifact_name.unwrap_or(candidate.id))
                .display()
                .to_string()
        ),
        ledger::json_string(candidate.sha256.unwrap_or("")),
        ledger::json_string(candidate.license.source),
        ledger::json_string(candidate.license.checked_at)
    )
}

fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

fn extract_json_string(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let mut value = String::new();
    let mut escaped = false;

    for ch in line[start..].chars() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_summary_reports_verified_count() {
        let summary = candidate_summary();
        assert!(summary.contains("3개 후보"));
        assert!(summary.contains("verified 0개"));
    }

    #[test]
    fn manifest_validation_blocks_candidate_without_artifact() {
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let validation = validate_install_ready(candidate);

        assert!(!validation.ready);
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("verified")));
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("SHA-256")));
    }

    #[test]
    fn sha256_validation_requires_64_hex_chars() {
        assert!(is_valid_sha256(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!is_valid_sha256("not-a-sha"));
        assert!(!is_valid_sha256(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        ));
    }

    #[test]
    fn manifest_report_names_required_source_backed_fields() {
        let report = manifest_report();
        assert!(report.contains("artifactUrl"));
        assert!(report.contains("sha256"));
        assert!(report.contains("benchmark ledger"));
    }
}
