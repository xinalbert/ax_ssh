const MAX_LABEL_CHARS: usize = 96;

pub(super) fn parse(output: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(output);
    let mut pretty = None;
    let mut name = None;
    let mut version = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches(|character| character == '"' || character == '\'');
        match key.trim() {
            "PRETTY_NAME" => pretty = Some(value.to_owned()),
            "NAME" => name = Some(value.to_owned()),
            "VERSION_ID" => version = Some(value.to_owned()),
            _ => {}
        }
    }
    let candidate = pretty
        .filter(|value| !value.is_empty())
        .or_else(|| match (name, version) {
            (Some(name), Some(version)) if !name.is_empty() && !version.is_empty() => {
                Some(format!("{name} {version}"))
            }
            _ => None,
        })
        .or_else(|| {
            text.lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_owned)
        });
    sanitize(candidate)
}

fn sanitize(candidate: Option<String>) -> Option<String> {
    let mut value = candidate?;
    value.retain(|character| !character.is_control());
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(MAX_LABEL_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn prefers_pretty_name() {
        assert_eq!(
            parse(b"NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n"),
            Some("Ubuntu 24.04 LTS".to_owned())
        );
    }

    #[test]
    fn falls_back_to_first_line_and_bounds_controls() {
        assert_eq!(
            parse(b"\nDarwin Kernel Version 24.0\n"),
            Some("Darwin Kernel Version 24.0".to_owned())
        );
        assert!(parse(&[b'x'; 200]).is_some_and(|value| value.len() == 96));
    }
}
