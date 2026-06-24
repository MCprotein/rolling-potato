use crate::app::AppError;

#[derive(Debug)]
struct ModelCandidate {
    id: &'static str,
    display_name: &'static str,
    role: &'static str,
    upstream_license: &'static str,
    upstream_source: &'static str,
    downloadable: bool,
    blocked_reason: &'static str,
}

const CANDIDATES: &[ModelCandidate] = &[
    ModelCandidate {
        id: "qwen3.5-4b",
        display_name: "Qwen3.5 4B GGUF",
        role: "우선 평가 후보",
        upstream_license: "apache-2.0",
        upstream_source: "https://huggingface.co/Qwen/Qwen3.5-4B",
        downloadable: false,
        blocked_reason: "GGUF artifact URL, provider terms, SHA-256, file size, llama.cpp 호환성 검증이 아직 없습니다.",
    },
    ModelCandidate {
        id: "gemma-4-e4b",
        display_name: "Gemma 4 E4B GGUF",
        role: "비교 평가 후보",
        upstream_license: "apache-2.0",
        upstream_source: "https://huggingface.co/google/gemma-4-E4B",
        downloadable: false,
        blocked_reason: "GGUF artifact URL, provider terms, SHA-256, file size, llama.cpp 호환성 검증이 아직 없습니다.",
    },
    ModelCandidate {
        id: "qwen3.5-9b",
        display_name: "Qwen3.5 9B GGUF",
        role: "품질 참고 후보",
        upstream_license: "apache-2.0",
        upstream_source: "https://huggingface.co/Qwen/Qwen3.5-9B",
        downloadable: false,
        blocked_reason: "제품 기본값 보류 상태이며, 16 GB runtime fit과 GGUF artifact 검증이 아직 없습니다.",
    },
];

pub fn candidate_summary() -> String {
    format!(
        "{}개 후보, 다운로드 가능 0개, artifact 검증 필요",
        CANDIDATES.len()
    )
}

pub fn list_report() -> String {
    let mut output = String::from("모델 후보\n");

    for candidate in CANDIDATES {
        let status = if candidate.downloadable {
            "설치 가능"
        } else {
            "설치 차단"
        };

        output.push_str(&format!(
            "- {} ({})\n  상태: {}\n  역할: {}\n  upstream license: {}\n  source: {}\n",
            candidate.id,
            candidate.display_name,
            status,
            candidate.role,
            candidate.upstream_license,
            candidate.upstream_source
        ));
    }

    output.push_str("설치 가능 상태가 되려면 GGUF URL, checksum, provider terms, file size, backend 호환성 검증이 필요합니다.");
    output
}

pub fn install_candidate(id: &str) -> Result<(), AppError> {
    let Some(candidate) = CANDIDATES.iter().find(|candidate| candidate.id == id) else {
        return Err(AppError::usage(format!(
            "알 수 없는 모델 id입니다: {id}\n사용 가능 후보는 `rpotato model list`로 확인하세요."
        )));
    };

    if !candidate.downloadable {
        return Err(AppError::blocked(format!(
            "설치를 차단했습니다: {}\n이유: {}\nsource: {}\n다음 단계: 검증된 GGUF artifact와 checksum을 manifest에 추가해야 합니다.",
            candidate.id, candidate.blocked_reason, candidate.upstream_source
        )));
    }

    println!("모델 설치를 시작합니다: {}", candidate.id);
    Ok(())
}
