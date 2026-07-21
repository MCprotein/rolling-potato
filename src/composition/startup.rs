//! Process startup, shutdown, and top-level command dispatch ownership.

use std::process::ExitCode;

use crate::foundation::error::AppError;

fn startup_error_message(error: &AppError) -> &str {
    &error.message
}

pub(crate) fn run(
    args: impl IntoIterator<Item = String>,
    dispatch: impl FnOnce(Vec<String>) -> Result<(), AppError>,
) -> ExitCode {
    match dispatch(args.into_iter().collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", startup_error_message(&err));
            ExitCode::from(err.code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_collected_arguments_to_dispatch() {
        let code = run(["doctor".to_string()], |args| {
            assert_eq!(args, ["doctor"]);
            Ok(())
        });

        assert_eq!(code, ExitCode::SUCCESS);
    }

    #[test]
    fn preserves_application_error_exit_code() {
        let code = run(Vec::<String>::new(), |_| {
            Err(AppError::usage("잘못된 명령"))
        });

        assert_eq!(code, ExitCode::from(2));
    }

    #[test]
    fn preserves_system_error_instead_of_applying_response_language_guard() {
        let error = AppError::blocked("TUI current-state project binding 불일치");

        assert_eq!(
            startup_error_message(&error),
            "TUI current-state project binding 불일치"
        );
    }
}
