use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{Command, PatchCommand, TuiCommand};
use crate::surfaces::cli::parser;

pub(crate) trait CommandDispatchPort {
    fn terminal_attached(&mut self) -> bool;
    fn validate_native_terminal(&mut self) -> Result<(), AppError>;
    fn recover_pending_source_bundles(&mut self) -> Result<(), AppError>;
    fn execute(&mut self, command: Command) -> Result<(), AppError>;
}

pub(crate) fn run(
    args: impl IntoIterator<Item = String>,
    port: &mut impl CommandDispatchPort,
) -> Result<(), AppError> {
    let command = parser::parse(args)?;
    let terminal_attached = port.terminal_attached();
    if matches!(&command, Command::Tui(TuiCommand::Interactive))
        || (matches!(&command, Command::Tui(TuiCommand::Auto)) && terminal_attached)
    {
        port.validate_native_terminal()?;
    }

    // Unsupported source installation is a strict NotDispatched boundary.
    // Other commands recover pending source bundles before command execution.
    let unsupported_source_entry = !cfg!(unix)
        && (matches!(
            &command,
            Command::Patch(PatchCommand::Approve { dry_run: false, .. })
        ) || matches!(&command, Command::Tui(TuiCommand::Interactive))
            || (matches!(&command, Command::Tui(TuiCommand::Auto)) && terminal_attached));
    if !unsupported_source_entry {
        port.recover_pending_source_bundles()?;
    }
    port.execute(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        attached: bool,
        validated: bool,
        recovered: bool,
        executed: bool,
    }

    impl CommandDispatchPort for RecordingPort {
        fn terminal_attached(&mut self) -> bool {
            self.attached
        }

        fn validate_native_terminal(&mut self) -> Result<(), AppError> {
            self.validated = true;
            Ok(())
        }

        fn recover_pending_source_bundles(&mut self) -> Result<(), AppError> {
            self.recovered = true;
            Ok(())
        }

        fn execute(&mut self, _command: Command) -> Result<(), AppError> {
            self.executed = true;
            Ok(())
        }
    }

    #[test]
    fn ordinary_command_recovers_before_execution() {
        let mut port = RecordingPort::default();

        run(["doctor".to_string()], &mut port).unwrap();

        assert!(!port.validated);
        assert!(port.recovered);
        assert!(port.executed);
    }

    #[test]
    fn attached_auto_tui_validates_terminal_before_execution() {
        let mut port = RecordingPort {
            attached: true,
            ..RecordingPort::default()
        };

        run(["tui".to_string()], &mut port).unwrap();

        assert!(port.validated);
        assert!(port.recovered);
        assert!(port.executed);
    }
}
