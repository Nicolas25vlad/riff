from pathlib import Path

path = Path("src/fuzzy.rs")
text = path.read_text()
text = text.replace(
    "use crate::player::SearchCandidate;\n",
    "use std::cmp::Ordering;\n\nuse crate::player::SearchCandidate;\n",
    1,
)
old = '''            .then_with(|| version_penalty(a).cmp(&version_penalty(b)))
            .then_with(|| release_year(a).cmp(&release_year(b)))
            .then_with(|| popularity(b).cmp(&popularity(a)))
'''
new = '''            .then_with(|| version_penalty(a).cmp(&version_penalty(b)))
            .then_with(|| canonical_release_order(a, b))
            .then_with(|| popularity(b).cmp(&popularity(a)))
'''
assert old in text, "release-year sorter anchor not found"
text = text.replace(old, new, 1)

anchor = '''fn release_year(candidate: &SearchCandidate) -> i32 {
    candidate
        .metadata
        .get("album_year")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|year| *year > 0)
        .unwrap_or(i32::MAX)
}
'''
replacement = '''fn canonical_release_order(a: &SearchCandidate, b: &SearchCandidate) -> Ordering {
    if same_recording_identity(a, b) {
        release_year(a).cmp(&release_year(b))
    } else {
        Ordering::Equal
    }
}

fn same_recording_identity(a: &SearchCandidate, b: &SearchCandidate) -> bool {
    let identity = |candidate: &SearchCandidate| {
        let title = candidate
            .metadata
            .get("title")
            .map(String::as_str)
            .unwrap_or("");
        let artist = candidate
            .metadata
            .get("artist")
            .map(String::as_str)
            .unwrap_or("");
        (normalize(title), normalize(artist))
    };

    let a_identity = identity(a);
    let b_identity = identity(b);
    !a_identity.0.is_empty()
        && !a_identity.1.is_empty()
        && a_identity == b_identity
}

fn release_year(candidate: &SearchCandidate) -> i32 {
    candidate
        .metadata
        .get("album_year")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|year| *year > 0)
        .unwrap_or(i32::MAX)
}
'''
assert anchor in text, "release year helper anchor not found"
text = text.replace(anchor, replacement, 1)

footer = '''    #[test]
    fn keeps_alternate_versions_but_ranks_clean_studio_release_first() {
        let live = candidate_with_release(
            "Signal Fire",
            "The Foundry",
            "Live Archive",
            1971,
            99,
            Some("Live"),
        );
        let studio = candidate_with_release(
            "Signal Fire",
            "The Foundry",
            "First Pressing",
            1972,
            70,
            None,
        );

        let ranked = rank_candidates(
            "The Foundry Signal Fire",
            vec![live, studio],
            false,
            DEFAULT_THRESHOLD,
        );

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].metadata["album"], "First Pressing");
        assert_eq!(ranked[1].metadata["version"], "Live");
    }
}
'''
replacement_footer = '''    #[test]
    fn keeps_alternate_versions_but_ranks_clean_studio_release_first() {
        let live = candidate_with_release(
            "Signal Fire",
            "The Foundry",
            "Live Archive",
            1971,
            99,
            Some("Live"),
        );
        let studio = candidate_with_release(
            "Signal Fire",
            "The Foundry",
            "First Pressing",
            1972,
            70,
            None,
        );

        let ranked = rank_candidates(
            "The Foundry Signal Fire",
            vec![live, studio],
            false,
            DEFAULT_THRESHOLD,
        );

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].metadata["album"], "First Pressing");
        assert_eq!(ranked[1].metadata["version"], "Live");
    }

    #[test]
    fn release_year_does_not_bias_different_recordings() {
        let old = candidate_with_release("Signal Fire", "The Foundry", "Old Record", 1968, 50, None);
        let new = candidate_with_release("Signal Fires", "Another Band", "New Record", 2024, 90, None);
        assert_eq!(canonical_release_order(&old, &new), Ordering::Equal);
    }
}
'''
assert footer in text, "test footer anchor not found"
path.write_text(text.replace(footer, replacement_footer, 1))
