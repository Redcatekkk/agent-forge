use crate::generator::Generator;
use crate::scanner::{Scanner, count_candidate_files};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::{Confirm, Text};
use serde_json::{Value, json};
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Target repository or service directory to analyze.
    #[arg(default_value = ".")]
    pub target: PathBuf,

    /// Print generated SKILL.md to stdout without writing output files.
    #[arg(long)]
    pub dry_run: bool,

    /// Directory where SKILL.md and mcp-config.json should be written.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Launch an interactive setup form instead of relying only on flags.
    #[arg(long)]
    pub interactive: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Serve the generated MCP tool catalog over stdio JSON-RPC.
    Serve {
        /// Target repository or service directory to expose.
        #[arg(default_value = ".")]
        target: PathBuf,
    },
}

pub fn run() -> Result<()> {
    let args = Args::parse();
    run_with_args(args)
}

pub fn run_with_args(mut args: Args) -> Result<()> {
    if let Some(Commands::Serve { target }) = args.command {
        return serve_mcp(target);
    }

    if args.interactive {
        print_banner();
        args = apply_interactive_form(args)?;
    } else {
        print_banner();
    }

    let target = args.target.canonicalize().with_context(|| {
        format!(
            "target directory does not exist or cannot be accessed: {}",
            args.target.display()
        )
    })?;

    let candidate_count = count_candidate_files(&target).unwrap_or(0);
    let progress = if candidate_count > 0 {
        let bar = ProgressBar::new(candidate_count);
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.cyan} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}",
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        bar.set_message("source files scanned");
        bar
    } else {
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_strings(&["-", "\\", "|", "/"]),
        );
        spinner.enable_steady_tick(Duration::from_millis(80));
        spinner.set_message(format!("Scanning {}", target.display()));
        spinner
    };

    let report = Scanner::default().scan_with_progress(&target, |_| progress.inc(1))?;
    progress.finish_with_message(format!(
        "Discovered {} entry point(s)",
        report.entry_points.len()
    ));

    let rendered = Generator::default().render(&report)?;

    if args.dry_run {
        eprintln!("{}", style("DRY RUN").cyan().bold());
        println!("{}", rendered.skill_md);
        return Ok(());
    }

    let output_dir = args.output.unwrap_or_else(|| target.clone());
    fs::create_dir_all(&output_dir)?;
    fs::write(output_dir.join("SKILL.md"), rendered.skill_md)?;
    fs::write(output_dir.join("mcp-config.json"), rendered.mcp_config)?;

    println!(
        "{} {} and {}",
        style("Generated").green().bold(),
        output_dir.join("SKILL.md").display(),
        output_dir.join("mcp-config.json").display()
    );
    pause_before_exit()?;
    Ok(())
}

fn print_banner() {
    if !io::stderr().is_terminal() {
        return;
    }

    let lines = [
        "    _                    __        ______                    ",
        "   / \\   __ _  ___ _ __ / /_      / / ___|___  _ __ ___  ___ ",
        "  / _ \\ / _` |/ _ \\ '_ \\ '_ \\ /\\ / / |   / _ \\| '__/ _ \\/ __|",
        " / ___ \\ (_| |  __/ | | | | | \\ V /| |__| (_) | | |  __/\\__ \\",
        "/_/   \\_\\__, |\\___|_| |_|_| |_| \\_/  \\____\\___/|_|  \\___||___/",
        "        |___/             Forge repo context into MCP tools   ",
    ];
    let colors = [39, 45, 51, 87, 123, 159];

    for (line, color) in lines.iter().zip(colors) {
        eprintln!("{}", style(*line).color256(color).bold());
    }
    eprintln!(
        "{}",
        style("Context condensation | OpenAPI-first | MCP stdio catalog")
            .color256(245)
            .italic()
    );
}

fn pause_before_exit() -> Result<()> {
    if !io::stdin().is_terminal() {
        return Ok(());
    }

    eprintln!("{}", style("Press Enter to close...").color256(245));
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(())
}

fn apply_interactive_form(mut args: Args) -> Result<Args> {
    let target = Text::new("Repository path")
        .with_default(&args.target.to_string_lossy())
        .prompt()?;
    let dry_run = Confirm::new("Preview SKILL.md without writing files?")
        .with_default(args.dry_run)
        .prompt()?;
    let output = Text::new("Output directory, blank for repository root")
        .with_default(
            &args
                .output
                .as_ref()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_default(),
        )
        .prompt()?;

    args.target = PathBuf::from(target);
    args.dry_run = dry_run;
    args.output = if output.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(output))
    };
    Ok(args)
}

fn serve_mcp(target: PathBuf) -> Result<()> {
    let target = target.canonicalize().with_context(|| {
        format!(
            "target directory does not exist or cannot be accessed: {}",
            target.display()
        )
    })?;
    let report = Scanner::default().scan(&target)?;
    let rendered = Generator::default().render(&report)?;
    let catalog: Value = serde_json::from_str(&rendered.mcp_config)?;

    if io::stdin().is_terminal() {
        println!("{}", rendered.mcp_config);
        return Ok(());
    }

    let tools = catalog.get("tools").cloned().unwrap_or_else(|| json!([]));
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: Value = serde_json::from_str(&line)?;
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        if method.starts_with("notifications/") {
            continue;
        }

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "agent-forge",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
            "tools/list" => json!({ "tools": tools }),
            "tools/call" => json!({
                "content": [{
                    "type": "text",
                    "text": "Agent-Forge exposes static repository entry-point metadata. Use the generated route, file, and schema metadata to perform the requested action in the target codebase."
                }],
                "isError": false
            }),
            _ => json!({
                "error": {
                    "code": -32601,
                    "message": format!("unknown MCP method: {method}")
                }
            }),
        };

        let response = if result.get("error").is_some() {
            json!({ "jsonrpc": "2.0", "id": id, "error": result["error"] })
        } else {
            json!({ "jsonrpc": "2.0", "id": id, "result": result })
        };
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }

    Ok(())
}
