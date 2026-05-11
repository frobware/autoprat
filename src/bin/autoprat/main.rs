mod display;
mod log_fetcher;

use std::io::IsTerminal;

use autoprat::{GitHub, fetch_pull_requests, parse_args};
use display::{display_pr_table, output_shell_commands};

/// Decide whether to format output for a human-readable terminal.
///
/// Returns true when stdout is a tty, or when the AUTOPRAT_FORCE_TTY
/// environment variable is set to a truthy value (1, true, yes;
/// case-insensitive). This mirrors gh's GH_FORCE_TTY behaviour: a way
/// to keep the rich table output when piping into a pager or capturing
/// into a file.
fn should_use_tty_output() -> bool {
    if let Ok(val) = std::env::var("AUTOPRAT_FORCE_TTY") {
        let v = val.trim().to_ascii_lowercase();
        if matches!(v.as_str(), "1" | "true" | "yes") {
            return true;
        }
        if matches!(v.as_str(), "0" | "false" | "no" | "") {
            return false;
        }
    }
    std::io::stdout().is_terminal()
}

fn handle_clap_help_version(clap_err: &clap::Error) -> ! {
    use clap::error::ErrorKind;
    match clap_err.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            print!("{clap_err}");
            std::process::exit(0);
        }
        _ => {
            eprint!("{clap_err}");
            std::process::exit(2);
        }
    }
}

fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).with_writer(std::io::stderr))
        .with(filter)
        .init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let (request, display_mode) = match parse_args(std::env::args()) {
        Ok(result) => result,
        Err(err) => {
            if let Some(clap_err) = err.downcast_ref::<clap::Error>() {
                handle_clap_help_version(clap_err);
            } else {
                return Err(err);
            }
        }
    };

    let result = fetch_pull_requests(&request, &GitHub).await?;
    let mut stdout = std::io::stdout();

    if request.has_actions() {
        output_shell_commands(&result.executable_actions, &mut stdout)?;
    } else {
        display_pr_table(
            &result.filtered_prs,
            &display_mode,
            request.truncate_titles,
            should_use_tty_output(),
            &mut stdout,
        )
        .await?;
    }

    Ok(())
}
