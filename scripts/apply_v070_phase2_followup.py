from pathlib import Path
import re


def replace(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f"missing expected block in {path}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1))


def sub(path: str, pattern: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    updated, count = re.subn(pattern, new, text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"expected one regex match in {path}, got {count}: {pattern[:120]!r}")
    p.write_text(updated)


# Cached CLI resolutions avoid fuzzy search but still verify that Spotify can load the track.
replace(
    "src/player.rs",
    '''        if let Some(uri) = cache
            .as_ref()
            .and_then(|cache| cache.spotify(&track.label))
            .map(str::to_string)
            .filter(|uri| is_spotify_track_uri(uri))
        {
            cache_hits += 1;
            println!("  [{}/{}] cached {}", index + 1, tracks.len(), track.label);
            println!("       -> {uri}");
            resolved.push(uri);
            continue;
        }
''',
    '''        if let Some(uri) = cache
            .as_ref()
            .and_then(|cache| cache.spotify(&track.label))
            .map(str::to_string)
            .filter(|uri| is_spotify_track_uri(uri))
        {
            let valid = match SpotifyUri::from_uri(&uri) {
                Ok(spotify_uri) => SpotifyTrack::get(session, &spotify_uri).await.is_ok(),
                Err(_) => false,
            };
            if valid {
                cache_hits += 1;
                println!("  [{}/{}] cached {}", index + 1, tracks.len(), track.label);
                println!("       -> {uri}");
                resolved.push(uri);
                continue;
            }
            log::debug!("discarding stale resolution cache entry `{}`", track.label);
            if let Some(cache) = cache.as_mut() {
                cache.remove_spotify(&track.label);
            }
        }
''',
)

# Always consult secondary query shapes. The primary result page no longer starves variants.
sub(
    "src/player.rs",
    r'''async fn search_with_session\(
    session: &Session,
    query: &str,
    limit: usize,
\) -> Result<Vec<SearchCandidate>, String> \{.*?\n\}\n\nasync fn raw_search_with_session''',
    '''async fn search_with_session(
    session: &Session,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchCandidate>, String> {
    let mut raw_by_uri = BTreeMap::<String, BTreeMap<String, String>>::new();
    for (index, variant) in query_variants(query).into_iter().enumerate() {
        let per_variant = if index == 0 {
            limit.clamp(1, 36)
        } else {
            limit.clamp(1, 12)
        };
        match raw_search_with_session(session, &variant, per_variant).await {
            Ok(raw) => {
                for (uri, metadata) in raw {
                    raw_by_uri.entry(uri).or_insert(metadata);
                }
            }
            Err(error) if index == 0 => return Err(error),
            Err(error) => {
                log::debug!("secondary Spotify search variant `{variant}` failed: {error}")
            }
        }
    }

    let enrichment_cap = limit.saturating_add(12);
    let mut candidates = Vec::with_capacity(raw_by_uri.len().min(enrichment_cap));
    for (uri, metadata) in raw_by_uri.into_iter().take(enrichment_cap) {
        candidates.push(enrich_candidate(session, uri, metadata).await?);
    }
    Ok(candidates)
}

async fn raw_search_with_session''',
)
