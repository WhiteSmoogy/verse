pub mod ast;
pub mod checker;
pub(crate) mod colors;
pub mod error;
pub mod eval;
pub mod lexer;
pub mod parser;
pub mod project;
pub mod token;

pub use checker::{Checker, Type, check_source};
pub use error::{Diagnostic, VerseError};
pub use eval::{Interpreter, Value};
pub use parser::parse_source;
pub use project::{check_project_file, load_project_source, run_project_file};

pub fn run_source(source: &str) -> Result<Value, VerseError> {
    let mut interpreter = Interpreter::new();
    interpreter.eval_source(source)
}
