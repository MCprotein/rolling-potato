pub(crate) fn strip_terminal_controls(capture: &str) -> String {
    let mut output = String::with_capacity(capture.len());
    let mut characters = capture.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\u{001b}' {
            if !character.is_control() || matches!(character, '\n' | '\r' | '\t') {
                output.push(character);
            }
            continue;
        }
        match characters.next() {
            Some('[') => {
                for control in characters.by_ref() {
                    if ('@'..='~').contains(&control) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(control) = characters.next() {
                    if control == '\u{0007}' {
                        break;
                    }
                    if control == '\u{001b}' && characters.next_if_eq(&'\\').is_some() {
                        break;
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    output
}

pub(crate) fn mode_probe_values(output: &[u8]) -> Vec<String> {
    let capture = strip_terminal_controls(&String::from_utf8_lossy(output));
    capture
        .lines()
        .filter_map(|line| line.split_once("MODE ECHO=").map(|(_, value)| value))
        .filter_map(|value| match value.trim_start().chars().next() {
            Some('0') => Some("0".to_string()),
            Some('1') => Some("1".to_string()),
            _ => None,
        })
        .collect()
}

pub fn tree_snapshot(roots: &[&std::path::Path]) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut snapshot = std::collections::BTreeMap::new();
    for (index, root) in roots.iter().enumerate() {
        let mut files = Vec::new();
        collect_files(root, root, &mut files);
        for (path, bytes) in files {
            snapshot.insert(format!("{index}/{path}"), bytes);
        }
    }
    snapshot
}

fn collect_files(
    root: &std::path::Path,
    path: &std::path::Path,
    files: &mut Vec<(String, Vec<u8>)>,
) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            files.push((relative, std::fs::read(&path).unwrap_or_default()));
        }
    }
}
