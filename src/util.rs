use regex::Regex;

pub fn sanitize_label(label: &str) -> String {
    let invalid = Regex::new(r#"[<>:"/\\|?*\x00-\x1F]"#).expect("valid regex");
    let repeated_dash = Regex::new("-+").expect("valid regex");
    let value = invalid.replace_all(label.trim(), "-").replace(' ', "-");
    repeated_dash
        .replace_all(&value, "-")
        .trim_matches('-')
        .to_string()
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
