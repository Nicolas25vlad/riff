use std::path::PathBuf;

use ratatui::layout::Rect;

use super::{git_context::GitContext, theme::Theme};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub version: Option<String>,
    pub uri: String,
    pub duration_ms: u32,
    pub cover_id: Option<String>,
    pub match_score: Option<u8>,
}

impl QueueItem {
    pub fn label(&self) -> String {
        if self.artist.trim().is_empty() {
            self.title.clone()
        } else {
            format!("{} - {}", self.artist, self.title)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Starting,
    Playing,
    Paused,
    Stopped,
}

impl PlaybackStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::Starting => "…",
            Self::Playing => "▶",
            Self::Paused => "Ⅱ",
            Self::Stopped => "■",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    NowPlaying,
    Search,
    Playlist,
    Lyrics,
    Editor,
}

impl View {
    pub const ALL: [Self; 5] = [
        Self::NowPlaying,
        Self::Search,
        Self::Playlist,
        Self::Lyrics,
        Self::Editor,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::NowPlaying => "Now Playing",
            Self::Search => "Search",
            Self::Playlist => "Playlist",
            Self::Lyrics => "Lyrics",
            Self::Editor => "Editor",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|view| *view == self).unwrap_or(0)
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        let index = if self.index() == 0 {
            Self::ALL.len() - 1
        } else {
            self.index() - 1
        };
        Self::ALL[index]
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub submitted_query: String,
    pub results: Vec<QueueItem>,
    pub selected: usize,
    pub searching: bool,
    pub error: Option<String>,
}

impl SearchState {
    pub fn selected(&self) -> Option<&QueueItem> {
        self.results.get(self.selected)
    }

    pub fn move_down(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsLine {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct LyricsState {
    pub track_uri: Option<String>,
    pub provider: Option<String>,
    pub lines: Vec<LyricsLine>,
    pub loading: bool,
    pub error: Option<String>,
}

impl LyricsState {
    pub fn active_line(&self, position_ms: u32) -> Option<usize> {
        self.lines.iter().enumerate().rev().find_map(|(index, line)| {
            (position_ms >= line.start_ms && (line.end_ms == 0 || position_ms < line.end_ms))
                .then_some(index)
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct HitMap {
    pub tabs: Vec<(View, Rect)>,
    pub progress: Option<Rect>,
    pub volume: Option<Rect>,
    pub previous: Option<Rect>,
    pub toggle: Option<Rect>,
    pub next: Option<Rect>,
    pub search_rows: Vec<(usize, Rect)>,
}

#[derive(Debug)]
pub struct AppState {
    pub file_path: PathBuf,
    pub file_name: String,
    pub playlist_name: String,
    pub queue: Vec<QueueItem>,
    pub status: PlaybackStatus,
    pub current_uri: Option<String>,
    pub position_ms: u32,
    pub volume: u16,
    pub shuffle: bool,
    pub repeat: bool,
    pub message: String,
    pub view: View,
    pub theme: Theme,
    pub git: Option<GitContext>,
    pub search: SearchState,
    pub lyrics: LyricsState,
    pub hits: HitMap,
}

impl AppState {
    pub fn current_index(&self) -> Option<usize> {
        let uri = self.current_uri.as_deref()?;
        self.queue.iter().position(|item| item.uri == uri)
    }

    pub fn current(&self) -> Option<&QueueItem> {
        self.current_index().and_then(|index| self.queue.get(index))
    }

    pub fn duration_ms(&self) -> u32 {
        self.current().map(|item| item.duration_ms).unwrap_or(0)
    }
}
