use super::*;

pub(super) fn parse_plugin_import(args: &[String]) -> Result<PluginCommand, AppError> {
    let mut source = None;
    let mut path = None;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--from" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "plugin import에는 source runtime이 필요합니다. 예: --from codex",
                    ));
                };

                let Some(parsed) = PluginSource::parse(value) else {
                    return Err(AppError::usage(
                        "plugin source는 codex 또는 claude-code만 허용합니다.",
                    ));
                };

                source = Some(parsed);
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(AppError::usage(format!(
                    "알 수 없는 plugin import 옵션입니다: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(AppError::usage(
                        "plugin import local path는 하나만 지정할 수 있습니다.",
                    ));
                }
                path = Some(value.to_string());
                index += 1;
            }
        }
    }

    let Some(source) = source else {
        return Err(AppError::usage(
            "plugin import에는 --from codex 또는 --from claude-code가 필요합니다.",
        ));
    };

    let Some(path) = path else {
        return Err(AppError::usage(
            "plugin import에는 local plugin directory path가 필요합니다.",
        ));
    };

    Ok(PluginCommand::Import {
        source,
        path,
        dry_run,
    })
}
