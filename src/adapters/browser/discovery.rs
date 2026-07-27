use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::foundation::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserKind {
    Chrome,
    Chromium,
    Edge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserExecutable {
    pub(crate) kind: BrowserKind,
    pub(crate) path: PathBuf,
}

pub(crate) fn discover_installed_browser() -> Result<BrowserExecutable, AppError> {
    let platform = Platform::current();
    let home = env::var_os(home_variable(platform)).map(PathBuf::from);
    let path_entries = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let local_app_data = env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let program_files = env::var_os("PROGRAMFILES").map(PathBuf::from);
    let program_files_x86 = env::var_os("PROGRAMFILES(X86)").map(PathBuf::from);
    let variables = PlatformVariables {
        home: home.as_deref(),
        local_app_data: local_app_data.as_deref(),
        program_files: program_files.as_deref(),
        program_files_x86: program_files_x86.as_deref(),
        path_entries: &path_entries,
    };

    discover_from_candidates(platform_candidates(platform, &variables))
}

fn discover_from_candidates(
    candidates: impl IntoIterator<Item = BrowserExecutable>,
) -> Result<BrowserExecutable, AppError> {
    candidates
        .into_iter()
        .find_map(|candidate| {
            executable_file(&candidate.path).then(|| BrowserExecutable {
                kind: candidate.kind,
                path: fs::canonicalize(&candidate.path).unwrap_or(candidate.path),
            })
        })
        .ok_or_else(|| {
            AppError::runtime(
                "설치된 Chrome, Chromium 또는 Edge를 찾지 못했습니다. 정적 WebSearch·WebOpen·WebFind는 계속 사용할 수 있습니다.",
            )
        })
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    MacOs,
    Linux,
    Windows,
    Unsupported,
}

impl Platform {
    const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unsupported
        }
    }
}

struct PlatformVariables<'a> {
    home: Option<&'a Path>,
    local_app_data: Option<&'a Path>,
    program_files: Option<&'a Path>,
    program_files_x86: Option<&'a Path>,
    path_entries: &'a [PathBuf],
}

fn home_variable(platform: Platform) -> &'static str {
    if platform == Platform::Windows {
        "USERPROFILE"
    } else {
        "HOME"
    }
}

fn platform_candidates(
    platform: Platform,
    variables: &PlatformVariables<'_>,
) -> Vec<BrowserExecutable> {
    let mut candidates = match platform {
        Platform::MacOs => macos_candidates(variables.home),
        Platform::Linux => Vec::new(),
        Platform::Windows => windows_candidates(variables),
        Platform::Unsupported => Vec::new(),
    };
    append_path_candidates(&mut candidates, platform, variables.path_entries);
    deduplicate_candidates(candidates)
}

fn macos_candidates(home: Option<&Path>) -> Vec<BrowserExecutable> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = home {
        roots.push(home.join("Applications"));
    }
    roots
        .into_iter()
        .flat_map(|root| {
            [
                candidate(
                    BrowserKind::Chrome,
                    root.join("Google Chrome.app/Contents/MacOS/Google Chrome"),
                ),
                candidate(
                    BrowserKind::Chromium,
                    root.join("Chromium.app/Contents/MacOS/Chromium"),
                ),
                candidate(
                    BrowserKind::Edge,
                    root.join("Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
                ),
            ]
        })
        .collect()
}

fn windows_candidates(variables: &PlatformVariables<'_>) -> Vec<BrowserExecutable> {
    let roots = [
        variables.local_app_data,
        variables.program_files,
        variables.program_files_x86,
    ];
    roots
        .into_iter()
        .flatten()
        .flat_map(|root| {
            [
                candidate(
                    BrowserKind::Chrome,
                    root.join("Google/Chrome/Application/chrome.exe"),
                ),
                candidate(
                    BrowserKind::Chromium,
                    root.join("Chromium/Application/chrome.exe"),
                ),
                candidate(
                    BrowserKind::Edge,
                    root.join("Microsoft/Edge/Application/msedge.exe"),
                ),
            ]
        })
        .collect()
}

fn append_path_candidates(
    output: &mut Vec<BrowserExecutable>,
    platform: Platform,
    path_entries: &[PathBuf],
) {
    let names: &[(BrowserKind, &str)] = match platform {
        Platform::Windows => &[
            (BrowserKind::Chrome, "chrome.exe"),
            (BrowserKind::Chromium, "chromium.exe"),
            (BrowserKind::Edge, "msedge.exe"),
        ],
        _ => &[
            (BrowserKind::Chrome, "google-chrome"),
            (BrowserKind::Chrome, "google-chrome-stable"),
            (BrowserKind::Chromium, "chromium"),
            (BrowserKind::Chromium, "chromium-browser"),
            (BrowserKind::Edge, "microsoft-edge"),
            (BrowserKind::Edge, "microsoft-edge-stable"),
        ],
    };
    for directory in path_entries {
        for (kind, name) in names {
            output.push(candidate(*kind, directory.join(name)));
        }
    }
}

fn candidate(kind: BrowserKind, path: PathBuf) -> BrowserExecutable {
    BrowserExecutable { kind, path }
}

fn deduplicate_candidates(candidates: Vec<BrowserExecutable>) -> Vec<BrowserExecutable> {
    let mut seen = Vec::<OsString>::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            let key = candidate.path.as_os_str().to_os_string();
            if seen.contains(&key) {
                false
            } else {
                seen.push(key);
                true
            }
        })
        .collect()
}

#[cfg(test)]
pub(super) fn test_platform_candidates(
    platform: &str,
    home: Option<&Path>,
    local_app_data: Option<&Path>,
    program_files: Option<&Path>,
    path_entries: &[PathBuf],
) -> Vec<BrowserExecutable> {
    let platform = match platform {
        "macos" => Platform::MacOs,
        "linux" => Platform::Linux,
        "windows" => Platform::Windows,
        _ => Platform::Unsupported,
    };
    platform_candidates(
        platform,
        &PlatformVariables {
            home,
            local_app_data,
            program_files,
            program_files_x86: None,
            path_entries,
        },
    )
}
