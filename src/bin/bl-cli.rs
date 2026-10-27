use backlog_cli::{
    Error,
    api::Api,
    config::Config,
    safety,
    service::{Candidate, Service},
};
use clap::{Parser, Subcommand, error::ErrorKind};
use serde_json::{Value, json};
use std::io::{self, Write};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "bl-cli", version, about = "Non-interactive Backlog CLI", arg_required_else_help = true, color = clap::ColorChoice::Never)]
struct Args {
    #[arg(long, global = true, default_value = "default")]
    profile: String,
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    no_input: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    IssueType {
        #[command(subcommand)]
        command: IssueType,
    },
    Resolution {
        #[command(subcommand)]
        command: Resolution,
    },
    Priority {
        #[command(subcommand)]
        command: Priority,
    },
    Project {
        #[command(subcommand)]
        command: Project,
    },
}
#[derive(Subcommand)]
enum Project {
    List,
}

#[derive(Subcommand)]
enum Priority {
    List,
}
#[derive(Subcommand)]
enum Resolution {
    List,
}
#[derive(Subcommand)]
enum IssueType {
    List {
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

fn write_stdout(value: &str) -> Result<(), Error> {
    io::stdout()
        .lock()
        .write_all(value.as_bytes())
        .map_err(|_| Error::new("output", "Cannot write standard output"))
}
fn report(mut error: Error, json_mode: bool, secrets: &[&str]) -> ExitCode {
    error.message = safety::text(&error.message, secrets);
    match safety::json(&Value::Array(error.api_errors.clone()), secrets) {
        Ok(Value::Array(errors)) => error.api_errors = errors,
        Err(mut output_error) => {
            output_error.http_status = error.http_status;
            output_error.message = safety::text(&output_error.message, secrets);
            error = output_error;
        }
        _ => unreachable!("safety preserves arrays"),
    }
    let code = error.exit_code();
    let message = if json_mode {
        format!("{}\n", json!({"error": error}))
    } else {
        format!("error: {}\n", error.message)
    };
    let _ = io::stderr().lock().write_all(message.as_bytes());
    ExitCode::from(code)
}
async fn run(args: &Args, config: &Config) -> Result<(), Error> {
    let (items, project_columns) = match &args.command {
        Command::IssueType {
            command: IssueType::List { project },
        } => {
            let project = project
                .as_deref()
                .or(config.settings.project.as_deref())
                .ok_or_else(|| {
                    Error::new(
                        "input",
                        "Project is required; use --project or cwd settings",
                    )
                })?;
            (
                Service::new(config)?
                    .candidates(Candidate::IssueType { project })
                    .await?,
                false,
            )
        }
        Command::Project { .. } => (Api::new(config)?.project_list().await?, true),
        Command::Resolution { .. } => (
            Service::new(config)?
                .candidates(Candidate::Resolution)
                .await?,
            false,
        ),
        Command::Priority { .. } => (
            Service::new(config)?
                .candidates(Candidate::Priority)
                .await?,
            false,
        ),
    };
    let secrets = &[config.api_key.as_str()];
    let output = if args.json {
        let safe = safety::json(&items, secrets).map_err(|mut error| {
            error.http_status = Some(200);
            error
        })?;
        format!("{}\n", json!({"items": safe}))
    } else {
        let mut output = String::from(if project_columns {
            "ID\tKEY\tNAME\n"
        } else {
            "ID\tNAME\n"
        });
        for item in items.as_array().expect("validated items array") {
            if project_columns {
                output.push_str(&format!(
                    "{}\t{}\t{}\n",
                    safety::text(&item["id"].to_string(), secrets),
                    safety::text(item["projectKey"].as_str().expect("validated key"), secrets),
                    safety::text(item["name"].as_str().expect("validated name"), secrets)
                ));
            } else {
                output.push_str(&format!(
                    "{}\t{}\n",
                    safety::text(&item["id"].to_string(), secrets),
                    safety::text(item["name"].as_str().expect("validated name"), secrets)
                ));
            }
        }
        output
    };
    write_stdout(&output).map_err(|mut error| {
        error.http_status = Some(200);
        error
    })
}
#[tokio::main]
async fn main() -> ExitCode {
    let argv: Vec<_> = std::env::args_os().collect();
    let json_mode = argv
        .iter()
        .skip(1)
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == "--json" || arg.as_encoded_bytes().starts_with(b"--json="));
    let args = match Args::try_parse_from(&argv) {
        Ok(args) => args,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayVersion
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            let code = if error.use_stderr() { 2 } else { 0 };
            if error.print().is_err() {
                return report(
                    Error::new("output", "Cannot write standard output"),
                    json_mode,
                    &[],
                );
            }
            return ExitCode::from(code);
        }
        Err(_) => {
            return report(
                Error::new("input", "Invalid command line arguments; use --help"),
                json_mode,
                &[],
            );
        }
    };
    // Clap propagates global values but permits repeats at different subcommand levels.
    for flag in ["--profile", "--json", "--no-input"] {
        let with_value = format!("{flag}=");
        if argv
            .iter()
            .skip(1)
            .take_while(|arg| *arg != "--")
            .filter(|arg| *arg == flag || arg.as_encoded_bytes().starts_with(with_value.as_bytes()))
            .count()
            > 1
        {
            return report(
                Error::new("input", "Global options must not be repeated"),
                args.json,
                &[],
            );
        }
    }
    let config = match Config::load(&args.profile) {
        Ok(config) => config,
        Err(error) => return report(error, args.json, &[]),
    };
    let result = run(&args, &config).await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => report(error, args.json, &[&config.api_key]),
    }
}
