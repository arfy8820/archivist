use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::cell::RefCell;
use std::io::{self, IsTerminal, Write};

thread_local! {
    static EDITOR: RefCell<Option<DefaultEditor>> = RefCell::new(DefaultEditor::new().ok());
}

pub fn prompt(message: &str) -> Option<String> {
    if !io::stdin().is_terminal() {
        return fallback_prompt(message, false);
    }

    EDITOR.with(|editor| {
        let mut editor = editor.borrow_mut();
        match editor.as_mut() {
            Some(editor) => match editor.readline(message) {
                Ok(line) => {
                    if !line.trim().is_empty() {
                        let _ = editor.add_history_entry(line.as_str());
                    }
                    Some(line)
                }
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => cancel_prompt(),
                Err(_) => fallback_prompt(message, true),
            },
            None => fallback_prompt(message, true),
        }
    })
}

pub fn prompt_required(message: &str) -> String {
    loop {
        let Some(value) = prompt(message) else {
            cancel_prompt();
        };

        let value = value.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
}

pub fn prompt_optional(message: &str) -> Option<String> {
    prompt(message).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn confirm_yes_default(message: &str) -> bool {
    match prompt(message) {
        None => true,
        Some(value) if value.trim().is_empty() => true,
        Some(value) => matches!(value.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
    }
}

pub fn confirm_no_default(message: &str) -> bool {
    match prompt(message) {
        None => false,
        Some(value) if value.trim().is_empty() => false,
        Some(value) => matches!(value.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
    }
}

fn fallback_prompt(message: &str, cancel_on_eof: bool) -> Option<String> {
    print!("{message}");
    io::stdout().flush().ok()?;
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) if cancel_on_eof => cancel_prompt(),
        Ok(0) => None,
        Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        Err(_) => None,
    }
}

fn cancel_prompt() -> ! {
    println!();
    std::process::exit(130);
}
