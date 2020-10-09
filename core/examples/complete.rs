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

//! Complete EndBASIC interpreter for programs given as arguments.
//!
//! This example sets up a complete EndBASIC interpreter: that is, one with all available commands,
//! and processes the given file.

use async_trait::async_trait;
use endbasic_core::console::{Console, Key};
use futures_lite::future::block_on;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process;
use std::rc::Rc;

/// Incomplete implementation of the EndBASIC console.
#[derive(Default)]
struct IncompleteConsole {
    buffer: VecDeque<Key>,
}

/// Converts a line of text read from stdin into a sequence of key presses.
// TODO(jmmv): Avoid duplicating this from cli/src/lib.rs.
fn line_to_keys(s: String) -> VecDeque<Key> {
    let mut keys = VecDeque::default();
    for ch in s.chars() {
        if ch == '\n' {
            keys.push_back(Key::NewLine);
        } else if ch == '\r' {
            keys.push_back(Key::CarriageReturn);
        } else {
            keys.push_back(Key::Char(ch));
        }
    }
    keys
}

#[async_trait(?Send)]
impl Console for IncompleteConsole {
    fn clear(&mut self) -> io::Result<()> {
        eprintln!("Clearing the screen not implemented");
        Ok(())
    }

    fn color(&mut self, _fg: Option<u8>, _bg: Option<u8>) -> io::Result<()> {
        eprintln!("Changing colors not implemented");
        Ok(())
    }

    fn is_interactive(&self) -> bool {
        false
    }

    fn locate(&mut self, _row: usize, _column: usize) -> io::Result<()> {
        eprintln!("Moving the cursor not implemented");
        Ok(())
    }

    fn print(&mut self, text: &str) -> io::Result<()> {
        println!("{}", text);
        Ok(())
    }

    async fn read_key(&mut self) -> io::Result<Key> {
        // TODO(jmmv): Avoid duplicating this from cli/src/lib.rs.
        if self.buffer.is_empty() {
            let mut line = String::new();
            if io::stdin().read_line(&mut line)? == 0 {
                return Ok(Key::Eof);
            }
            self.buffer = line_to_keys(line);
        }
        match self.buffer.pop_front() {
            Some(key) => Ok(key),
            None => Ok(Key::Eof),
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        stdout.write_all(bytes)?;
        stdout.flush()
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = match args.as_slice() {
        [_, path] => path,
        _ => {
            eprintln!("Usage error: expected a file name");
            process::exit(1);
        }
    };

    // TODO(jmmv): Truly add all commands in the core library and test for them.
    let console = Rc::from(RefCell::from(IncompleteConsole::default()));
    let mut machine = endbasic_core::exec::MachineBuilder::default()
        .add_builtins(endbasic_core::console::all_commands(console))
        .build();

    let mut input = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    };

    match block_on(machine.exec(&mut input)) {
        Ok(()) => (),
        Err(e) => {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    }
}
