use std::{fs, path::PathBuf};

use crate::{fuzzy::normalize, platform};

const CACHE_VERSION: &str = "v1";

#[derive(Debug, Clone)]
pub struct ResolutionCache {
    dir: PathBuf,
}

impl ResolutionCache {
    pub fn open() -> Result<Self, String> {
        let dir = platform::riff_cache_dir()?.join("resolution-v1");
        fs::create_dir_all(&dir)
            .map_err(|err| format!("could not create Riff resolution cache: {err}"))?;
        platform::secure_cache_dir(&dir)?;
        Ok(Self { dir })
    }

    pub fn get(&self, query: &str) -> Option<String> {
        let normalized = normalize(query);
        if normalized.is_empty() {
            return None;
        }
        let path = self.entry_path(&normalized);
        let source = fs::read_to_string(path).ok()?;
        let mut lines = source.lines();
        if lines.next()? != CACHE_VERSION {
            return None;
        }
        if lines.next()? != normalized {
            return None;
        }
        let uri = lines.next()?.trim();
        is_spotify_track_uri(uri).then(|| uri.to_string())
    }

    pub fn put(&self, query: &str, uri: &str) -> Result<(), String> {
        let normalized = normalize(query);
        if normalized.is_empty() || !is_spotify_track_uri(uri) {
            return Ok(());
        }
        let path = self.entry_path(&normalized);
        let temp = path.with_extension("tmp");
        let body = format!("{CACHE_VERSION}\n{normalized}\n{uri}\n");
        fs::write(&temp, body)
            .map_err(|err| format!("could not write Riff resolution cache: {err}"))?;
        fs::rename(&temp, &path)
            .map_err(|err| format!("could not commit Riff resolution cache: {err}"))
    }

    fn entry_path(&self, normalized: &str) -> PathBuf {
        self.dir
            .join(format!("{:016x}.cache", fnv1a64(normalized.as_bytes())))
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn is_spotify_track_uri(uri: &str) -> bool {
    uri.starts_with("spotify:track:") && uri.len() > "spotify:track:".len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn cache() -> ResolutionCache {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("riff-resolution-cache-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        ResolutionCache { dir }
    }

    #[test]
    fn cache_hit_survives_new_instance() {
        let cache = cache();
        cache
            .put("Black Sabbath - War Pigs", "spotify:track:abc")
            .unwrap();
        let reopened = ResolutionCache {
            dir: cache.dir.clone(),
        };
        assert_eq!(
            reopened.get("black sabbath war pigs"),
            Some("spotify:track:abc".to_string())
        );
        let _ = fs::remove_dir_all(cache.dir);
    }

    #[test]
    fn changed_query_misses() {
        let cache = cache();
        cache.put("Dio - Holy Diver", "spotify:track:def").unwrap();
        assert_eq!(cache.get("Dio - Rainbow in the Dark"), None);
        let _ = fs::remove_dir_all(cache.dir);
    }

    #[test]
    fn corrupt_entry_falls_back_to_miss() {
        let cache = cache();
        let normalized = normalize("Megadeth - Tornado of Souls");
        fs::write(cache.entry_path(&normalized), "broken\nentry\n").unwrap();
        assert_eq!(cache.get("Megadeth - Tornado of Souls"), None);
        let _ = fs::remove_dir_all(cache.dir);
    }
}
