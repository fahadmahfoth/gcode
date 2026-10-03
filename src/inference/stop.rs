//! When a generation has produced a whole command.
//!
//! A model asked for one shell command keeps going after it: a second command, an
//! explanation, a fence. Stopping at the newline that closes the command saves
//! the tokens and, more to the point, keeps a trailing sentence from ever being
//! mistaken for part of the command. The rule is pure so it is tested without a
//! model.

/// Whether `text` already holds one complete command, closed by a newline.
///
/// A leading blank line or a markdown fence opener (the ```` ```bash ```` line) is
/// skipped. After that, the first newline ends the command unless it is inside a
/// quote, escaped by a backslash, or follows a trailing `|`, `&&`, or `||`, which
/// all mean the command continues on the next line. Text with no closing newline
/// is never complete: the end of generation, not this rule, ends it.
#[must_use]
pub fn command_is_complete(text: &str) -> bool {
    let mut command = String::new();
    for line in text.split_inclusive('\n') {
        if command.is_empty() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("```") {
                continue;
            }
        }
        command.push_str(line);
        if !line.ends_with('\n') {
            return false;
        }
        let body = command.trim_end_matches(['\n', '\r']);
        if !continues(body) {
            return true;
        }
    }
    false
}

/// Whether `command`, which ended at a newline, is still open.
fn continues(command: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None | Some('"'), '\\') if chars.next().is_none() => return true,
            _ => {}
        }
    }
    if quote.is_some() {
        return true;
    }
    let tail = command.trim_end();
    tail.ends_with('|') || tail.ends_with("&&")
}

#[cfg(test)]
mod tests {
    use super::command_is_complete as done;

    #[test]
    fn a_line_closed_by_a_newline_is_complete() {
        assert!(done("ls -la\n"));
        assert!(done("find . -name '*.rs' | wc -l\n"));
    }

    #[test]
    fn text_without_a_newline_is_never_complete() {
        assert!(!done("ls -la"));
        assert!(!done(""));
    }

    #[test]
    fn a_leading_blank_line_or_fence_opener_is_skipped() {
        assert!(done("\n\nls\n"));
        assert!(done("```bash\nls -la\n"));
        assert!(!done("```bash\n"));
        assert!(!done("```bash\nls -la"));
    }

    #[test]
    fn a_newline_inside_a_quote_does_not_end_the_command() {
        assert!(!done("echo 'a\n"));
        assert!(!done("echo \"a\n"));
        assert!(done("echo 'a\nb'\n"));
    }

    #[test]
    fn a_backslash_continuation_does_not_end_the_command() {
        assert!(!done("ls \\\n"));
        assert!(done("ls \\\n -la\n"));
    }

    #[test]
    fn a_trailing_pipe_or_and_continues_the_command() {
        assert!(!done("ls |\n"));
        assert!(!done("make &&\n"));
        assert!(done("ls |\n wc -l\n"));
    }

    #[test]
    fn an_escaped_quote_does_not_open_one() {
        assert!(done("echo it\\'s\n"));
        assert!(done("echo \"say \\\"hi\\\"\"\n"));
    }

    #[test]
    fn a_double_ampersand_in_the_middle_is_not_a_continuation() {
        assert!(done("make && make install\n"));
    }

    #[test]
    fn a_second_command_after_the_first_does_not_matter() {
        assert!(done("ls\nrm -rf x\n"));
    }

    #[test]
    fn a_windows_line_ending_closes_the_command() {
        assert!(done("ls\r\n"));
    }
}
