use std::env;
use std::io::{self, Write};
use std::process;

use verse_rs::{Value, load_project_source, parse_source, run_source};

fn main() {
    if let Err(message) = run_cli() {
        eprintln!("{message}");
        process::exit(1);
    }
}

fn run_cli() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();

    match args.as_slice() {
        [] => repl(),
        [flag] if flag == "--help" || flag == "-h" => {
            print_help();
            Ok(())
        }
        [command, file] if command == "run" => run_file(file, Mode::Run),
        [command, file] if command == "ast" => run_file(file, Mode::Ast),
        [command, file] if command == "check" => run_file(file, Mode::Check),
        [file] => run_file(file, Mode::Run),
        _ => Err("invalid arguments; try `verse-rs --help`".to_string()),
    }
}

fn print_help() {
    println!(
        "verse-rs\n\nUSAGE:\n  verse-rs                 Start the REPL\n  verse-rs <file>          Run a Verse-like source file\n  verse-rs run <file>      Run a Verse-like source file\n  verse-rs check <file>    Parse and statically check a file\n  verse-rs ast <file>      Print the parsed AST\n\nREPL COMMANDS:\n  :help                    Show REPL help\n  :quit                    Exit"
    );
}

#[derive(Clone, Copy)]
enum Mode {
    Run,
    Ast,
    Check,
}

fn run_file(file: &str, mode: Mode) -> Result<(), String> {
    let source = load_project_source(file).map_err(|err| err.pretty(""))?;

    match mode {
        Mode::Ast => {
            let program = parse_source(&source).map_err(|err| err.pretty(&source))?;
            println!("{program:#?}");
            Ok(())
        }
        Mode::Check => {
            let value_type = verse_rs::check_source(&source).map_err(|err| err.pretty(&source))?;
            println!("check ok: {value_type}");
            Ok(())
        }
        Mode::Run => run_source(&source)
            .map(|_| ())
            .map_err(|err| err.pretty(&source)),
    }
}

fn repl() -> Result<(), String> {
    let stdin = io::stdin();

    println!("verse-rs REPL. Type :help for commands.");

    loop {
        print!("verse> ");
        io::stdout()
            .flush()
            .map_err(|err| format!("failed to flush stdout: {err}"))?;

        let mut line = String::new();
        let bytes = stdin
            .read_line(&mut line)
            .map_err(|err| format!("failed to read line: {err}"))?;
        if bytes == 0 {
            break;
        }

        let input = line.trim();
        match input {
            "" => continue,
            ":quit" | ":q" => break,
            ":help" => {
                println!("Enter complete source snippets.");
                println!("Examples: x: number := 41, add(a: number, b: number): number := a + b");
                continue;
            }
            _ => {}
        }

        match run_source(input) {
            Ok(value) if value != Value::None => println!("{value}"),
            Ok(_) => {}
            Err(err) => eprintln!("{}", err.pretty(input)),
        }
    }

    Ok(())
}
