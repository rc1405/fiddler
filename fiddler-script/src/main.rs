//! FiddlerScript interactive interpreter and script runner.
#![allow(unused_crate_dependencies)]

use clap::Parser;
use fiddler_script::Value;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process;

/// FiddlerScript — a minimal scripting language interpreter
#[derive(Parser)]
#[command(name = "fiddler-script")]
#[command(
    version,
    about = "Interactive interpreter and script runner for FiddlerScript"
)]
struct Cli {
    /// Execute a script string directly
    #[arg(short = 'e', long = "eval")]
    eval: Option<String>,

    /// Script file to execute. If omitted, starts the interactive REPL.
    script: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let exit_code = if let Some(code) = cli.eval {
        run_string(&code)
    } else if let Some(path) = cli.script {
        run_file(&path)
    } else if !std::io::stdin().is_terminal() {
        run_stdin()
    } else {
        run_repl()
    };

    process::exit(exit_code);
}

fn run_string(source: &str) -> i32 {
    let mut interpreter = fiddler_script::Interpreter::new();
    match interpreter.run(source) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    }
}

fn run_file(path: &std::path::Path) -> i32 {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path.display(), e);
            return 1;
        }
    };

    let mut interpreter = fiddler_script::Interpreter::new();
    match interpreter.run(&source) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    }
}

fn run_stdin() -> i32 {
    let mut source = String::new();
    if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut source) {
        eprintln!("Error reading stdin: {}", e);
        return 1;
    }
    run_string(&source)
}

fn run_repl() -> i32 {
    let mut editor = match DefaultEditor::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to initialize editor: {}", e);
            return 1;
        }
    };

    // Load history from ~/.fiddler_script_history if it exists
    let history_path = dirs_or_home().join(".fiddler_script_history");
    let _ = editor.load_history(&history_path);

    println!(
        "FiddlerScript {} — Type 'exit' or Ctrl-D to quit.",
        env!("CARGO_PKG_VERSION")
    );

    let mut interpreter = fiddler_script::Interpreter::new();
    let mut buffer = String::new();
    let mut continuation = false;

    loop {
        let prompt = if continuation { "... " } else { ">>> " };

        match editor.readline(prompt) {
            Ok(line) => {
                // Handle exit command (only at primary prompt)
                if !continuation && line.trim() == "exit" {
                    break;
                }

                if continuation {
                    buffer.push('\n');
                }
                buffer.push_str(&line);

                match interpreter.run(&buffer) {
                    Ok(value) => {
                        let _ = editor.add_history_entry(&buffer);
                        // Print non-null results (like Python prints expression values)
                        if !matches!(value, Value::Null) {
                            println!("{}", value);
                        }
                        buffer.clear();
                        continuation = false;
                    }
                    Err(fiddler_script::FiddlerError::Parse(
                        fiddler_script::ParseError::UnexpectedEof(_),
                    )) => {
                        // Incomplete input — continue reading
                        continuation = true;
                    }
                    Err(fiddler_script::FiddlerError::Lex(
                        fiddler_script::LexError::UnterminatedString(_),
                    )) => {
                        // Incomplete string — continue reading
                        continuation = true;
                    }
                    Err(e) => {
                        let _ = editor.add_history_entry(&buffer);
                        eprintln!("{}", e);
                        buffer.clear();
                        continuation = false;
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C: cancel current input
                buffer.clear();
                continuation = false;
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D: exit
                break;
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    // Save history
    let _ = editor.save_history(&history_path);
    0
}

/// Get the user's home directory, falling back to current dir.
fn dirs_or_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
