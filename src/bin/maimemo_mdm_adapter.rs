use std::io::{self, BufRead, Read, Write};

use learning_backend::amas::memory::benchmark_adapter::{evaluate_batch, BenchmarkAdapterRequest};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let oneshot = args.iter().any(|a| a == "--oneshot");

    if oneshot {
        if let Err(err) = run_oneshot() {
            eprintln!("{err}");
            std::process::exit(1);
        }
    } else if let Err(err) = run_server() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

/// Original one-shot mode: read entire stdin, process, write stdout.
fn run_oneshot() -> Result<(), String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|err| format!("failed to read stdin: {err}"))?;

    let request: BenchmarkAdapterRequest =
        serde_json::from_str(&buffer).map_err(|err| format!("invalid request json: {err}"))?;
    let response = evaluate_batch(request)?;
    let json = serde_json::to_string(&response)
        .map_err(|err| format!("failed to encode response: {err}"))?;
    println!("{json}");
    Ok(())
}

/// JSONL server mode: read one JSON object per line, respond one JSON per line.
/// Exit on EOF or empty line.
fn run_server() -> Result<(), String> {
    let stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();

    for line in stdin.lines() {
        let line = line.map_err(|e| format!("stdin read error: {e}"))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }

        let request: BenchmarkAdapterRequest =
            serde_json::from_str(trimmed).map_err(|err| format!("invalid request json: {err}"))?;
        let response = evaluate_batch(request)?;
        let json = serde_json::to_string(&response)
            .map_err(|err| format!("failed to encode response: {err}"))?;
        writeln!(stdout, "{json}").map_err(|e| format!("stdout write error: {e}"))?;
        stdout
            .flush()
            .map_err(|e| format!("stdout flush error: {e}"))?;
    }

    Ok(())
}
