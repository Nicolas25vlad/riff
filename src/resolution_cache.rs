use std::{collections::BTreeMap, fs, path::PathBuf};

use crate::{fuzzy::normalize, platform};

const FORMAT_HEADER: &str = "riff-resolution-cache-v1";
const PROVIDER_SPOTIFY: &str = "spotify";

#[derive(Debug, Default)]
pub struct ResolutionCache {
    path: PathBuf,
    entries: BTreeMap<String, String>,
    dirty: bool,
}

impl ResolutionCache {
    pub fn load_default() -> Result<Self, String> {
        let root = platform::riff_cache_dir()?;
        fs::create_dir_all(&root)
            .map_err(|err| format!("could not create Riff cache directory: {err}"))?;
        platform::secure_cache_dir(&root)?;
        Ok(Self::load(root.join("resolution-v1.tsv")))
    }

    pub fn load(path: PathBuf) -> Self {
        let Ok(source) = fs::read_to_string(&path) else {
            return Self {
                path,
                entries: BTreeMap::new(),
                dirty: false,
            };
        };

        let mut lines = source.lines();
        if lines.next() != Some(FORMAT_HEADER) {
            return Self {
                path,
                entries: BTreeMap::new(),
                dirty: false,
            };
        }

        let mut entries = BTreeMap::new();
        for line in lines {
            let mut fields = line.split('\t');
            let (Some(provider), Some(label), Some(uri), None) = (
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
            ) else {
                continue;
            };
            if provider.is_empty() || label.is_empty() || uri.is_empty() {
                continue;
            }
            entries.insert(cache_key(provider, label), uri.to_string());
        }

        Self {
            path,
            entries,
            dirty: false,
        }
    }

    pub fn spotify(&self, label: &str) -> Option<&str> {
        self.get(PROVIDER_SPOTIFY, label)
    }

    pub fn insert_spotify(&mut self, label: &str, uri: &str) {
        self.insert(PROVIDER_SPOTIFY, label, uri);
    }

    pub fn remove_spotify(&mut self, label: &str) {
        self.remove(PROVIDER_SPOTIFY, label);
    }

    pub fn get(&self, provider: &str, label: &str) -> Option<&str> {
        self.entries
            .get(&cache_key(provider, label))
            .map(String::as_str)
    }

    pub fn insert(&mut self, provider: &str, label: &str, uri: &str) {
        let key = cache_key(provider, label);
        if self.entries.get(&key).is_some_and(|value| value == uri) {
            return;
        }
        self.entries.insert(key, uri.to_string());
        self.dirty = true;
    }

    pub fn remove(&mut self, provider: &str, label: &str) {
        if self.entries.remove(&cache_key(provider, label)).is_some() {
            self.dirty = true;
        }
    }

    pub fn save(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("could not create resolution cache directory: {err}"))?;
        }

        let mut source = String::from(FORMAT_HEADER);
        source.push('\n');
        for (key, uri) in &self.entries {
            let Some((provider, label)) = key.split_once('\t') else {
                continue;
            };
            source.push_str(provider);
            source.push('\t');
            source.push_str(label);
            source.push('\t');
            source.push_str(uri);
            source.push('\n');
        }

        fs::write(&self.path, source)
            .map_err(|err| format!("could not write resolution cache: {err}"))?;
        self.dirty = false;
        Ok(())
    }
}

fn cache_key(provider: &str, label: &str) -> String {
    format!("{}\t{}", provider.trim().to_ascii_lowercase(), normalize(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("riff-{name}-{nonce}.tsv"))
    }

    #[test]
    fn cache_survives_process_style_reload() {
        let path = temp_path("resolution-cache");
        let mut cache = ResolutionCache::load(path.clone());
        cache.insert_spotify("Black Sabbath - War Pigs", "spotify:track:abc");
        cache.save().unwrap();

        let cache = ResolutionCache::load(path.clone());
        assert_eq!(
            cache.spotify("black sabbath — WAR PIGS"),
            Some("spotify:track:abc")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn changed_label_is_a_cache_miss() {
        let path = temp_path("resolution-miss");
        let mut cache = ResolutionCache::load(path.clone());
        cache.insert_spotify("Dio - Holy Diver", "spotify:track:dio");
        assert_eq!(cache.spotify("Dio - Holy Diver"), Some("spotify:track:dio"));
        assert_eq!(cache.spotify("Dio - Rainbow in the Dark"), None);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_or_old_cache_is_ignored() {
        let path = temp_path("resolution-corrupt");
        fs::write(&path, "old-format\nspotify\tthing\tspotify:track:nope\n").unwrap();
        let cache = ResolutionCache::load(path.clone());
        assert_eq!(cache.spotify("thing"), None);

        fs::write(
            &path,
            format!("{FORMAT_HEADER}\nnot-enough-fields\nspotify\tvalid song\tspotify:track:ok\n"),
        )
        .unwrap();
        let cache = ResolutionCache::load(path.clone());
        assert_eq!(cache.spotify("valid song"), Some("spotify:track:ok"));
        let _ = fs::remove_file(path);
    }
}
