from pathlib import Path

path = Path("src/workbench/player_task.rs")
text = path.read_text()
text = text.replace("    time::Duration,\n", "    time::{Duration, Instant},\n", 1)

anchor = '''const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-playback-state",
    "user-modify-playback-state",
];
'''
addition = '''const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-playback-state",
    "user-modify-playback-state",
];

#[derive(Debug)]
struct TransitionProbe {
    trigger: &'static str,
    from_uri: Option<String>,
    started_at: Instant,
}

impl TransitionProbe {
    fn new(trigger: &'static str, from_uri: Option<String>) -> Self {
        Self {
            trigger,
            from_uri,
            started_at: Instant::now(),
        }
    }

    fn elapsed_ms(&self) -> u128 {
        self.started_at.elapsed().as_millis()
    }
}
'''
assert anchor in text, "oauth scope anchor not found"
text = text.replace(anchor, addition, 1)

anchor = '''    let lyrics_pending = Arc::new(Mutex::new(HashSet::<String>::new()));

    tokio::pin!(spirc_task);
'''
addition = '''    let lyrics_pending = Arc::new(Mutex::new(HashSet::<String>::new()));
    let mut current_uri: Option<String> = None;
    let mut transition_probe: Option<TransitionProbe> = None;
    let mut preload_started = HashMap::<String, Instant>::new();

    tokio::pin!(spirc_task);
'''
assert anchor in text, "runtime state anchor not found"
text = text.replace(anchor, addition, 1)

old_controls = '''                    Some(Control::Toggle) => spirc.play_pause().map_err(|err| format!("could not toggle playback: {err}"))?,
                    Some(Control::Next) => spirc.next().map_err(|err| format!("could not skip track: {err}"))?,
                    Some(Control::Previous) => spirc.prev().map_err(|err| format!("could not go to previous track: {err}"))?,
                    Some(Control::SetVolume(volume)) => spirc.set_volume(volume).map_err(|err| format!("could not set volume: {err}"))?,
'''
new_controls = '''                    Some(Control::Toggle) => spirc.play_pause().map_err(|err| format!("could not toggle playback: {err}"))?,
                    Some(Control::Next) => {
                        log::debug!(
                            "playback transition command trigger=next from={}",
                            current_uri.as_deref().unwrap_or("unknown")
                        );
                        transition_probe = Some(TransitionProbe::new("next", current_uri.clone()));
                        spirc.next().map_err(|err| format!("could not skip track: {err}"))?;
                    }
                    Some(Control::Previous) => {
                        log::debug!(
                            "playback transition command trigger=previous from={}",
                            current_uri.as_deref().unwrap_or("unknown")
                        );
                        transition_probe = Some(TransitionProbe::new("previous", current_uri.clone()));
                        spirc.prev().map_err(|err| format!("could not go to previous track: {err}"))?;
                    }
                    Some(Control::SetVolume(volume)) => spirc.set_volume(volume).map_err(|err| format!("could not set volume: {err}"))?,
'''
assert old_controls in text, "next/previous control anchor not found"
text = text.replace(old_controls, new_controls, 1)

old_play_uri = '''                    Some(Control::PlayUri(uri)) => {
                        spirc.load(LoadRequest::from_tracks(vec![uri], LoadRequestOptions::default()))
                            .map_err(|err| format!("could not load selected track: {err}"))?;
                        spirc.play().map_err(|err| format!("could not play selected track: {err}"))?;
                    }
'''
new_play_uri = '''                    Some(Control::PlayUri(uri)) => {
                        log::debug!(
                            "playback transition command trigger=play-uri from={} target={uri}",
                            current_uri.as_deref().unwrap_or("unknown")
                        );
                        transition_probe = Some(TransitionProbe::new("play-uri", current_uri.clone()));
                        spirc.load(LoadRequest::from_tracks(vec![uri], LoadRequestOptions::default()))
                            .map_err(|err| format!("could not load selected track: {err}"))?;
                        spirc.play().map_err(|err| format!("could not play selected track: {err}"))?;
                    }
'''
assert old_play_uri in text, "PlayUri anchor not found"
text = text.replace(old_play_uri, new_play_uri, 1)

old_playing = '''                    PlayerEvent::Playing { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Playing));
'''
new_playing = '''                    PlayerEvent::Playing { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        if let Some(probe) = transition_probe.take() {
                            log::debug!(
                                "playback transition event=playing trigger={} from={} target={} elapsed_ms={}",
                                probe.trigger,
                                probe.from_uri.as_deref().unwrap_or("unknown"),
                                uri,
                                probe.elapsed_ms()
                            );
                        }
                        if let Some(started_at) = preload_started.remove(&uri) {
                            log::debug!(
                                "playback transition preload_to_play target={} elapsed_ms={}",
                                uri,
                                started_at.elapsed().as_millis()
                            );
                        }
                        current_uri = Some(uri.clone());
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Playing));
'''
assert old_playing in text, "Playing event anchor not found"
text = text.replace(old_playing, new_playing, 1)

old_paused = '''                    PlayerEvent::Paused { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Paused));
'''
new_paused = '''                    PlayerEvent::Paused { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        current_uri = Some(uri.clone());
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Paused));
'''
assert old_paused in text, "Paused event anchor not found"
text = text.replace(old_paused, new_paused, 1)

old_stopped = '''                    PlayerEvent::Stopped { track_id, .. } => {
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Stopped));
                        let _ = updates.send(PlayerUpdate::Track { uri: track_id.to_string(), position_ms: 0 });
                    }
                    PlayerEvent::VolumeChanged { volume } => { let _ = updates.send(PlayerUpdate::Volume(volume)); }
'''
new_stopped = '''                    PlayerEvent::Stopped { track_id, .. } => {
                        let uri = track_id.to_string();
                        log::debug!("playback transition event=stopped track={uri}");
                        current_uri = Some(uri.clone());
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Stopped));
                        let _ = updates.send(PlayerUpdate::Track { uri, position_ms: 0 });
                    }
                    PlayerEvent::Preloading { track_id } => {
                        let uri = track_id.to_string();
                        preload_started.insert(uri.clone(), Instant::now());
                        log::debug!("playback transition event=preloading target={uri}");
                    }
                    PlayerEvent::TimeToPreloadNextTrack { track_id, .. } => {
                        log::debug!(
                            "playback transition event=time-to-preload current={}",
                            track_id
                        );
                    }
                    PlayerEvent::Loading { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        if let Some(probe) = transition_probe.as_ref() {
                            log::debug!(
                                "playback transition event=loading trigger={} target={} position_ms={} elapsed_ms={}",
                                probe.trigger,
                                uri,
                                position_ms,
                                probe.elapsed_ms()
                            );
                        } else {
                            log::debug!(
                                "playback transition event=loading trigger=unknown target={} position_ms={}",
                                uri,
                                position_ms
                            );
                        }
                    }
                    PlayerEvent::EndOfTrack { track_id, .. } => {
                        let uri = track_id.to_string();
                        log::debug!("playback transition event=end-of-track current={uri}");
                        if transition_probe.is_none() {
                            transition_probe = Some(TransitionProbe::new("automatic", Some(uri)));
                        }
                    }
                    PlayerEvent::Unavailable { track_id, .. } => {
                        log::debug!("playback transition event=unavailable track={}", track_id);
                    }
                    PlayerEvent::TrackChanged { audio_item } => {
                        log::debug!("playback transition event=track-changed track={}", audio_item.uri);
                    }
                    PlayerEvent::VolumeChanged { volume } => { let _ = updates.send(PlayerUpdate::Volume(volume)); }
'''
assert old_stopped in text, "Stopped event anchor not found"
text = text.replace(old_stopped, new_stopped, 1)

path.write_text(text)
