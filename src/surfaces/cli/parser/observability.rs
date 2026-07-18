use super::*;

pub(super) fn parse_monitor_export(args: &[String]) -> Result<MonitorCommand, AppError> {
    match args {
        [flag, format] if flag == "--format" => {
            let format = match format.as_str() {
                "jsonl" => MonitorExportFormat::Jsonl,
                "csv" => MonitorExportFormat::Csv,
                "html" => MonitorExportFormat::Html,
                _ => {
                    return Err(AppError::usage(
                        "monitor export format은 jsonl, csv 또는 html만 허용합니다.",
                    ));
                }
            };
            Ok(MonitorCommand::Export { format })
        }
        _ => Err(AppError::usage(
            "monitor export에는 --format jsonl, --format csv 또는 --format html이 필요합니다.",
        )),
    }
}

pub(super) fn parse_ontology_context(args: &[String]) -> Result<OntologyCommand, AppError> {
    match args {
        [flag, rest @ ..] if flag == "--query" => {
            if rest.is_empty() {
                return Err(AppError::usage(
                    "ontology context에는 --query <text> 값이 필요합니다.",
                ));
            }
            Ok(OntologyCommand::Context {
                query: rest.join(" "),
            })
        }
        _ => Err(AppError::usage(
            "ontology context는 --query <text> 형식이 필요합니다.",
        )),
    }
}

pub(super) fn parse_ontology_export(args: &[String]) -> Result<OntologyCommand, AppError> {
    match args {
        [flag, format] if flag == "--format" => {
            let format = match format.as_str() {
                "json" => OntologyExportFormat::Json,
                "jsonl" => OntologyExportFormat::Jsonl,
                _ => {
                    return Err(AppError::usage(
                        "ontology export format은 json 또는 jsonl만 허용합니다.",
                    ));
                }
            };
            Ok(OntologyCommand::Export { format })
        }
        _ => Err(AppError::usage(
            "ontology export에는 --format json 또는 --format jsonl 형식이 필요합니다.",
        )),
    }
}

pub(super) fn parse_ontology_import(args: &[String]) -> Result<OntologyCommand, AppError> {
    let mut path = None;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--file" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "ontology import --file에는 path가 필요합니다.",
                    ));
                };
                if path.is_some() {
                    return Err(AppError::usage(
                        "ontology import --file은 한 번만 지정할 수 있습니다.",
                    ));
                }
                path = Some(value.clone());
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 ontology import 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let Some(path) = path else {
        return Err(AppError::usage(
            "ontology import에는 --file <path>가 필요합니다.",
        ));
    };
    if !dry_run {
        return Err(AppError::usage(
            "ontology import는 현재 --dry-run을 명시해야 합니다.",
        ));
    }

    Ok(OntologyCommand::Import { path, dry_run })
}

pub(super) fn parse_benchmark_record(args: &[String]) -> Result<BenchmarkCommand, AppError> {
    match args {
        [flag, fixture] if flag == "--fixture" => Ok(BenchmarkCommand::Record {
            fixture: fixture.clone(),
        }),
        _ => Err(AppError::usage(
            "benchmark record에는 --fixture <fixture.json> 형식이 필요합니다.",
        )),
    }
}

pub(super) fn parse_benchmark_run(args: &[String]) -> Result<BenchmarkCommand, AppError> {
    let mut fixture = None;
    let mut prompt = None;
    let mut max_tokens = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--fixture" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "benchmark run --fixture에는 fixture path가 필요합니다.",
                    ));
                };
                fixture = Some(value.clone());
                index += 2;
            }
            "--prompt" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "benchmark run --prompt에는 prompt artifact path가 필요합니다.",
                    ));
                };
                prompt = Some(value.clone());
                index += 2;
            }
            "--max-tokens" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "benchmark run --max-tokens에는 양의 정수가 필요합니다.",
                    ));
                };
                let parsed = value.parse::<u32>().map_err(|_| {
                    AppError::usage("benchmark run --max-tokens에는 양의 정수가 필요합니다.")
                })?;
                if parsed == 0 {
                    return Err(AppError::usage(
                        "benchmark run --max-tokens는 1 이상이어야 합니다.",
                    ));
                }
                max_tokens = Some(parsed);
                index += 2;
            }
            _ => {
                return Err(AppError::usage(
                    "benchmark run은 --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>] 형식이 필요합니다.",
                ));
            }
        }
    }

    let Some(fixture) = fixture else {
        return Err(AppError::usage(
            "benchmark run에는 --fixture <fixture.json>이 필요합니다.",
        ));
    };
    let Some(prompt) = prompt else {
        return Err(AppError::usage(
            "benchmark run에는 --prompt <artifact>가 필요합니다.",
        ));
    };

    Ok(BenchmarkCommand::Run {
        fixture,
        prompt,
        max_tokens,
    })
}

pub(super) fn parse_benchmark_report(args: &[String]) -> Result<BenchmarkCommand, AppError> {
    match args {
        [flag, format] if flag == "--format" => {
            let format = match format.as_str() {
                "jsonl" => BenchmarkReportFormat::Jsonl,
                _ => {
                    return Err(AppError::usage(
                        "benchmark report format은 jsonl만 허용합니다.",
                    ));
                }
            };
            Ok(BenchmarkCommand::Report { format })
        }
        _ => Err(AppError::usage(
            "benchmark report에는 --format jsonl 형식이 필요합니다.",
        )),
    }
}

pub(super) fn parse_monitor_prune(args: &[String]) -> Result<MonitorCommand, AppError> {
    let mut before_days = None;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--before" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "monitor prune에는 --before 30d 같은 기간이 필요합니다.",
                    ));
                };
                before_days = Some(parse_days(value)?);
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 monitor prune 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let Some(before_days) = before_days else {
        return Err(AppError::usage(
            "monitor prune에는 --before 30d 같은 기간이 필요합니다.",
        ));
    };

    if !dry_run {
        return Err(AppError::usage(
            "monitor prune은 현재 --dry-run만 허용합니다.",
        ));
    }

    Ok(MonitorCommand::Prune {
        before_days,
        dry_run,
    })
}

fn parse_days(value: &str) -> Result<u64, AppError> {
    let Some(days) = value.strip_suffix('d') else {
        return Err(AppError::usage(
            "기간은 day 단위만 허용합니다. 예: --before 30d",
        ));
    };

    let parsed = days
        .parse::<u64>()
        .map_err(|_| AppError::usage("기간은 양의 정수 day 단위여야 합니다. 예: --before 30d"))?;

    if parsed == 0 {
        return Err(AppError::usage("기간은 1d 이상이어야 합니다."));
    }

    Ok(parsed)
}
