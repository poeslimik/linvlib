//! Normalize text for search: ignore spaces and punctuation (no Hangul choseong keys).

/// Strip spaces/punctuation/symbols and lowercase for comparison.
pub fn search_norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// True when normalized `haystack` contains normalized `needle`.
pub fn norm_contains(haystack: &str, needle_norm: &str) -> bool {
    if needle_norm.is_empty() {
        return false;
    }
    search_norm(haystack).contains(needle_norm)
}

/// Derive searchable auto-keys from a series title (excluding the title itself).
/// Latin initials only (e.g. "Sword Art Online" → "sao"). Compact matching is
/// done at query time via [`search_norm`]; Hangul choseong keys are not stored.
pub fn auto_keys_for_title(title: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let initials = latin_initials(title.trim());
    if initials.chars().count() >= 2 {
        keys.push(initials);
    }
    keys
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_spaces_and_punctuation() {
        assert_eq!(
            search_norm("29세 독신은 이세계에서…"),
            search_norm("29세독신은이세계에서")
        );
        assert!(norm_contains(
            "전생했더니 슬라임이었던 건에 대하여",
            &search_norm("전생했더니슬라임이었던")
        ));
        assert!(norm_contains("무직전생!", &search_norm("무직전생")));
    }

    #[test]
    fn no_choseong_auto_keys() {
        let keys = auto_keys_for_title("전생했더니 슬라임이었던 건에 대하여");
        assert!(keys.iter().all(|k| !k.chars().any(|c| ('ㄱ'..='ㅎ').contains(&c))));
    }

    #[test]
    fn latin_initials_sao() {
        let keys = auto_keys_for_title("Sword Art Online");
        assert!(keys.iter().any(|k| k == "sao"));
    }
}
