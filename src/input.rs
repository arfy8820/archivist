use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::cell::RefCell;
use std::io::{self, IsTerminal, Write};

thread_local! {
    static EDITOR: RefCell<Option<DefaultEditor>> = RefCell::new(DefaultEditor::new().ok());
}

pub fn prompt(message: &str) -> Option<String> {
    if !io::stdin().is_terminal() {
        return fallback_prompt(message);
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
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => None,
                Err(_) => fallback_prompt(message),
            },
            None => fallback_prompt(message),
        }
    })
}

pub fn prompt_required(message: &str) -> String {
    loop {
        if let Some(value) = prompt(message) {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
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

fn fallback_prompt(message: &str) -> Option<String> {
    print!("{message}");
    io::stdout().flush().ok()?;
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        Err(_) => None,
    }
}
