use clap::Parser;

#[derive(Parser)]
#[command(
    name = "bl-cli",
    version,
    about = "Non-interactive Backlog CLI (initial development)",
    arg_required_else_help = true
)]
struct Args {}

fn main() {
    Args::parse();
}
