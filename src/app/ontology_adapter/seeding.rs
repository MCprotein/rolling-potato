use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;

use super::*;

const MAX_SEEDED_FILES: usize = 256;
const MAX_INDEXED_FILE_BYTES: u64 = 1024 * 1024;

pub(super) fn ensure_layout() -> Result<(), AppError> {
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

pub(super) fn seed_candidates() -> Result<Vec<OntologyRecord>, AppError> {
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

pub(super) fn append_records(records: &[OntologyRecord]) -> Result<(), AppError> {
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
