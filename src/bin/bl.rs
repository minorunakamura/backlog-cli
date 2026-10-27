use clap::Parser;

#[derive(Parser)]
#[command(
    name = "bl",
    version,
    about = "Backlog TUI (initial development)",
    arg_required_else_help = true
)]
struct Args {}

fn main() {
    Args::parse();
}
