// EndBASIC
// Copyright 2020 Julio Merino
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not
// use this file except in compliance with the License.  You may obtain a copy
// of the License at:
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
// License for the specific language governing permissions and limitations
// under the License.

//! Interactive interpreter for the EndBASIC language.

// Keep these in sync with other top-level files.
#![warn(anonymous_parameters, bad_style, missing_docs)]
#![warn(unused, unused_extern_crates, unused_import_braces, unused_qualifications)]
#![warn(unsafe_code)]

mod exit;
mod store;

use async_trait::async_trait;
use crossterm::{cursor, execute, style, terminal, tty::IsTty, QueueableCommand};
use endbasic_core::console;
use endbasic_core::exec::{ClearCommand, MachineBuilder};
use endbasic_core::help::HelpCommand;
use endbasic_core::program;
pub use exit::ExitCommand;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::cell::RefCell;
use std::io::{self, Write};
use std::path::Path;
use std::rc::Rc;
use store::FileStore;

//// Converts a `crossterm::ErrorKind` to an `io::Error`.
fn crossterm_error_to_io_error(e: crossterm::ErrorKind) -> io::Error {
    match e {
        crossterm::ErrorKind::IoError(e) => e,
        crossterm::ErrorKind::Utf8Error(e) => {
            io::Error::new(io::ErrorKind::InvalidData, format!("{}", e))
        }
        _ => io::Error::new(io::ErrorKind::Other, format!("{}", e)),
    }
}

/// Converts a `ReadLine` error into an `io::Error`.
pub(crate) fn readline_error_to_io_error(e: ReadlineError) -> io::Error {
    let kind = match e {
        ReadlineError::Eof => io::ErrorKind::UnexpectedEof,
        ReadlineError::Interrupted => io::ErrorKind::Interrupted,
        #[cfg(unix)]
        ReadlineError::Utf8Error => io::ErrorKind::InvalidData,
        #[cfg(windows)]
        ReadlineError::Decode(_) => io::ErrorKind::InvalidData,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, format!("{}", e))
}

/// Implementation of the EndBASIC console to interact with stdin and stdout.
pub struct TextConsole {
    is_tty: bool,
}

impl TextConsole {
    /// Creates a new console based on the properties of stdin/stdout.
    pub fn from_stdio() -> Self {
        Self { is_tty: io::stdout().is_tty() }
    }
}

#[async_trait(?Send)]
impl console::Console for TextConsole {
    fn clear(&mut self) -> io::Result<()> {
        execute!(io::stdout(), terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0))
            .map_err(crossterm_error_to_io_error)
    }

    fn color(&mut self, fg: Option<u8>, bg: Option<u8>) -> io::Result<()> {
        let mut output = io::stdout();
        if let Some(color) = fg {
            output
                .queue(style::SetForegroundColor(style::Color::AnsiValue(color)))
                .map_err(crossterm_error_to_io_error)?;
        }
        if let Some(color) = bg {
            output
                .queue(style::SetBackgroundColor(style::Color::AnsiValue(color)))
                .map_err(crossterm_error_to_io_error)?;
        }
        output.flush()?;
        Ok(())
    }

    async fn input(&mut self, prompt: &str, previous: &str) -> io::Result<String> {
        let answer = if self.is_tty {
            let mut rl = Editor::<()>::new();
            match rl.readline_with_initial(prompt, (previous, "")) {
                Ok(line) => line,
                Err(e) => return Err(readline_error_to_io_error(e)),
            }
        } else {
            if !prompt.is_empty() {
                let mut stdout = io::stdout();
                stdout.write_all(prompt.as_bytes())?;
                stdout.flush()?;
            }

            let mut answer = String::new();
            io::stdin().read_line(&mut answer)?;
            answer
        };
        Ok(answer.trim_end().to_owned())
    }

    fn locate(&mut self, row: usize, column: usize) -> io::Result<()> {
        if row > std::u16::MAX as usize {
            return Err(io::Error::new(io::ErrorKind::Other, "Row out of range"));
        }
        let row = row as u16;

        if column > std::u16::MAX as usize {
            return Err(io::Error::new(io::ErrorKind::Other, "Column out of range"));
        }
        let column = column as u16;

        execute!(io::stdout(), cursor::MoveTo(column, row)).map_err(crossterm_error_to_io_error)
    }

    fn print(&mut self, text: &str) -> io::Result<()> {
        println!("{}", text);
        Ok(())
    }
}

/// Enters the interactive interpreter.
///
/// `dir` specifies the directory that the interpreter will use for any commands that manipulate
/// files.
pub fn run_repl_loop(dir: &Path) -> io::Result<i32> {
    let console = Rc::from(RefCell::from(TextConsole::from_stdio()));
    let store = Rc::from(RefCell::from(FileStore::new(dir)));
    let exit_code = Rc::from(RefCell::from(None));
    let mut machine = MachineBuilder::default()
        .add_builtin(Rc::from(ClearCommand::default()))
        .add_builtin(Rc::from(ExitCommand::new(exit_code.clone())))
        .add_builtin(Rc::from(HelpCommand::new(console.clone())))
        .add_builtins(console::all_commands(console.clone()))
        .add_builtins(program::all_commands(console, store))
        .build();

    println!();
    println!("    Welcome to EndBASIC {}.", env!("CARGO_PKG_VERSION"));
    println!("    Type HELP for interactive usage information.");
    println!();

    let mut rl = Editor::<()>::new();
    while exit_code.borrow().is_none() {
        match rl.readline("Ready\n") {
            Ok(line) => {
                rl.add_history_entry(line.as_str());
                match machine.exec(&mut line.as_bytes()) {
                    Ok(()) => (),
                    Err(e) => {
                        if exit_code.borrow().is_some() {
                            if let Some(e) = e.downcast_ref::<io::Error>() {
                                debug_assert!(e.kind() == io::ErrorKind::UnexpectedEof);
                            }
                        } else {
                            println!("ERROR: {}", e);
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("Interrupted by CTRL-C");
                // TODO(jmmv): This should cause the interpreter to exit with a signal.
                *exit_code.borrow_mut() = Some(1);
            }
            Err(ReadlineError::Eof) => {
                println!("End of input by CTRL-D");
                *exit_code.borrow_mut() = Some(0);
            }
            Err(e) => return Err(readline_error_to_io_error(e)),
        }
    }
    let exit_code = exit_code.borrow();
    Ok(exit_code.expect("Some code path above did not set an exit code"))
}
