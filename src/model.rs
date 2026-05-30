use std::collections::VecDeque;

use crate::{
    config::Config,
    playlist::{Playlist, RepeatMode, Track},
    youtube::SearchState,
};

const MAX_LOG_MESSAGES: usize = 1000;

/// Which field of the Settings popup is currently focused.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SettingsField {
    /// The list of configured directories (navigate, remove, set primary).
    #[default]
    DirList,
    /// The text input for adding a new directory.
    NewDir,
    /// The "use working directory" checkbox.
    UseCurrentDir,
}

/// Editable, in-progress state of the Settings popup. Committed to [`Config`]
/// only when the user saves.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SettingsState {
    /// Working copy of the scan directories. First entry is the download target.
    pub dirs: Vec<String>,
    /// Text being typed for a new directory entry.
    pub new_dir: String,
    /// Currently highlighted entry in `dirs`.
    pub selected: usize,
    /// When `Some(i)`, the entry `dirs[i]` is being edited in place using
    /// `edit_buf` as the working text.
    pub editing: Option<usize>,
    pub edit_buf: String,
    pub use_current_dir: bool,
    pub field: SettingsField,
}

impl SettingsState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            dirs: config.scan_dirs.clone(),
            new_dir: String::new(),
            selected: 0,
            editing: None,
            edit_buf: String::new(),
            use_current_dir: config.use_current_dir,
            // Start focused on the "add directory" row, which is rendered (and
            // navigated) first, above the directory list.
            field: SettingsField::NewDir,
        }
    }
}

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
    pub show_settings: bool,
    pub settings: SettingsState,
    pub log_messages: VecDeque<String>,
    pub log_scroll: usize,
    pub log_selected: usize,
    pub playback_error: Option<String>,
    /// When `Some(i)`, a "delete this track?" confirmation popup is open for
    /// playlist entry `i`. Cleared on confirm or cancel.
    pub confirm_delete: Option<usize>,
    pub active_panel: ActivePanel,
    pub search_scroll: usize,
    pub search_selected: usize,
    // Visible row counts of each scrollable panel, updated every frame from the
    // real layout so scrolling stays correct at any terminal size.
    pub playlist_viewport: usize,
    pub search_viewport: usize,
    pub log_viewport: usize,
    /// Free-running frame counter bumped every `Tick`. Drives time-based UI
    /// animations (e.g. the download skeleton shimmer) without needing wall
    /// clock access in the render layer.
    pub anim_tick: u64,
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
    pub config: Config,
}

impl Model {
    pub fn new(config: Config) -> Self {
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
            config,
        }
    }
}

impl Default for Model {
    fn default() -> Self {
        Self::new(Config::default())
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
