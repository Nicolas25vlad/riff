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


# Export the persistent provider cache.
replace(
    "src/lib.rs",
    "pub mod player;\n",
    "pub mod player;\npub mod resolution_cache;\n",
)

# Broaden Spotify search, expose session reuse, enrich result metadata, and cache CLI resolves.
replace(
    "src/player.rs",
    '''use crate::{
    fuzzy::{DEFAULT_THRESHOLD, rank_candidates},
    platform,
};''',
    '''use crate::{
    fuzzy::{DEFAULT_THRESHOLD, rank_candidates},
    platform,
    resolution_cache::ResolutionCache,
};''',
)
replace("src/player.rs", "const FUZZY_CANDIDATE_POOL: usize = 30;", "const FUZZY_CANDIDATE_POOL: usize = 48;")

replace(
    "src/player.rs",
    '''pub async fn search_advanced(
    query: &str,
    limit: usize,
    exact: bool,
    threshold: u8,
) -> Result<Vec<SearchCandidate>, String> {
    let session = discovery_session().await?;
    let result = smart_search_with_session(&session, query, limit, exact, threshold).await;
    session.shutdown();
    result
}
''',
    '''pub async fn search_advanced(
    query: &str,
    limit: usize,
    exact: bool,
    threshold: u8,
) -> Result<Vec<SearchCandidate>, String> {
    let session = discovery_session().await?;
    let result = search_in_session(&session, query, limit, exact, threshold).await;
    session.shutdown();
    result
}

pub async fn search_in_session(
    session: &Session,
    query: &str,
    limit: usize,
    exact: bool,
    threshold: u8,
) -> Result<Vec<SearchCandidate>, String> {
    smart_search_with_session(session, query, limit, exact, threshold).await
}
''',
)

sub(
    "src/player.rs",
    r'''async fn resolve_tracks\(session: &Session, tracks: &\[TrackRequest\]\) -> Result<Vec<String>, String> \{.*?\n\}\n\nasync fn discovery_session''',
    '''async fn resolve_tracks(session: &Session, tracks: &[TrackRequest]) -> Result<Vec<String>, String> {
    let mut resolved = Vec::with_capacity(tracks.len());
    let mut cache = ResolutionCache::load_default().ok();
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;

    for (index, track) in tracks.iter().enumerate() {
        if let Some(uri) = track.id.as_deref() {
            if !is_spotify_track_uri(uri) {
                return Err(format!(
                    "invalid pinned id for `{}`: expected `spotify:track:<id>`, got `{uri}`",
                    track.label
                ));
            }
            println!("  [{}/{}] pinned {}", index + 1, tracks.len(), track.label);
            println!("       -> {uri}");
            resolved.push(uri.to_string());
            continue;
        }

        if let Some(uri) = cache
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

        cache_misses += 1;
        println!(
            "  [{}/{}] resolving {}",
            index + 1,
            tracks.len(),
            track.label
        );
        let candidate = smart_search_with_session(
            session,
            &track.label,
            1,
            false,
            DEFAULT_THRESHOLD,
        )
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| format!("no confident Spotify track found for `{}`", track.label))?;
        let confidence = candidate
            .metadata
            .get("match")
            .map(String::as_str)
            .unwrap_or("?");
        println!(
            "       -> {} ({}, {confidence}% match)",
            candidate.display_name(),
            candidate.uri
        );
        if let Some(cache) = cache.as_mut() {
            cache.insert_spotify(&track.label, &candidate.uri);
        }
        resolved.push(candidate.uri);
    }

    if let Some(cache) = cache.as_mut()
        && let Err(error) = cache.save()
    {
        log::warn!("resolution cache could not be saved: {error}");
    }
    log::debug!("resolution cache: {cache_hits} hit(s), {cache_misses} miss(es)");
    Ok(resolved)
}

async fn discovery_session''',
)

sub(
    "src/player.rs",
    r'''async fn smart_search_with_session\(.*?\n\}\n\nasync fn enrich_candidate''',
    '''async fn smart_search_with_session(
    session: &Session,
    query: &str,
    limit: usize,
    exact: bool,
    threshold: u8,
) -> Result<Vec<SearchCandidate>, String> {
    let pool_size = FUZZY_CANDIDATE_POOL.max(limit.saturating_mul(4));
    let raw = search_with_session(session, query, pool_size).await?;
    let mut ranked = rank_candidates(query, raw, exact, threshold);
    ranked.truncate(limit);
    Ok(ranked)
}

fn query_variants(query: &str) -> Vec<String> {
    let query = query.trim();
    let mut variants = Vec::new();
    if !query.is_empty() {
        variants.push(query.to_string());
    }
    if let Some((artist, title)) = query.split_once(" - ") {
        let artist = artist.trim();
        let title = title.trim();
        if !artist.is_empty() && !title.is_empty() {
            variants.push(format!("{title} {artist}"));
            variants.push(title.to_string());
        }
    }
    variants.dedup();
    variants
}

async fn search_with_session(
    session: &Session,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchCandidate>, String> {
    let mut raw_by_uri = BTreeMap::<String, BTreeMap<String, String>>::new();
    for (index, variant) in query_variants(query).into_iter().enumerate() {
        match raw_search_with_session(session, &variant, limit).await {
            Ok(raw) => {
                for (uri, metadata) in raw {
                    raw_by_uri.entry(uri).or_insert(metadata);
                    if raw_by_uri.len() >= limit {
                        break;
                    }
                }
            }
            Err(error) if index == 0 => return Err(error),
            Err(error) => log::debug!("secondary Spotify search variant `{variant}` failed: {error}"),
        }
        if raw_by_uri.len() >= limit {
            break;
        }
    }

    let mut candidates = Vec::with_capacity(raw_by_uri.len());
    for (uri, metadata) in raw_by_uri.into_iter().take(limit) {
        candidates.push(enrich_candidate(session, uri, metadata).await?);
    }
    Ok(candidates)
}

async fn raw_search_with_session(
    session: &Session,
    query: &str,
    limit: usize,
) -> Result<Vec<(String, BTreeMap<String, String>)>, String> {
    let search_uri = spotify_search_uri(query);
    let context = session
        .spclient()
        .get_context(&search_uri)
        .await
        .map_err(|err| format!("Spotify internal search failed for `{query}`: {err}"))?;

    let mut candidates = Vec::new();
    for track in context.pages.into_iter().flat_map(|page| page.tracks) {
        let Some(uri) = track.uri else {
            continue;
        };
        if !is_spotify_track_uri(&uri) {
            continue;
        }
        candidates.push((
            uri,
            track.metadata.into_iter().collect::<BTreeMap<_, _>>(),
        ));
        if candidates.len() == limit {
            break;
        }
    }
    Ok(candidates)
}

async fn enrich_candidate''',
)

replace(
    "src/player.rs",
    '''    metadata.insert("album".into(), track.album.name.clone());
    metadata.insert("duration".into(), format_duration(track.duration));
    metadata.insert("popularity".into(), track.popularity.to_string());
''',
    '''    metadata.insert("album".into(), track.album.name.clone());
    metadata.insert("duration".into(), format_duration(track.duration));
    metadata.insert("duration_ms".into(), track.duration.max(0).to_string());
    metadata.insert("popularity".into(), track.popularity.to_string());
    if let Some(cover) = track
        .album
        .covers
        .iter()
        .max_by_key(|cover| cover.width.saturating_mul(cover.height))
    {
        metadata.insert("cover_id".into(), cover.id.to_string());
    }
''',
)

replace(
    "src/player.rs",
    '''    #[test]
    fn formats_track_duration() {
        assert_eq!(format_duration(475_000), "7:55");
    }
''',
    '''    #[test]
    fn formats_track_duration() {
        assert_eq!(format_duration(475_000), "7:55");
    }

    #[test]
    fn broadens_artist_dash_title_queries_without_hardcoding_music() {
        assert_eq!(
            query_variants("Artist Name - Song Name"),
            vec![
                "Artist Name - Song Name".to_string(),
                "Song Name Artist Name".to_string(),
                "Song Name".to_string(),
            ]
        );
        assert_eq!(query_variants("Song Name"), vec!["Song Name".to_string()]);
    }
''',
)

# Prefer canonical album releases over obvious compilations when fuzzy score ties.
replace(
    "src/fuzzy.rs",
    '''        "radio edit",
        "re-record",
''',
    '''        "radio edit",
        "re-record",
        "greatest hits",
        "best of",
        "anthology",
        "collection",
        "compilation",
        "essentials",
''',
)
replace(
    "src/fuzzy.rs",
    '''    fn normalizes_punctuation_and_case() {
        assert_eq!(normalize("WAR-PIGS!!"), "war pigs");
    }
''',
    '''    fn normalizes_punctuation_and_case() {
        assert_eq!(normalize("WAR-PIGS!!"), "war pigs");
    }

    #[test]
    fn canonical_album_beats_compilation_when_match_is_equal() {
        let mut canonical = candidate("Example Song", "Example Artist");
        canonical.metadata.insert("album".into(), "Original Album".into());
        let mut compilation = candidate("Example Song", "Example Artist");
        compilation.uri = "spotify:track:compilation".into();
        compilation
            .metadata
            .insert("album".into(), "Example Artist Greatest Hits".into());
        compilation.metadata.insert("popularity".into(), "99".into());

        let ranked = rank_candidates(
            "Example Artist Example Song",
            vec![compilation, canonical.clone()],
            false,
            DEFAULT_THRESHOLD,
        );
        assert_eq!(ranked.first().map(|item| item.uri.as_str()), Some(canonical.uri.as_str()));
    }
''',
)

# Workbench resolver and search reuse their existing Spotify sessions and cache exact resolutions.
replace(
    "src/workbench/player_task.rs",
    'use riff::{Playlist, platform, player};',
    'use riff::{Playlist, platform, player, resolution_cache::ResolutionCache};',
)
sub(
    "src/workbench/player_task.rs",
    r'''pub async fn resolve_queue\(playlist: &Playlist\) -> Result<Vec<QueueItem>, String> \{.*?\n\}\n\npub async fn run_player''',
    '''pub async fn resolve_queue(playlist: &Playlist) -> Result<Vec<QueueItem>, String> {
    let (session_config, cache, credentials) = session_parts()?;
    let session = Session::new(session_config, Some(cache));
    session
        .connect(credentials, false)
        .await
        .map_err(|err| format!("could not connect to Spotify for TUI metadata: {err}"))?;

    let mut resolution_cache = ResolutionCache::load_default().ok();
    let mut queue = Vec::with_capacity(playlist.tracks.len());
    let mut cache_hits = 0usize;
    let mut cache_misses = 0usize;

    for request in &playlist.tracks {
        if let Some(uri) = request.id.as_deref() {
            queue.push(queue_item_from_uri(&session, uri).await?);
            continue;
        }

        if let Some(uri) = resolution_cache
            .as_ref()
            .and_then(|cache| cache.spotify(&request.label))
            .map(str::to_string)
        {
            match queue_item_from_uri(&session, &uri).await {
                Ok(item) => {
                    cache_hits += 1;
                    queue.push(item);
                    continue;
                }
                Err(error) => {
                    log::debug!("discarding stale resolution cache entry `{}`: {error}", request.label);
                    if let Some(cache) = resolution_cache.as_mut() {
                        cache.remove_spotify(&request.label);
                    }
                }
            }
        }

        cache_misses += 1;
        let candidate = player::search_in_session(
            &session,
            &request.label,
            1,
            false,
            riff::fuzzy::DEFAULT_THRESHOLD,
        )
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| format!("no confident Spotify track found for `{}`", request.label))?;
        if let Some(cache) = resolution_cache.as_mut() {
            cache.insert_spotify(&request.label, &candidate.uri);
        }
        queue.push(queue_item_from_candidate(candidate)?);
    }

    if let Some(cache) = resolution_cache.as_mut()
        && let Err(error) = cache.save()
    {
        log::warn!("resolution cache could not be saved: {error}");
    }
    log::debug!("Workbench resolution cache: {cache_hits} hit(s), {cache_misses} miss(es)");
    session.shutdown();
    Ok(queue)
}

pub async fn run_player''',
)

sub(
    "src/workbench/player_task.rs",
    r'''fn spawn_search\(session: Session, query: String, updates: mpsc::UnboundedSender<PlayerUpdate>\) \{.*?\n\}\n\nasync fn queue_item_from_uri''',
    '''fn spawn_search(session: Session, query: String, updates: mpsc::UnboundedSender<PlayerUpdate>) {
    tokio::spawn(async move {
        let candidates = match player::search_in_session(
            &session,
            &query,
            20,
            false,
            riff::fuzzy::DEFAULT_THRESHOLD,
        )
        .await
        {
            Ok(candidates) => candidates,
            Err(error) => {
                let _ = updates.send(PlayerUpdate::SearchError { query, error });
                return;
            }
        };

        let mut results = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            match queue_item_from_candidate(candidate) {
                Ok(item) => results.push(item),
                Err(error) => {
                    let _ = updates.send(PlayerUpdate::SearchError {
                        query: query.clone(),
                        error,
                    });
                    return;
                }
            }
        }
        let _ = updates.send(PlayerUpdate::SearchResults { query, results });
    });
}

fn queue_item_from_candidate(candidate: player::SearchCandidate) -> Result<QueueItem, String> {
    let metadata = &candidate.metadata;
    let title = metadata
        .get("title")
        .cloned()
        .ok_or_else(|| format!("search result `{}` is missing title metadata", candidate.uri))?;
    let artist = metadata.get("artist").cloned().unwrap_or_default();
    let album = metadata.get("album").cloned().unwrap_or_default();
    let duration_ms = metadata
        .get("duration_ms")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let match_score = metadata
        .get("match")
        .and_then(|value| value.parse::<u8>().ok());

    Ok(QueueItem {
        title,
        artist,
        album,
        version: metadata.get("version").cloned(),
        uri: candidate.uri,
        duration_ms,
        cover_id: metadata.get("cover_id").cloned(),
        match_score,
    })
}

async fn queue_item_from_uri''',
)

# Fix the startup copy while the resolver is intentionally opaque about exact percentage progress.
replace(
    "src/workbench/mod.rs",
    '''        Line::from(format!(
            "resolving {} track{} · Spotify session + metadata",
            workbench.state.playlist_name,
            if workbench.state.queue.len() == 1 {
                ""
            } else {
                "s"
            }
        )),''',
    '''        Line::from("resolving playlist · cache + Spotify metadata"),''',
)
