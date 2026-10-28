use backlog_cli::{Error, config::Config, safety};
use clap::{Parser, error::ErrorKind};
use std::{io::IsTerminal, process::ExitCode};

#[derive(Parser)]
#[command(name = "bl", version, about = "Backlog TUI shell")]
struct Args {
    #[arg(long, default_value = "default")]
    profile: String,
}

fn main() -> ExitCode {
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            return if error.print().is_ok() {
                ExitCode::SUCCESS
            } else {
                report(Error::new("output", "Cannot write standard output"))
            };
        }
        Err(_) => {
            return report(Error::new(
                "input",
                "Invalid command line arguments; use --help",
            ));
        }
    };
    // Panic payloads can contain external data or credentials; report only a fixed error.
    std::panic::set_hook(Box::new(|_| {}));
    let result = if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        Err(Error::new(
            "terminal",
            "bl requires both stdin and stdout to be TTYs",
        ))
    } else {
        Config::load(&args.profile).and_then(backlog_cli::tui::run)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report(error),
    }
}
fn report(error: Error) -> ExitCode {
    eprintln!("{}: {}", error.kind, safety::text(&error.message, &[]));
    ExitCode::from(error.exit_code())
}
