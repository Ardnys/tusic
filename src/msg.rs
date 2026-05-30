use crate::{playlist::Track, youtube::YoutubeTrack};

#[derive(Debug)]
pub enum Message {
    // Player controls
    Play,
    Pause,
    Next,
    Prev,
    PlayPause,

    SeekForward,
    SeekBackward,
    IncreaseVolume,
    DecreaseVolume,
    ToggleShuffle,
    CycleRepeat,

    // Playlist Controls
    RequestDeleteTrack,
    ConfirmDeleteTrack,
    CancelDeleteTrack,
    ScrollUp,
    ScrollDown,
    ScrollUpHalf,
    ScrollDownHalf,
    ScrollTop,
    ScrollBottom,

    // Key press
    Enter,
    Escape,

    ToggleHelp,
    ToggleLogs,
    ToggleActivePanel,
    ToggleYoutube,

    // Settings popup
    ToggleSettings,
    SettingsInput(char),
    SettingsBackspace,
    SettingsToggleCwd,
    SettingsNavUp,
    SettingsNavDown,
    SettingsAddDir,
    SettingsRemoveDir,
    SettingsMakePrimary,
    SettingsStartEdit,
    SettingsEditInput(char),
    SettingsEditBackspace,
    SettingsCommitEdit,
    SettingsCancelEdit,
    SettingsSave,

    // Youtube Search
    SearchInput(char),
    SearchBackspace,
    DoYoutubeSearch(String),
    YoutubeSearchResult(anyhow::Result<Vec<YoutubeTrack>>),
    YoutubeDownloadResult(anyhow::Result<Track>),
    DownloadYoutube(usize),

    LogScrollUp,
    LogScrollDown,
    LogScrollTop,
    LogScrollBottom,

    // Watch file changes (carries a human-readable description of the event):
    FileChanged(String),

    Tick,
    None,
    Quit,
}
