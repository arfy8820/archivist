use regex::Regex;

pub fn sanitize_label(label: &str) -> String {
    let invalid = Regex::new(r#"[<>:"/\\|?*\x00-\x1F]"#).expect("valid regex");
    let repeated_dash = Regex::new("-+").expect("valid regex");
    let value = invalid.replace_all(label.trim(), "-");
    let value = pascal_case_whitespace(&value);
    repeated_dash
        .replace_all(&value, "-")
        .trim_matches('-')
        .to_string()
}

fn pascal_case_whitespace(value: &str) -> String {
    if !value.chars().any(char::is_whitespace) {
        return value.to_string();
    }

    value
        .split_whitespace()
        .map(capitalize_first_char)
        .collect::<String>()
}

fn capitalize_first_char(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn quote_arg(arg: &str) -> String {
    if arg.contains(' ') || arg.contains('\t') || arg.contains('"') {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

pub fn format_command(executable: &str, args: &[String]) -> String {
    std::iter::once(executable.to_string())
        .chain(args.iter().map(|arg| quote_arg(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn log_info(quiet: bool, message: &str) {
    if !quiet {
        println!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_label_converts_spaced_words_to_pascal_case() {
        assert_eq!(sanitize_label("the gilded shadows"), "TheGildedShadows");
        assert_eq!(sanitize_label("The Gilded Shadows"), "TheGildedShadows");
    }

    #[test]
    fn sanitize_label_preserves_existing_dash_labels() {
        assert_eq!(sanitize_label("youtube-linux"), "youtube-linux");
    }
}
