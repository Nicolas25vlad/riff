from pathlib import Path

player = Path("src/player.rs")
text = player.read_text()
text = text.replace("const FUZZY_CANDIDATE_POOL: usize = 30;", "const FUZZY_CANDIDATE_POOL: usize = 48;")
old = '''    metadata.insert("album".into(), track.album.name.clone());
    metadata.insert("duration".into(), format_duration(track.duration));
'''
new = '''    metadata.insert("album".into(), track.album.name.clone());
    metadata.insert("album_year".into(), track.album.date.year().to_string());
    metadata.insert("duration".into(), format_duration(track.duration));
'''
assert old in text, "album metadata anchor not found"
player.write_text(text.replace(old, new, 1))

fuzzy = Path("src/fuzzy.rs")
text = fuzzy.read_text()
old = '''        b_score
            .cmp(&a_score)
            .then_with(|| version_penalty(a).cmp(&version_penalty(b)))
            .then_with(|| popularity(b).cmp(&popularity(a)))
            .then_with(|| a.uri.cmp(&b.uri))
'''
new = '''        b_score
            .cmp(&a_score)
            .then_with(|| version_penalty(a).cmp(&version_penalty(b)))
            .then_with(|| release_year(a).cmp(&release_year(b)))
            .then_with(|| popularity(b).cmp(&popularity(a)))
            .then_with(|| a.uri.cmp(&b.uri))
'''
assert old in text, "ranking sorter anchor not found"
text = text.replace(old, new, 1)

anchor = '''fn popularity(candidate: &SearchCandidate) -> i32 {
    candidate
        .metadata
        .get("popularity")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}
'''
replacement = '''fn release_year(candidate: &SearchCandidate) -> i32 {
    candidate
        .metadata
        .get("album_year")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|year| *year > 0)
        .unwrap_or(i32::MAX)
}

fn popularity(candidate: &SearchCandidate) -> i32 {
    candidate
        .metadata
        .get("popularity")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}
'''
assert anchor in text, "popularity helper anchor not found"
text = text.replace(anchor, replacement, 1)

old_helper = '''    fn candidate(title: &str, artist: &str) -> SearchCandidate {
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
'''
new_helper = '''    fn candidate(title: &str, artist: &str) -> SearchCandidate {
        candidate_with_release(title, artist, "Studio Album", 1970, 80, None)
    }

    fn candidate_with_release(
        title: &str,
        artist: &str,
        album: &str,
        year: i32,
        popularity: i32,
        version: Option<&str>,
    ) -> SearchCandidate {
        let mut metadata = BTreeMap::from([
            ("title".into(), title.into()),
            ("artist".into(), artist.into()),
            ("album".into(), album.into()),
            ("album_year".into(), year.to_string()),
            ("popularity".into(), popularity.to_string()),
        ]);
        if let Some(version) = version {
            metadata.insert("version".into(), version.into());
        }
        SearchCandidate {
            uri: format!("spotify:track:{title}:{album}:{year}"),
            metadata,
        }
    }
'''
assert old_helper in text, "test candidate helper anchor not found"
text = text.replace(old_helper, new_helper, 1)

footer = '''    #[test]
    fn normalizes_punctuation_and_case() {
        assert_eq!(normalize("WAR-PIGS!!"), "war pigs");
    }
}
'''
new_footer = '''    #[test]
    fn normalizes_punctuation_and_case() {
        assert_eq!(normalize("WAR-PIGS!!"), "war pigs");
    }

    #[test]
    fn prefers_earlier_release_for_equally_strong_studio_matches() {
        let later_compilation = candidate_with_release(
            "Signal Fire",
            "The Foundry",
            "Collected Works",
            2005,
            95,
            None,
        );
        let original_album = candidate_with_release(
            "Signal Fire",
            "The Foundry",
            "First Pressing",
            1972,
            70,
            None,
        );

        let ranked = rank_candidates(
            "The Foundry Signal Fire",
            vec![later_compilation, original_album],
            false,
            DEFAULT_THRESHOLD,
        );

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].metadata["album_year"], "1972");
    }

    #[test]
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
assert footer in text, "test module footer not found"
fuzzy.write_text(text.replace(footer, new_footer, 1))
