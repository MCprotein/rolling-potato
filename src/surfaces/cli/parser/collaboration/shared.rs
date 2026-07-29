use super::AppError;

pub(super) fn parse_write_owner_for(value: &str, command: &str) -> Result<(u32, String), AppError> {
    let Some((lane, path)) = value.split_once(':') else {
        return Err(AppError::usage(format!(
            "{command}의 --write-owner 값은 <lane:path> 형식이어야 합니다."
        )));
    };
    let lane = lane.parse::<u32>().map_err(|_| {
        AppError::usage(format!(
            "{command}의 --write-owner lane은 양의 정수여야 합니다."
        ))
    })?;
    if lane == 0 {
        return Err(AppError::usage(format!(
            "{command}의 --write-owner lane은 1 이상이어야 합니다."
        )));
    }
    if path.trim().is_empty() {
        return Err(AppError::usage(format!(
            "{command}의 --write-owner path는 비어 있을 수 없습니다."
        )));
    }
    Ok((lane, path.to_string()))
}
