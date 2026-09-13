use std::{collections::BTreeMap, fs, io, path::PathBuf};

use crate::{fuzzy::normalize, platform};

const CACHE_HEADER: &str = "riff-resolution-cache-v1";
const CACHE_FILE: &str = "resolution-cache-v1.tsv";

#[derive(Debug)]
pub struct ResolutionCache {
    path: PathBuf,
    entries: BTreeMap<String, String>,
    dirty: bool,
}

impl ResolutionCache {
    pub fn load() -> Option<Self> {
        let path = platform::spotify_cache_dir().ok()?.join(CACHE_FILE);
        Some(Self::from_path(path))
    }

    pub fn get(&self, provider: &str, query: &str) -> Option<&str> {
        self.entries
            .get(&cache_key(provider, query))
            .map(String::as_str)
    }

    pub fn insert(&mut self, provider: &str, query: &str, uri: &str) {
        let key = cache_key(provider, query);
        if self.entries.get(&key).is_some_and(|cached| cached == uri) {
            return;
        }
        self.entries.insert(key, uri.to_string());
        self.dirty = true;
    }

    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut body = String::from(CACHE_HEADER);
        body.push('\n');
        for (key, uri) in &self.entries {
            body.push_str(key);
            body.push('\t');
            body.push_str(uri);
            body.push('\n');
        }

        fs::write(&self.path, body)?;
        self.dirty = false;
        Ok(())
    }

    fn from_path(path: PathBuf) -> Self {
        let entries = fs::read_to_string(&path)
            .ok()
            .and_then(|source| parse_cache(&source))
            .unwrap_or_default();

        Self {
            path,
            entries,
            dirty: false,
        }
    }
}

fn cache_key(provider: &str, query: &str) -> String {
    format!("{}\t{}", provider.trim().to_ascii_lowercase(), normalize(query))
}

fn parse_cache(source: &str) -> Option<BTreeMap<String, String>> {
    let mut lines = source.lines();
    if lines.next()? != CACHE_HEADER {
        return None;
    }

    let mut entries = BTreeMap::new();
    for line in lines {
        let Some((provider, rest)) = line.split_once('\t') else {
            continue;
        };
        let Some((query, uri)) = rest.split_once('\t') else {
            continue;
        };
        if provider.trim().is_empty() || query.trim().is_empty() || uri.trim().is_empty() {
            continue;
        }
        entries.insert(format!("{}\t{}", provider, query), uri.to_string());
    }
    Some(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_cache(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "riff-resolution-cache-{}-{name}-{nonce}.tsv",
            std::process::id()
        ))
    }

    #[test]
    fn normalizes_cache_keys() {
        assert_eq!(
            cache_key("Spotify", "  Black Sabbath - WAR PIGS!! "),
            "spotify\tblack sabbath war pigs"
        );
    }

    #[test]
    fn cache_round_trip_survives_process_boundaries() {
        let path = temp_cache("round-trip");
        let mut cache = ResolutionCache::from_path(path.clone());
        cache.insert(
            "spotify",
            "Black Sabbath - War Pigs",
            "spotify:track:abc123",
        );
        cache.save().expect("cache should save");

        let loaded = ResolutionCache::from_path(path.clone());
        assert_eq!(
            loaded.get("spotify", "black sabbath war pigs"),
            Some("spotify:track:abc123")
        );
        assert_eq!(loaded.get("spotify", "Dio - Holy Diver"), None);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn changing_query_is_a_cache_miss() {
        let path = temp_cache("miss");
        let mut cache = ResolutionCache::from_path(path.clone());
        cache.insert("spotify", "War Pigs", "spotify:track:abc123");

        assert_eq!(cache.get("spotify", "War Pigs"), Some("spotify:track:abc123"));
        assert_eq!(cache.get("spotify", "War Pigs live"), None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn corrupt_or_old_cache_falls_back_to_empty() {
        let path = temp_cache("corrupt");
        fs::write(&path, "riff-resolution-cache-v0\nnot valid\n")
            .expect("fixture should write");

        let loaded = ResolutionCache::from_path(path.clone());
        assert_eq!(loaded.get("spotify", "War Pigs"), None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_rows_are_ignored() {
        let path = temp_cache("malformed");
        fs::write(
            &path,
            format!(
                "{CACHE_HEADER}\ninvalid row\nspotify\twar pigs\tspotify:track:abc123\n"
            ),
        )
        .expect("fixture should write");

        let loaded = ResolutionCache::from_path(path.clone());
        assert_eq!(loaded.get("spotify", "War Pigs"), Some("spotify:track:abc123"));
        let _ = fs::remove_file(path);
    }
}
