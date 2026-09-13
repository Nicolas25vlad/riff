from pathlib import Path

path = Path("src/workbench/player_task.rs")
text = path.read_text()

old_preload = '''                    PlayerEvent::Preloading { track_id } => {
                        let uri = track_id.to_string();
                        preload_started.insert(uri.clone(), Instant::now());
                        log::debug!("playback transition event=preloading target={uri}");
                    }
'''
new_preload = '''                    PlayerEvent::Preloading { track_id } => {
                        let uri = track_id.to_string();
                        preload_started.retain(|_, started_at| {
                            started_at.elapsed() < Duration::from_secs(180)
                        });
                        preload_started.insert(uri.clone(), Instant::now());
                        log::debug!("playback transition event=preloading target={uri}");
                    }
'''
assert old_preload in text, "preload instrumentation anchor not found"
text = text.replace(old_preload, new_preload, 1)

old_unavailable = '''                    PlayerEvent::Unavailable { track_id, .. } => {
                        log::debug!("playback transition event=unavailable track={}", track_id);
                    }
'''
new_unavailable = '''                    PlayerEvent::Unavailable { track_id, .. } => {
                        let uri = track_id.to_string();
                        preload_started.remove(&uri);
                        if let Some(probe) = transition_probe.take() {
                            log::debug!(
                                "playback transition event=unavailable trigger={} track={} elapsed_ms={}",
                                probe.trigger,
                                uri,
                                probe.elapsed_ms()
                            );
                        } else {
                            log::debug!("playback transition event=unavailable track={uri}");
                        }
                    }
'''
assert old_unavailable in text, "unavailable instrumentation anchor not found"
text = text.replace(old_unavailable, new_unavailable, 1)
path.write_text(text)
