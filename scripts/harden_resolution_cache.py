from pathlib import Path

path = Path("src/player.rs")
text = path.read_text()
old = '''        if let Some(uri) = resolution_cache
            .as_ref()
            .and_then(|cache| cache.get("spotify", &track.label))
            .filter(|uri| is_spotify_track_uri(uri))
        {
            if verbose {
                println!("  [{}/{}] cached {}", index + 1, tracks.len(), track.label);
                println!("       -> {uri}");
            }
            log::debug!("resolution cache hit for `{}` -> {uri}", track.label);
            resolved.push(uri.to_string());
            continue;
        }

        log::debug!("resolution cache miss for `{}`", track.label);
'''
new = '''        let cached_uri = resolution_cache
            .as_ref()
            .and_then(|cache| cache.get("spotify", &track.label))
            .map(str::to_string);
        if let Some(uri) = cached_uri {
            if cached_track_is_valid(session, &uri).await {
                if verbose {
                    println!("  [{}/{}] cached {}", index + 1, tracks.len(), track.label);
                    println!("       -> {uri}");
                }
                log::debug!("resolution cache hit for `{}` -> {uri}", track.label);
                resolved.push(uri);
                continue;
            }
            log::debug!(
                "stale resolution cache entry for `{}` -> {uri}; resolving again",
                track.label
            );
        } else {
            log::debug!("resolution cache miss for `{}`", track.label);
        }
'''
assert old in text, "cache hit block not found"
text = text.replace(old, new, 1)

anchor = '''fn is_spotify_track_uri(uri: &str) -> bool {
    matches!(SpotifyUri::from_uri(uri), Ok(SpotifyUri::Track { .. }))
}
'''
replacement = '''async fn cached_track_is_valid(session: &Session, uri: &str) -> bool {
    let Ok(spotify_uri @ SpotifyUri::Track { .. }) = SpotifyUri::from_uri(uri) else {
        return false;
    };
    SpotifyTrack::get(session, &spotify_uri).await.is_ok()
}

fn is_spotify_track_uri(uri: &str) -> bool {
    matches!(SpotifyUri::from_uri(uri), Ok(SpotifyUri::Track { .. }))
}
'''
assert anchor in text, "track URI helper anchor not found"
path.write_text(text.replace(anchor, replacement, 1))
