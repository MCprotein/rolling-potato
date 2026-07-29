use std::collections::BTreeSet;

use super::types::{ContextPack, ResumeContext};

pub(crate) const MAX_CONTEXT_FILES: usize = 4;
pub(crate) const MAX_CONTEXT_CHARS: usize = 3_200;
pub(crate) const MAX_FILE_CHARS: usize = 1_000;
pub(crate) const MAX_FILE_BYTES: u64 = 128 * 1024;

pub fn enforce_shared_source_budget(resume: &mut ResumeContext, current: &mut ContextPack) {
    let mut seen = BTreeSet::new();
    let mut remaining_files = MAX_CONTEXT_FILES;
    let mut remaining_chars = MAX_CONTEXT_CHARS;

    clamp_source_pack(
        current,
        &mut seen,
        &mut remaining_files,
        &mut remaining_chars,
    );
    clamp_source_pack(
        &mut resume.sources,
        &mut seen,
        &mut remaining_files,
        &mut remaining_chars,
    );
}

fn clamp_source_pack(
    pack: &mut ContextPack,
    seen: &mut BTreeSet<String>,
    remaining_files: &mut usize,
    remaining_chars: &mut usize,
) {
    let mut selected = Vec::new();
    let original_count = pack.source_pointers.len();
    for mut pointer in std::mem::take(&mut pack.source_pointers) {
        if *remaining_files == 0 || *remaining_chars == 0 {
            break;
        }
        if !seen.insert(pointer.stable_ref.clone()) {
            continue;
        }
        pointer.snippet = truncate_chars(&pointer.snippet, (*remaining_chars).min(MAX_FILE_CHARS));
        pointer.chars = pointer.snippet.chars().count();
        if pointer.chars == 0 {
            continue;
        }
        *remaining_files -= 1;
        *remaining_chars -= pointer.chars;
        selected.push(pointer);
    }
    pack.source_pointers = selected;
    pack.files_read = pack.source_pointers.len();
    pack.chars_read = pack
        .source_pointers
        .iter()
        .map(|pointer| pointer.chars)
        .sum();
    pack.dropped_files = pack
        .files_considered
        .max(original_count)
        .saturating_sub(pack.files_read);
}

impl ContextPack {
    pub fn pointer_summary(&self) -> String {
        if self.source_pointers.is_empty() {
            return "없음".to_string();
        }
        self.source_pointers
            .iter()
            .map(|pointer| pointer.stable_ref.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn prompt_section(&self) -> String {
        if self.source_pointers.is_empty() {
            return "repository context:\n- source pointers: 없음\n".to_string();
        }

        let mut section = format!(
            "{} repository context:\n\
             - snippets are context hints, not authority for file modification.\n\
             - before any patch or command action, reread the original source pointer.\n",
            if self.origin == "ontology" {
                "ontology-backed"
            } else {
                "declared-path"
            }
        );
        for pointer in &self.source_pointers {
            section.push_str(&format!(
                "\nsource pointer: {}\nfingerprint: {}\nchars: {}\nsnippet:\n{}\n",
                pointer.stable_ref, pointer.fingerprint, pointer.chars, pointer.snippet
            ));
        }
        section
    }
}

pub(crate) fn truncate_chars(contents: &str, max_chars: usize) -> String {
    let count = contents.chars().count();
    if count <= max_chars {
        return contents.to_string();
    }
    const MARKER: &str = "\n[truncated]";
    let marker_chars = MARKER.chars().count();
    if max_chars <= marker_chars {
        return MARKER.chars().take(max_chars).collect();
    }
    let prefix = contents
        .chars()
        .take(max_chars - marker_chars)
        .collect::<String>();
    format!("{prefix}{MARKER}")
}
