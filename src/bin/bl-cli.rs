use backlog_cli::{
    Error,
    api::Api,
    body,
    config::Config,
    safety,
    service::{Candidate, IssueSearch, Selection, Service},
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
    Issue {
        #[command(subcommand)]
        command: Issue,
    },
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
enum Issue {
    List(ListArgs),
    Mine(SearchArgs),
    View {
        issue_id_or_key: String,
    },
    Comment {
        #[command(subcommand)]
        command: Comment,
    },
}
#[derive(Subcommand)]
enum Comment {
    View {
        issue_id_or_key: String,
        comment_id: String,
    },
    List {
        issue_id_or_key: String,
        #[arg(long, default_value = "20")]
        limit: u8,
        #[arg(long)]
        min_id: Option<String>,
        #[arg(long)]
        max_id: Option<String>,
    },
}
#[derive(clap::Args)]
struct ListArgs {
    #[command(flatten)]
    search: SearchArgs,
    #[arg(long, short = 'a', conflicts_with = "assignee_id")]
    assignee: Vec<String>,
    #[arg(long, conflicts_with = "assignee")]
    assignee_id: Vec<String>,
}
#[derive(clap::Args)]
struct SearchArgs {
    keyword: Option<String>,
    #[arg(long, short = 'p')]
    project: Option<String>,
    #[arg(long, short = 's', conflicts_with = "status_id")]
    status: Vec<String>,
    #[arg(long, conflicts_with = "status")]
    status_id: Vec<String>,
    #[arg(long, short = 'm', conflicts_with = "milestone_id")]
    milestone: Vec<String>,
    #[arg(long, conflicts_with = "milestone")]
    milestone_id: Vec<String>,
    #[arg(long, default_value = "20")]
    limit: u8,
    #[arg(long, default_value = "0")]
    offset: String,
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

fn selections<'a>(names: &'a [String], ids: &'a [String]) -> Vec<Selection<'a>> {
    names
        .iter()
        .map(|name| Selection::Name(name))
        .chain(ids.iter().map(|id| Selection::Id(id)))
        .collect()
}

fn comment_display(item: &Value, rule: &str, config: &Config) -> Result<String, Error> {
    let secrets = &[config.api_key.as_str()];
    let mut output = format!(
        "ID\t{}\nTYPE\t{}\n",
        safety::text(&item["id"].to_string(), secrets),
        if item["content"].is_null() {
            "CHANGE HISTORY"
        } else {
            "COMMENT"
        }
    );
    for (label, value) in [
        ("AUTHOR", &item["createdUser"]["name"]),
        ("CREATED", &item["created"]),
        ("UPDATED", &item["updated"]),
    ] {
        output.push_str(&format!(
            "{label}\t{}\n",
            safety::text(value.as_str().expect("validated comment field"), secrets)
        ));
    }
    if let Some(content) = item["content"].as_str() {
        output.push_str("BODY\n");
        output.push_str(&body::display_with_origin(
            content,
            rule,
            &config.origin,
            secrets,
        ));
        output.push('\n');
    }
    if let Some(changes) = item["changeLog"]
        .as_array()
        .filter(|changes| !changes.is_empty())
    {
        output.push_str("CHANGES\n");
        for change in changes {
            let field = change["field"].as_str().expect("validated history field");
            let label = match field {
                "summary" => "TITLE",
                "description" => "BODY",
                "issueType" => "TYPE",
                "priority" => "PRIORITY",
                "status" => "STATUS",
                "assigner" | "assignee" => "ASSIGNEE",
                "resolution" => "RESOLUTION",
                "startDate" => "START DATE",
                "limitDate" | "dueDate" => "DUE DATE",
                "milestone" => "MILESTONES",
                _ => field,
            };
            let mut values = Vec::new();
            for field in ["originalValue", "newValue"] {
                let safe = safety::json(&change[field], secrets)?;
                values.push(
                    safe.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| safe.to_string()),
                );
            }
            output.push_str(&format!(
                "{}\t{} -> {}\n",
                safety::text(label, secrets),
                values[0],
                values[1]
            ));
        }
    }
    output.push('\n');
    Ok(output)
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
    if let Command::Issue {
        command: Issue::Comment { command },
    } = &args.command
    {
        let service = Service::new(config)?;
        let value = match command {
            Comment::View {
                issue_id_or_key,
                comment_id,
            } => service.comment_view(issue_id_or_key, comment_id).await?,
            Comment::List {
                issue_id_or_key,
                limit,
                min_id,
                max_id,
            } => {
                service
                    .comment_list(
                        issue_id_or_key,
                        *limit,
                        min_id.as_deref(),
                        max_id.as_deref(),
                    )
                    .await?
            }
        };
        let secrets = &[config.api_key.as_str()];
        let output = if args.json {
            let safe = safety::json(&value, secrets).map_err(|mut error| {
                error.http_status = Some(200);
                error
            })?;
            format!("{safe}\n")
        } else {
            let issue_id_or_key = match command {
                Comment::List {
                    issue_id_or_key, ..
                }
                | Comment::View {
                    issue_id_or_key, ..
                } => issue_id_or_key,
            };
            let mut output = String::new();
            let items = match command {
                Comment::List { .. } => value["items"]
                    .as_array()
                    .expect("validated comments")
                    .as_slice(),
                Comment::View { .. } => std::slice::from_ref(&value),
            };
            if items.is_empty() {
                output.push_str("No comments\n");
            } else {
                let rule = if items.iter().any(|item| item["content"].is_string()) {
                    service.issue_text_formatting_rule(issue_id_or_key).await?
                } else {
                    String::new()
                };
                for item in items {
                    output.push_str(&comment_display(item, &rule, config).map_err(
                        |mut error| {
                            error.http_status = Some(200);
                            error
                        },
                    )?);
                }
            }
            output
        };
        return write_stdout(&output).map_err(|mut error| {
            error.http_status = Some(200);
            error
        });
    }
    if let Command::Issue {
        command: Issue::View { issue_id_or_key },
    } = &args.command
    {
        let service = Service::new(config)?;
        let value = service.issue_view(issue_id_or_key).await?;
        let secrets = &[config.api_key.as_str()];
        let output = if args.json {
            let safe = safety::json(&value, secrets).map_err(|mut error| {
                error.http_status = Some(200);
                error
            })?;
            format!("{safe}\n")
        } else {
            let rule = service
                .project_text_formatting_rule(
                    value["projectId"]
                        .as_number()
                        .expect("validated project ID"),
                )
                .await?;
            let mut output = String::new();
            for (label, field) in [
                ("KEY", "issueKey"),
                ("TITLE", "summary"),
                ("TYPE", "issueType"),
                ("PRIORITY", "priority"),
                ("ASSIGNEE", "assignee"),
                ("STATUS", "status"),
                ("RESOLUTION", "resolution"),
                ("START DATE", "startDate"),
                ("DUE DATE", "dueDate"),
            ] {
                let text = value[field]
                    .as_str()
                    .or_else(|| value[field]["name"].as_str())
                    .unwrap_or("-");
                output.push_str(&format!("{label}\t{}\n", safety::text(text, secrets)));
            }
            let milestones = value["milestone"]
                .as_array()
                .expect("validated milestones")
                .iter()
                .map(|item| item["name"].as_str().expect("validated milestone name"))
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!(
                "MILESTONES\t{}\n",
                safety::text(&milestones, secrets)
            ));
            for (label, field) in [
                ("CREATED", "created"),
                ("UPDATED", "updated"),
                ("URL", "url"),
            ] {
                output.push_str(&format!(
                    "{label}\t{}\n",
                    safety::text(value[field].as_str().expect("validated field"), secrets)
                ));
            }
            output.push_str("BODY\n");
            output.push_str(&body::display_with_origin(
                value["description"].as_str().unwrap_or(""),
                &rule,
                &config.origin,
                secrets,
            ));
            output.push('\n');
            output
        };
        return write_stdout(&output).map_err(|mut error| {
            error.http_status = Some(200);
            error
        });
    }
    if let Command::Issue { command } = &args.command {
        let (search, assignee, mine) = match command {
            Issue::List(list) => (
                &list.search,
                selections(&list.assignee, &list.assignee_id),
                false,
            ),
            Issue::Mine(search) => (search, vec![], true),
            Issue::View { .. } | Issue::Comment { .. } => unreachable!("handled read"),
        };
        let value = Service::new(config)?
            .search(
                &config.settings,
                IssueSearch {
                    project: search.project.as_deref(),
                    keyword: search.keyword.as_deref(),
                    status: selections(&search.status, &search.status_id),
                    assignee,
                    mine,
                    milestone: selections(&search.milestone, &search.milestone_id),
                    limit: search.limit,
                    offset: &search.offset,
                },
            )
            .await?;
        let secrets = &[config.api_key.as_str()];
        let output = if args.json {
            let safe = safety::json(&value, secrets).map_err(|mut error| {
                error.http_status = Some(200);
                error
            })?;
            format!("{safe}\n")
        } else {
            let mut output = String::from("ID\tKEY\tTITLE\tSTATUS\n");
            for item in value["items"].as_array().expect("validated items") {
                output.push_str(&format!(
                    "{}\t{}\t{}\t{}\n",
                    safety::text(&item["id"].to_string(), secrets),
                    safety::text(item["issueKey"].as_str().expect("validated key"), secrets),
                    safety::text(item["summary"].as_str().expect("validated title"), secrets),
                    safety::text(
                        item["status"]["name"].as_str().expect("validated status"),
                        secrets
                    )
                ));
            }
            output
        };
        return write_stdout(&output).map_err(|mut error| {
            error.http_status = Some(200);
            error
        });
    }
    let (items, project_columns) = match &args.command {
        Command::Issue { .. } => unreachable!("handled search"),
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
