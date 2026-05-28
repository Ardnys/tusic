use std::collections::VecDeque;

use crate::{
    playlist::{Playlist, RepeatMode, Track},
    youtube::SearchState,
};

const MAX_LOG_MESSAGES: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ActivePanel {
    #[default]
    Playlist,
    SearchInput,
    SearchResults,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PlaybackStatus {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PlaybackState {
    pub status: PlaybackStatus,
    pub position_ms: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct UiState {
    pub scroll: usize,
    pub selected: usize,
    pub show_help: bool,
    pub show_logs: bool,
    pub show_youtube: bool,
    pub log_messages: VecDeque<String>,
    pub log_scroll: usize,
    pub log_selected: usize,
    pub playback_error: Option<String>,
    pub active_panel: ActivePanel,
    pub search_scroll: usize,
    pub search_selected: usize,
    // Visible row counts of each scrollable panel, updated every frame from the
    // real layout so scrolling stays correct at any terminal size.
    pub playlist_viewport: usize,
    pub search_viewport: usize,
    pub log_viewport: usize,
}

#[derive(Debug)]
pub struct Model {
    pub playlist: Playlist,
    pub current_index: Option<usize>,
    pub playback: PlaybackState,
    pub volume: u8,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub ui: UiState,
    pub search: SearchState,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            playlist: Playlist::new(),
            current_index: None,
            playback: PlaybackState::default(),
            volume: 100, // Volume default is 100
            repeat: RepeatMode::None,
            shuffle: false,
            ui: UiState {
                show_help: true,
                show_youtube: true,
                ..Default::default()
            },
            search: SearchState::default(),
        }
    }
}

impl Model {
    pub fn current_track(&self) -> Option<&Track> {
        self.current_index.and_then(|i| self.playlist.get(i))
    }

    pub fn set_tracks(&mut self, tracks: Vec<Track>) {
        self.playlist = Playlist::from_tracks(tracks);
    }

    pub fn add_log(&mut self, msg: &str) {
        for m in msg.split('\n') {
            self.ui.log_messages.push_back(m.to_string());
        }
        while self.ui.log_messages.len() > MAX_LOG_MESSAGES {
            self.ui.log_messages.pop_front();
        }
    }
}
