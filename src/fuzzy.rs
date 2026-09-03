use crate::player::SearchCandidate;

pub const DEFAULT_THRESHOLD: u8 = 42;

pub fn rank_candidates(
    query: &str,
    mut candidates: Vec<SearchCandidate>,
    exact: bool,
    threshold: u8,
) -> Vec<SearchCandidate> {
    let normalized_query = normalize(query);

    for candidate in &mut candidates {
        let title = candidate.metadata.get("title").map(String::as_str).unwrap_or("");
        let artist = candidate.metadata.get("artist").map(String::as_str).unwrap_or("");
        let combined = if artist.is_empty() {
            title.to_string()
        } else {
            format!("{artist} {title}")
        };

        let score = if exact {
            let normalized_title = normalize(title);
            let normalized_combined = normalize(&combined);
            if normalized_query == normalized_title || normalized_query == normalized_combined {
                100
            } else {
                0
            }
        } else {
            fuzzy_score(&normalized_query, &normalize(&combined), &normalize(title))
        };

        candidate.metadata.insert("match".into(), score.to_string());
    }

    candidates.retain(|candidate| {
        candidate
            .metadata
            .get("match")
            .and_then(|score| score.parse::<u8>().ok())
            .is_some_and(|score| score >= threshold)
    });

    candidates.sort_by(|a, b| {
        let a_score = a
            .metadata
            .get("match")
            .and_then(|score| score.parse::<u8>().ok())
            .unwrap_or(0);
        let b_score = b
            .metadata
            .get("match")
            .and_then(|score| score.parse::<u8>().ok())
            .unwrap_or(0);
        b_score
            .cmp(&a_score)
            .then_with(|| version_penalty(a).cmp(&version_penalty(b)))
            .then_with(|| popularity(b).cmp(&popularity(a)))
            .then_with(|| a.uri.cmp(&b.uri))
    });

    candidates
}

fn fuzzy_score(query: &str, combined: &str, title: &str) -> u8 {
    if query.is_empty() || combined.is_empty() {
        return 0;
    }

    let combined_edit = normalized_edit_similarity(query, combined);
    let title_edit = normalized_edit_similarity(query, title);
    let token = token_similarity(query, combined);
    let containment = if combined.contains(query) || query.contains(combined) {
        1.0
    } else {
        0.0
    };

    let score = combined_edit * 0.45 + title_edit * 0.20 + token * 0.30 + containment * 0.05;
    (score * 100.0).round().clamp(0.0, 100.0) as u8
}

fn normalized_edit_similarity(a: &str, b: &str) -> f64 {
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let distance = levenshtein(a, b);
    1.0 - distance as f64 / max_len as f64
}

fn token_similarity(a: &str, b: &str) -> f64 {
    let a_tokens = a.split_whitespace().collect::<Vec<_>>();
    let b_tokens = b.split_whitespace().collect::<Vec<_>>();
    if a_tokens.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }

    let mut matched = 0.0;
    for token in &a_tokens {
        let best = b_tokens
            .iter()
            .map(|candidate| normalized_edit_similarity(token, candidate))
            .fold(0.0, f64::max);
        if best >= 0.55 {
            matched += best;
        }
    }

    matched / a_tokens.len().max(b_tokens.len()) as f64
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b_chars = b.chars().collect::<Vec<_>>();
    let mut previous = (0..=b_chars.len()).collect::<Vec<_>>();

    for (i, a_char) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, b_char) in b_chars.iter().enumerate() {
            let insert = current[j] + 1;
            let delete = previous[j + 1] + 1;
            let replace = previous[j] + usize::from(a_char != *b_char);
            current.push(insert.min(delete).min(replace));
        }
        previous = current;
    }

    previous[b_chars.len()]
}

pub fn normalize(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn version_penalty(candidate: &SearchCandidate) -> u8 {
    let haystack = format!(
        "{} {} {}",
        candidate.metadata.get("title").map(String::as_str).unwrap_or(""),
        candidate.metadata.get("album").map(String::as_str).unwrap_or(""),
        candidate.metadata.get("version").map(String::as_str).unwrap_or("")
    )
    .to_lowercase();

    [
        "live",
        "remaster",
        "acoustic",
        "demo",
        "karaoke",
        "tribute",
        "instrumental",
        "radio edit",
        "re-record",
    ]
    .iter()
    .filter(|marker| haystack.contains(**marker))
    .count() as u8
}

fn popularity(candidate: &SearchCandidate) -> i32 {
    candidate
        .metadata
        .get("popularity")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn candidate(title: &str, artist: &str) -> SearchCandidate {
        SearchCandidate {
            uri: format!("spotify:track:{title}"),
            metadata: BTreeMap::from([
                ("title".into(), title.into()),
                ("artist".into(), artist.into()),
                ("album".into(), "Paranoid".into()),
                ("popularity".into(), "80".into()),
            ]),
        }
    }

    #[test]
    fn tolerates_typo_in_artist_and_title() {
        let ranked = rank_candidates(
            "black sabath war pig",
            vec![candidate("War Pigs", "Black Sabbath")],
            false,
            DEFAULT_THRESHOLD,
        );
        assert_eq!(ranked.len(), 1);
        let score = ranked[0].metadata["match"].parse::<u8>().unwrap();
        assert!(score >= 70, "score was {score}");
    }

    #[test]
    fn exact_mode_rejects_typo() {
        let ranked = rank_candidates(
            "black sabath war pig",
            vec![candidate("War Pigs", "Black Sabbath")],
            true,
            100,
        );
        assert!(ranked.is_empty());
    }

    #[test]
    fn normalizes_punctuation_and_case() {
        assert_eq!(normalize("WAR-PIGS!!"), "war pigs");
    }
}
