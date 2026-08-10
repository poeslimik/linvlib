//! Pure helpers for Hangul choseong / compact / Latin-initial search keys.

const CHOSEONG: &[char] = &[
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ',
    'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];

/// Derive searchable keys from a series title (excluding the title itself).
pub fn auto_keys_for_title(title: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return keys;
    }

    let compact = compact_key(trimmed);
    if compact.chars().count() >= 2 && !eq_ignore_case(&compact, trimmed) {
        push_unique(&mut keys, compact);
    }

    let choseong = hangul_choseong(trimmed);
    if choseong.chars().count() >= 2 {
        push_unique(&mut keys, choseong);
    }

    let initials = latin_initials(trimmed);
    if initials.chars().count() >= 2 {
        push_unique(&mut keys, initials);
    }

    keys
}

fn hangul_choseong(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ('가'..='힣').contains(&ch) {
            let idx = ((ch as u32) - 0xAC00) / (21 * 28);
            if let Some(c) = CHOSEONG.get(idx as usize) {
                out.push(*c);
            }
        }
    }
    out
}

fn compact_key(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{00a0}')
        .filter(|c| {
            !matches!(
                c,
                ':' | '：'
                    | '-'
                    | '–'
                    | '—'
                    | '/'
                    | '·'
                    | '.'
                    | ','
                    | '!'
                    | '?'
                    | '"'
                    | '\''
                    | '('
                    | ')'
                    | '['
                    | ']'
            )
        })
        .collect::<String>()
        .to_lowercase()
}

fn latin_initials(s: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = true;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            if prev_sep {
                out.push(ch.to_ascii_lowercase());
            }
            prev_sep = false;
        } else {
            prev_sep = true;
        }
    }
    out
}

fn push_unique(keys: &mut Vec<String>, value: String) {
    if value.is_empty() {
        return;
    }
    if keys.iter().any(|k| eq_ignore_case(k, &value)) {
        return;
    }
    keys.push(value);
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(|c| c.to_lowercase())
        .eq(b.chars().flat_map(|c| c.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choseong_tensura_like() {
        let keys = auto_keys_for_title("전생했더니 슬라임이었던 건에 대하여");
        assert!(keys.iter().any(|k| k.starts_with("ㅈㅅ")));
        assert!(keys.iter().any(|k| k.contains("전생했더니슬라임")));
    }

    #[test]
    fn latin_initials_sao() {
        let keys = auto_keys_for_title("Sword Art Online");
        assert!(keys.iter().any(|k| k == "sao"));
    }
}
