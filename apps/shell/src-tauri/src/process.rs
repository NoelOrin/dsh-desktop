use std::path::PathBuf;

pub fn parse_dsh_web_url(line: &str) -> Option<String> {
    const PREFIX: &str = "dsh web: http://127.0.0.1:";
    let start = line.find(PREFIX)?;
    let rest = &line[start + PREFIX.len()..];
    let end = rest
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(rest.len());
    let port: u16 = rest[..end].parse().ok()?;
    Some(format!("http://127.0.0.1:{port}"))
}

pub fn classify_drop(paths: &[PathBuf]) -> &'static str {
    if paths.is_empty() {
        return "file";
    }
    let mut has_file = false;
    let mut has_dir = false;
    for path in paths {
        if path.is_dir() {
            has_dir = true;
        } else {
            has_file = true;
        }
    }
    if has_file && has_dir {
        "mixed"
    } else if has_dir {
        "directory"
    } else {
        "file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_url_line() {
        assert_eq!(
            parse_dsh_web_url("dsh web: http://127.0.0.1:49321"),
            Some("http://127.0.0.1:49321".into())
        );
        assert_eq!(parse_dsh_web_url("random log line"), None);
    }

    #[test]
    fn classifies_drop_kind() {
        let dir = std::env::temp_dir();
        assert_eq!(classify_drop(std::slice::from_ref(&dir)), "directory");
        assert_eq!(classify_drop(&[]), "file");
    }
}
