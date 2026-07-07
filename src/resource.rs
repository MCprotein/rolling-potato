use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const KIB_BYTES: u64 = 1024;
const GIB_BYTES: u64 = 1024 * 1024 * 1024;
const DEGRADED_CPU_PERCENT: f64 = 80.0;
const CRITICAL_CPU_PERCENT: f64 = 95.0;
const DEGRADED_RSS_BYTES: u64 = 8 * GIB_BYTES;
const CRITICAL_RSS_BYTES: u64 = 12 * GIB_BYTES;
pub const DEGRADED_CHAT_MAX_TOKENS: u32 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourcePressure {
    Unknown,
    Normal,
    Degraded,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceGovernorAdmission {
    Allow,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceGovernorTokenAction {
    Unchanged,
    Clamped,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceGovernorDecision {
    pub pressure: ResourcePressure,
    pub requested_max_tokens: u32,
    pub effective_max_tokens: Option<u32>,
    pub admission: ResourceGovernorAdmission,
    pub token_action: ResourceGovernorTokenAction,
    pub reason: &'static str,
    pub hint: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessResourceSnapshot {
    pub pid: u32,
    pub process_cpu_percent: Option<f64>,
    pub average_rss_bytes: Option<u64>,
    pub peak_rss_bytes: Option<u64>,
    pub disk_bytes: Option<u64>,
    pub sample_count: u32,
    pub pressure: ResourcePressure,
}

impl ResourcePressure {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourcePressure::Unknown => "unknown",
            ResourcePressure::Normal => "normal",
            ResourcePressure::Degraded => "degraded",
            ResourcePressure::Critical => "critical",
        }
    }
}

impl ResourceGovernorAdmission {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceGovernorAdmission::Allow => "allow",
            ResourceGovernorAdmission::Block => "block",
        }
    }
}

impl ResourceGovernorTokenAction {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceGovernorTokenAction::Unchanged => "unchanged",
            ResourceGovernorTokenAction::Clamped => "clamped",
            ResourceGovernorTokenAction::Blocked => "blocked",
        }
    }
}

impl ResourceGovernorDecision {
    pub fn is_blocked(&self) -> bool {
        self.admission == ResourceGovernorAdmission::Block
    }
}

pub fn sample_process(pid: u32, disk_paths: &[PathBuf]) -> ProcessResourceSnapshot {
    let (process_cpu_percent, ps_rss_bytes) = process_cpu_and_rss(pid);
    let (proc_rss_bytes, proc_peak_rss_bytes) = linux_status_rss(pid);
    let average_rss_bytes = proc_rss_bytes.or(ps_rss_bytes);
    let peak_rss_bytes = proc_peak_rss_bytes.or(average_rss_bytes);
    let disk_bytes = disk_bytes(disk_paths);
    let pressure = classify_pressure(process_cpu_percent, average_rss_bytes, peak_rss_bytes);

    ProcessResourceSnapshot {
        pid,
        process_cpu_percent,
        average_rss_bytes,
        peak_rss_bytes,
        disk_bytes,
        sample_count: 1,
        pressure,
    }
}

pub fn classify_pressure(
    process_cpu_percent: Option<f64>,
    average_rss_bytes: Option<u64>,
    peak_rss_bytes: Option<u64>,
) -> ResourcePressure {
    let cpu = process_cpu_percent.filter(|value| value.is_finite());
    let memory = peak_rss_bytes.or(average_rss_bytes);

    if cpu.is_none() && memory.is_none() {
        return ResourcePressure::Unknown;
    }
    if cpu.is_some_and(|value| value >= CRITICAL_CPU_PERCENT)
        || memory.is_some_and(|value| value >= CRITICAL_RSS_BYTES)
    {
        return ResourcePressure::Critical;
    }
    if cpu.is_some_and(|value| value >= DEGRADED_CPU_PERCENT)
        || memory.is_some_and(|value| value >= DEGRADED_RSS_BYTES)
    {
        return ResourcePressure::Degraded;
    }
    ResourcePressure::Normal
}

pub fn chat_governor_decision(
    pressure: ResourcePressure,
    requested_max_tokens: u32,
) -> ResourceGovernorDecision {
    match pressure {
        ResourcePressure::Critical => ResourceGovernorDecision {
            pressure,
            requested_max_tokens,
            effective_max_tokens: None,
            admission: ResourceGovernorAdmission::Block,
            token_action: ResourceGovernorTokenAction::Blocked,
            reason: "critical resource pressure",
            hint: "run backend status, stop the sidecar, or lower host load before retrying",
        },
        ResourcePressure::Degraded => {
            let effective_max_tokens = requested_max_tokens.min(DEGRADED_CHAT_MAX_TOKENS);
            ResourceGovernorDecision {
                pressure,
                requested_max_tokens,
                effective_max_tokens: Some(effective_max_tokens),
                admission: ResourceGovernorAdmission::Allow,
                token_action: if effective_max_tokens < requested_max_tokens {
                    ResourceGovernorTokenAction::Clamped
                } else {
                    ResourceGovernorTokenAction::Unchanged
                },
                reason: "degraded resource pressure",
                hint: "use a smaller --max-tokens value or restart with a smaller --ctx-size if pressure persists",
            }
        }
        ResourcePressure::Unknown => ResourceGovernorDecision {
            pressure,
            requested_max_tokens,
            effective_max_tokens: Some(requested_max_tokens),
            admission: ResourceGovernorAdmission::Allow,
            token_action: ResourceGovernorTokenAction::Unchanged,
            reason: "resource pressure unknown",
            hint: "resource sample is incomplete, so the requested token limit is preserved",
        },
        ResourcePressure::Normal => ResourceGovernorDecision {
            pressure,
            requested_max_tokens,
            effective_max_tokens: Some(requested_max_tokens),
            admission: ResourceGovernorAdmission::Allow,
            token_action: ResourceGovernorTokenAction::Unchanged,
            reason: "resource pressure normal",
            hint: "no runtime clamp applied",
        },
    }
}

#[cfg(unix)]
fn process_cpu_and_rss(pid: u32) -> (Option<f64>, Option<u64>) {
    let Ok(output) = Command::new("ps")
        .arg("-o")
        .arg("%cpu=")
        .arg("-o")
        .arg("rss=")
        .arg("-p")
        .arg(pid.to_string())
        .output()
    else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }

    parse_ps_cpu_rss(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(windows)]
fn process_cpu_and_rss(pid: u32) -> (Option<f64>, Option<u64>) {
    let query = format!("ProcessId={pid}");
    let Ok(output) = Command::new("wmic")
        .args([
            "process",
            "where",
            query.as_str(),
            "get",
            "WorkingSetSize,PeakWorkingSetSize",
            "/format:list",
        ])
        .output()
    else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }

    let mut working_set = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("WorkingSetSize") {
            working_set = value.trim().parse::<u64>().ok();
        }
    }
    (None, working_set)
}

#[cfg(not(any(unix, windows)))]
fn process_cpu_and_rss(_pid: u32) -> (Option<f64>, Option<u64>) {
    (None, None)
}

fn parse_ps_cpu_rss(contents: &str) -> (Option<f64>, Option<u64>) {
    let Some(line) = contents
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    else {
        return (None, None);
    };
    let mut parts = line.split_whitespace();
    let cpu = parts.next().and_then(|value| value.parse::<f64>().ok());
    let rss_bytes = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|rss_kib| rss_kib.saturating_mul(KIB_BYTES));
    (cpu, rss_bytes)
}

#[cfg(target_os = "linux")]
fn linux_status_rss(pid: u32) -> (Option<u64>, Option<u64>) {
    fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .map(|contents| parse_linux_status_rss(&contents))
        .unwrap_or((None, None))
}

#[cfg(not(target_os = "linux"))]
fn linux_status_rss(_pid: u32) -> (Option<u64>, Option<u64>) {
    (None, None)
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_status_rss(contents: &str) -> (Option<u64>, Option<u64>) {
    let mut rss = None;
    let mut high_water = None;
    for line in contents.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = parse_status_kib(value);
        match key.trim() {
            "VmRSS" => rss = value,
            "VmHWM" => high_water = value,
            _ => {}
        }
    }
    (rss, high_water)
}

#[cfg(any(target_os = "linux", test))]
fn parse_status_kib(value: &str) -> Option<u64> {
    value
        .split_whitespace()
        .next()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(KIB_BYTES))
}

fn disk_bytes(paths: &[PathBuf]) -> Option<u64> {
    let mut total = 0_u64;
    let mut saw_path = false;
    for path in paths {
        if let Some(bytes) = path_disk_bytes(path) {
            saw_path = true;
            total = total.saturating_add(bytes);
        }
    }
    saw_path.then_some(total)
}

fn path_disk_bytes(path: &Path) -> Option<u64> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.is_file() || metadata.file_type().is_symlink() {
        return Some(metadata.len());
    }
    if !metadata.is_dir() {
        return Some(0);
    }

    let mut total = 0_u64;
    for entry in fs::read_dir(path).ok()? {
        let Ok(entry) = entry else {
            continue;
        };
        if let Some(bytes) = path_disk_bytes(&entry.path()) {
            total = total.saturating_add(bytes);
        }
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_pressure_handles_unknown_normal_and_thresholds() {
        assert_eq!(
            classify_pressure(None, None, None),
            ResourcePressure::Unknown
        );
        assert_eq!(
            classify_pressure(Some(12.5), Some(512 * 1024 * 1024), None),
            ResourcePressure::Normal
        );
        assert_eq!(
            classify_pressure(Some(80.0), Some(512 * 1024 * 1024), None),
            ResourcePressure::Degraded
        );
        assert_eq!(
            classify_pressure(Some(20.0), None, Some(12 * GIB_BYTES)),
            ResourcePressure::Critical
        );
    }

    #[test]
    fn parses_ps_cpu_and_rss_output() {
        let (cpu, rss) = parse_ps_cpu_rss(" 12.7  4096\n");

        assert_eq!(cpu, Some(12.7));
        assert_eq!(rss, Some(4 * 1024 * 1024));
    }

    #[test]
    fn parses_linux_status_rss_fields() {
        let (rss, peak) = parse_linux_status_rss(
            "Name:\tllama-server\nVmHWM:\t  1048576 kB\nVmRSS:\t   524288 kB\n",
        );

        assert_eq!(rss, Some(512 * 1024 * 1024));
        assert_eq!(peak, Some(1024 * 1024 * 1024));
    }

    #[test]
    fn chat_governor_allows_clamps_and_blocks_by_pressure() {
        let normal = chat_governor_decision(ResourcePressure::Normal, 512);
        assert_eq!(normal.admission, ResourceGovernorAdmission::Allow);
        assert_eq!(normal.token_action, ResourceGovernorTokenAction::Unchanged);
        assert_eq!(normal.effective_max_tokens, Some(512));

        let degraded = chat_governor_decision(ResourcePressure::Degraded, 512);
        assert_eq!(degraded.admission, ResourceGovernorAdmission::Allow);
        assert_eq!(degraded.token_action, ResourceGovernorTokenAction::Clamped);
        assert_eq!(
            degraded.effective_max_tokens,
            Some(DEGRADED_CHAT_MAX_TOKENS)
        );

        let small_degraded = chat_governor_decision(ResourcePressure::Degraded, 64);
        assert_eq!(
            small_degraded.token_action,
            ResourceGovernorTokenAction::Unchanged
        );
        assert_eq!(small_degraded.effective_max_tokens, Some(64));

        let critical = chat_governor_decision(ResourcePressure::Critical, 512);
        assert!(critical.is_blocked());
        assert_eq!(critical.token_action, ResourceGovernorTokenAction::Blocked);
        assert_eq!(critical.effective_max_tokens, None);
    }
}
