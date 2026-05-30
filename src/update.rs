use std::fs;
use std::time::Duration;

use crate::audio::AudioBackend;
use crate::config::Config;
use crate::model::{ActivePanel, Model, PlaybackStatus, SettingsField, SettingsState};
use crate::msg::Message;
use crate::playlist::{RepeatMode, Track};
use crate::task::Task;
use crate::watcher::Watcher;
use crate::youtube::YoutubeService;
use anyhow::Result;
use souvlaki::{MediaControls, MediaMetadata, MediaPlayback, MediaPosition};

pub fn update_media_controls(
    model: &Model,
    media_controls: &mut Option<MediaControls>,
) -> Result<()> {
    // Media controls are optional: if the platform/session didn't provide them
    // we simply skip updating.
    let Some(controls) = media_controls.as_mut() else {
        return Ok(());
    };

    // Update the currently playing song's metadata.
    match model.current_track() {
        Some(t) => controls.set_metadata(MediaMetadata {
            title: Some(&t.title),
            album: Some(&t.album),
            artist: Some(&t.artist),
            cover_url: None,
            duration: Some(Duration::from_millis(t.duration_ms)),
        })?,
        None => controls.set_metadata(MediaMetadata::default())?,
    };

    // Report playback status. Without this the OS/MPRIS layer assumes the
    // player is stopped and won't show the player or route media keys back to
    // us — this is what makes the media keys actually work.
    let progress = Some(MediaPosition(Duration::from_millis(
        model.playback.position_ms,
    )));
    let playback = match model.playback.status {
        PlaybackStatus::Playing => MediaPlayback::Playing { progress },
        PlaybackStatus::Paused => MediaPlayback::Paused { progress },
        PlaybackStatus::Stopped => MediaPlayback::Stopped,
    };
    controls.set_playback(playback)?;

    Ok(())
}

pub fn update<T: AudioBackend>(
    model: &mut Model,
    msg: Message,
    player: &mut T,
    yt_service: &YoutubeService,
    task: &Task<Message>,
    media_controls: &mut Option<MediaControls>,
    watcher: &mut Watcher,
) -> Result<Message> {
    // Logger: skip high-frequency / no-op messages to keep the log useful.
    match msg {
        Message::None
        | Message::Quit
        | Message::Tick
        | Message::ScrollUp
        | Message::ScrollDown
        | Message::ScrollUpHalf
        | Message::ScrollDownHalf
        | Message::ScrollTop
        | Message::ScrollBottom
        | Message::LogScrollUp
        | Message::LogScrollDown
        | Message::LogScrollTop
        | Message::LogScrollBottom
        | Message::SettingsInput(_)
        | Message::SettingsBackspace
        | Message::SettingsToggleCwd
        | Message::SettingsNavUp
        | Message::SettingsNavDown
        | Message::SettingsEditInput(_)
        | Message::SettingsEditBackspace => {}
        _ => model.add_log(&format!("{msg:?}")),
    }

    match msg {
        Message::Play => {
            // duration_ms == 0 means the duration is unknown; still allow playback.
            if model.playback.status != PlaybackStatus::Playing
                && (model.playback.duration_ms == 0
                    || model.playback.position_ms < model.playback.duration_ms)
            {
                player.play()?;
                model.playback.status = PlaybackStatus::Playing;
                update_media_controls(model, media_controls)?;
            }
        }

        Message::Pause => {
            if model.playback.status == PlaybackStatus::Playing {
                player.pause();
                model.playback.position_ms = player.get_position().saturating_sub(100);
                model.playback.status = PlaybackStatus::Paused;
                update_media_controls(model, media_controls)?;
            }
        }

        Message::PlayPause => {
            if player.is_playing() {
                return Ok(Message::Pause);
            } else {
                return Ok(Message::Play);
            }
        }

        Message::Next => {
            let next_idx =
                model
                    .playlist
                    .next_index(model.current_index, model.repeat.clone(), model.shuffle);

            if let Some(idx) = next_idx {
                play_track(idx, model, player, media_controls);
                update_media_controls(model, media_controls)?;
            }
        }

        Message::Prev => {
            let prev_idx = model
                .playlist
                .prev_index(model.current_index, model.repeat.clone());

            if let Some(idx) = prev_idx {
                play_track(idx, model, player, media_controls);
                update_media_controls(model, media_controls)?;
            }
        }

        Message::SeekForward => {
            let new_pos = model.playback.position_ms + 5000;
            let total_duration = model.playback.duration_ms;

            // if is there a song loaded, total_duration should be higher than 100
            if total_duration > 100 {
                let target = new_pos.clamp(0, total_duration.saturating_sub(100));

                if let Err(e) = player.seek_to(target) {
                    model.add_log(&format!("Seek error: {}", e));
                }

                model.add_log(&format!("player.is_paused:{}", player.is_playing()));

                model.playback.position_ms = target;
                model.add_log(&format!("Seek to: {}", target));
            }
        }

        Message::SeekBackward => {
            let new_pos = model.playback.position_ms.saturating_sub(5000);

            if let Err(e) = player.seek_to(new_pos) {
                model.add_log(&format!("Seek error: {}", e));
            }

            model.playback.position_ms = new_pos;
            model.add_log(&format!("Seek to: {}", new_pos));
        }

        Message::IncreaseVolume => {
            let vol = model.volume.saturating_add(5).min(100);
            player.set_volume(vol);
            model.volume = vol;
        }
        Message::DecreaseVolume => {
            let vol = model.volume.saturating_sub(5);
            player.set_volume(vol);
            model.volume = vol;
        }

        Message::ToggleShuffle => {
            model.shuffle = !model.shuffle;
        }

        Message::CycleRepeat => {
            model.repeat = model.repeat.next();
        }

        Message::ScrollUp => {
            match model.ui.active_panel {
                ActivePanel::Playlist => {
                    if model.ui.selected > 0 {
                        model.ui.selected -= 1;
                        adjust_scroll(model);
                    }
                }
                ActivePanel::SearchInput => {
                    // Do nothing - navigation only in search results
                }
                ActivePanel::SearchResults => {
                    if model.ui.search_selected > 0 {
                        model.ui.search_selected -= 1;
                        adjust_search_scroll(model);
                    }
                }
            }
        }

        Message::ScrollDown => {
            match model.ui.active_panel {
                ActivePanel::Playlist => {
                    if model.ui.selected < model.playlist.len().saturating_sub(1) {
                        model.ui.selected += 1;
                        adjust_scroll(model);
                    }
                }
                ActivePanel::SearchInput => {
                    // Do nothing - navigation only in search results
                }
                ActivePanel::SearchResults => {
                    if model.ui.search_selected < model.search.results.len().saturating_sub(1) {
                        model.ui.search_selected += 1;
                        adjust_search_scroll(model);
                    }
                }
            }
        }

        Message::ScrollUpHalf => {
            match model.ui.active_panel {
                ActivePanel::Playlist => {
                    let half = (model.ui.playlist_viewport / 2).max(1);
                    model.ui.selected = model.ui.selected.saturating_sub(half);
                    adjust_scroll(model);
                }
                ActivePanel::SearchInput => {
                    // Do nothing
                }
                ActivePanel::SearchResults => {
                    let half = (model.ui.search_viewport / 2).max(1);
                    model.ui.search_selected = model.ui.search_selected.saturating_sub(half);
                    adjust_search_scroll(model);
                }
            }
        }

        Message::ScrollDownHalf => {
            match model.ui.active_panel {
                ActivePanel::Playlist => {
                    let half = (model.ui.playlist_viewport / 2).max(1);
                    model.ui.selected =
                        (model.ui.selected + half).min(model.playlist.len().saturating_sub(1));
                    adjust_scroll(model);
                }
                ActivePanel::SearchInput => {
                    // Do nothing
                }
                ActivePanel::SearchResults => {
                    let half = (model.ui.search_viewport / 2).max(1);
                    model.ui.search_selected = (model.ui.search_selected + half)
                        .min(model.search.results.len().saturating_sub(1));
                    adjust_search_scroll(model);
                }
            }
        }

        Message::ScrollTop => {
            match model.ui.active_panel {
                ActivePanel::Playlist => {
                    model.ui.selected = 0;
                    model.ui.scroll = 0;
                }
                ActivePanel::SearchInput => {
                    // Do nothing
                }
                ActivePanel::SearchResults => {
                    model.ui.search_selected = 0;
                    model.ui.search_scroll = 0;
                }
            }
        }

        Message::ScrollBottom => {
            match model.ui.active_panel {
                ActivePanel::Playlist => {
                    let list_len = model.playlist.len();
                    model.ui.selected = list_len.saturating_sub(1);
                    model.ui.scroll = model
                        .ui
                        .selected
                        .saturating_sub(model.ui.playlist_viewport.saturating_sub(1));
                }
                ActivePanel::SearchInput => {
                    // Do nothing
                }
                ActivePanel::SearchResults => {
                    let results_len = model.search.results.len();
                    model.ui.search_selected = results_len.saturating_sub(1);
                    model.ui.search_scroll = model
                        .ui
                        .search_selected
                        .saturating_sub(model.ui.search_viewport.saturating_sub(1));
                }
            }
        }

        Message::Enter => {
            if matches!(model.ui.active_panel, ActivePanel::SearchInput) {
                if !model.search.query.is_empty() {
                    model.search.is_loading = true;
                }
            } else if matches!(model.ui.active_panel, ActivePanel::Playlist) {
                play_track(model.ui.selected, model, player, media_controls);
            }
        }

        Message::Escape => {
            if matches!(
                model.ui.active_panel,
                ActivePanel::SearchInput | ActivePanel::SearchResults
            ) {
                model.ui.active_panel = ActivePanel::Playlist;
            } else {
                model.ui.selected = model.current_index.unwrap_or(0);
            }
        }

        Message::RequestDeleteTrack => {
            // Only the playlist supports deletion, and only when it has a valid
            // selection. Opens the confirmation popup; nothing is deleted yet.
            if matches!(model.ui.active_panel, ActivePanel::Playlist)
                && model.ui.selected < model.playlist.len()
            {
                model.ui.confirm_delete = Some(model.ui.selected);
            }
        }

        Message::CancelDeleteTrack => {
            model.ui.confirm_delete = None;
        }

        Message::ConfirmDeleteTrack => {
            if let Some(idx) = model.ui.confirm_delete.take() {
                if let Some(track) = model.playlist.get(idx) {
                    let path = track.path.clone();
                    let was_current = model.current_index == Some(idx);
                    match std::fs::remove_file(&path) {
                        Ok(_) => {
                            model.add_log(&format!("Deleted: {}", path.display()));
                            // If the deleted track was playing, stop playback.
                            if was_current {
                                player.stop();
                                model.playback.status = PlaybackStatus::Stopped;
                                model.current_index = None;
                            }
                            // Reload the library from disk so indices stay valid.
                            let playlist = read_tracks(&model.config);
                            model.set_tracks(playlist);
                            model.ui.selected = model
                                .ui
                                .selected
                                .min(model.playlist.len().saturating_sub(1));
                        }
                        Err(e) => model.add_log(&format!("Delete failed: {e}")),
                    }
                }
            }
        }

        Message::ToggleHelp => {
            model.ui.show_help = !model.ui.show_help;
        }

        Message::ToggleLogs => {
            model.ui.show_logs = !model.ui.show_logs;
        }

        Message::ToggleYoutube => {
            model.ui.show_youtube = !model.ui.show_youtube;
            if !model.ui.show_youtube {
                model.ui.active_panel = ActivePanel::Playlist;
            }
        }

        Message::ToggleActivePanel => {
            if model.ui.show_youtube {
                match model.ui.active_panel {
                    ActivePanel::Playlist => model.ui.active_panel = ActivePanel::SearchInput,
                    ActivePanel::SearchInput => model.ui.active_panel = ActivePanel::SearchResults,
                    ActivePanel::SearchResults => model.ui.active_panel = ActivePanel::Playlist,
                }
            } else {
                model.ui.active_panel = ActivePanel::Playlist;
            }
        }

        Message::SearchInput(c) => {
            model.search.query.push(c);
        }

        Message::SearchBackspace => {
            model.search.query.pop();
        }

        Message::DoYoutubeSearch(query) => {
            if !query.is_empty() {
                model.search.is_loading = true;
                model.search.error = None;
                model.add_log(&format!("Searching YouTube: {}", &query));
                model.ui.active_panel = ActivePanel::SearchInput;
                // Restart the loading animation from its first frame.
                model.ui.anim_tick = 0;

                let service = yt_service.clone();

                task.spawn(async move {
                    let result = service.search(&query, 10).await;

                    Message::YoutubeSearchResult(result)
                });
            }
        }
        Message::YoutubeSearchResult(result) => {
            model.search.is_loading = false;

            match result {
                Ok(tracks) => {
                    model.search.error = None;
                    model.search.results = tracks;
                    model.ui.active_panel = ActivePanel::SearchResults;

                    model.add_log(&format!("Found {} tracks", model.search.results.len()));
                }
                Err(e) => {
                    model.add_log(&format!("Search error: {e:?}"));
                    model.search.results.clear();
                    model.search.error = Some(
                        "YouTube araması yapılamadı. İnternet bağlantınızı kontrol edin."
                            .to_string(),
                    );
                    model.ui.active_panel = ActivePanel::SearchResults;
                }
            }
        }

        Message::DownloadYoutube(idx) => {
            if idx < model.search.results.len() {
                if model.search.is_downloading {
                    model.add_log("Already downloading, please wait...");
                    return Ok(Message::None);
                }

                model.search.is_downloading = true;
                // Restart the loading animation from its first frame.
                model.ui.anim_tick = 0;

                let track = model.search.results[idx].clone();
                let track_title = track.title.clone();

                let service = yt_service.clone();
                let dir = model.config.download_dir();

                task.spawn(async move {
                    let result = service.download_track(&track, &dir).await;

                    Message::YoutubeDownloadResult(result)
                });

                model.add_log(&format!("Downloading: {}", track_title));
            }
        }

        Message::YoutubeDownloadResult(result) => {
            model.search.is_downloading = false;

            match result {
                Ok(track) => {
                    let idx = model.playlist.push(track.clone());

                    model.current_index = Some(idx);
                    model.ui.active_panel = ActivePanel::Playlist;
                    play_track(idx, model, player, media_controls);

                    model.add_log(&format!("Downloaded: {}", track.display_name()));
                }
                Err(e) => model.add_log(&format!("Download error: {e}")),
            }
        }

        Message::LogScrollUp => {
            if model.ui.show_logs && model.ui.log_selected > 0 {
                model.ui.log_selected -= 1;
                adjust_log_scroll(model);
            }
        }

        Message::LogScrollDown => {
            if model.ui.show_logs {
                let log_count = model.ui.log_messages.len();
                if model.ui.log_selected < log_count.saturating_sub(1) {
                    model.ui.log_selected += 1;
                    adjust_log_scroll(model);
                }
            }
        }

        Message::LogScrollTop => {
            if model.ui.show_logs {
                model.ui.log_selected = 0;
                model.ui.log_scroll = 0;
            }
        }

        Message::LogScrollBottom => {
            if model.ui.show_logs {
                model.ui.log_selected = model.ui.log_messages.len().saturating_sub(1);
                adjust_log_scroll(model);
            }
        }

        Message::FileChanged(e) => {
            model.add_log(&e);

            let playlist = read_tracks(&model.config);
            model.set_tracks(playlist);
        }

        Message::ToggleSettings => {
            model.ui.show_settings = !model.ui.show_settings;
            if model.ui.show_settings {
                // Load current config into the editable popup state.
                model.ui.settings = SettingsState::from_config(&model.config);
            } else {
                model.ui.active_panel = ActivePanel::Playlist;
            }
        }

        Message::SettingsInput(c) => {
            if model.ui.settings.field == SettingsField::NewDir {
                model.ui.settings.new_dir.push(c);
            }
        }

        Message::SettingsBackspace => {
            if model.ui.settings.field == SettingsField::NewDir {
                model.ui.settings.new_dir.pop();
            }
        }

        Message::SettingsToggleCwd => {
            model.ui.settings.use_current_dir = !model.ui.settings.use_current_dir;
        }

        // Arrow navigation flows across the whole popup in the same order the
        // fields are rendered: the "add" input (top), then the directory list,
        // then the checkbox (and back up).
        Message::SettingsNavDown => {
            let s = &mut model.ui.settings;
            match s.field {
                SettingsField::NewDir => {
                    if s.dirs.is_empty() {
                        s.field = SettingsField::UseCurrentDir;
                    } else {
                        s.field = SettingsField::DirList;
                        s.selected = 0;
                    }
                }
                SettingsField::DirList => {
                    if s.selected + 1 < s.dirs.len() {
                        s.selected += 1;
                    } else {
                        s.field = SettingsField::UseCurrentDir;
                    }
                }
                SettingsField::UseCurrentDir => {}
            }
        }

        Message::SettingsNavUp => {
            let s = &mut model.ui.settings;
            match s.field {
                // The "add" input is the first field; nothing above it.
                SettingsField::NewDir => {}
                SettingsField::DirList => {
                    if s.selected > 0 {
                        s.selected -= 1;
                    } else {
                        s.field = SettingsField::NewDir;
                    }
                }
                SettingsField::UseCurrentDir => {
                    if s.dirs.is_empty() {
                        s.field = SettingsField::NewDir;
                    } else {
                        s.field = SettingsField::DirList;
                        s.selected = s.dirs.len().saturating_sub(1);
                    }
                }
            }
        }

        Message::SettingsAddDir => {
            let s = &mut model.ui.settings;
            let new = s.new_dir.trim().to_string();
            if !new.is_empty() && !s.dirs.contains(&new) {
                s.dirs.push(new);
                s.selected = s.dirs.len() - 1;
            }
            s.new_dir.clear();
        }

        Message::SettingsRemoveDir => {
            let s = &mut model.ui.settings;
            if s.selected < s.dirs.len() {
                s.dirs.remove(s.selected);
                s.selected = s.selected.min(s.dirs.len().saturating_sub(1));
            }
        }

        Message::SettingsMakePrimary => {
            let s = &mut model.ui.settings;
            if s.selected < s.dirs.len() && s.selected > 0 {
                let dir = s.dirs.remove(s.selected);
                s.dirs.insert(0, dir);
                s.selected = 0;
            }
        }

        Message::SettingsStartEdit => {
            let s = &mut model.ui.settings;
            if s.selected < s.dirs.len() {
                s.editing = Some(s.selected);
                s.edit_buf = s.dirs[s.selected].clone();
            }
        }

        Message::SettingsEditInput(c) => {
            if model.ui.settings.editing.is_some() {
                model.ui.settings.edit_buf.push(c);
            }
        }

        Message::SettingsEditBackspace => {
            if model.ui.settings.editing.is_some() {
                model.ui.settings.edit_buf.pop();
            }
        }

        Message::SettingsCommitEdit => {
            let s = &mut model.ui.settings;
            if let Some(i) = s.editing.take() {
                let value = s.edit_buf.trim().to_string();
                if value.is_empty() {
                    // Empty value removes the entry.
                    if i < s.dirs.len() {
                        s.dirs.remove(i);
                        s.selected = s.selected.min(s.dirs.len().saturating_sub(1));
                    }
                } else if i < s.dirs.len() {
                    s.dirs[i] = value;
                }
                s.edit_buf.clear();
            }
        }

        Message::SettingsCancelEdit => {
            let s = &mut model.ui.settings;
            s.editing = None;
            s.edit_buf.clear();
        }

        Message::SettingsSave => {
            model.config.scan_dirs = model.ui.settings.dirs.clone();
            model.config.use_current_dir = model.ui.settings.use_current_dir;

            if let Err(e) = model.config.save() {
                model.add_log(&format!("Failed to save config: {e}"));
            }

            let dirs = model.config.resolved_dirs();
            model.add_log(&format!("Scan directories: {dirs:?}"));

            // Re-point the file watcher and reload the library immediately.
            if let Err(e) = watcher.set_paths(dirs) {
                model.add_log(&format!("Failed to update watcher: {e}"));
            }
            let playlist = read_tracks(&model.config);
            model.set_tracks(playlist);

            // Selection may now be out of range; clamp it.
            model.ui.selected = model
                .ui
                .selected
                .min(model.playlist.len().saturating_sub(1));

            model.ui.show_settings = false;
            model.ui.active_panel = ActivePanel::Playlist;
        }

        Message::Tick => {
            // Advance the UI animation clock (~25 fps given the 40ms poll). This
            // is what makes the download skeleton shimmer move.
            model.ui.anim_tick = model.ui.anim_tick.wrapping_add(1);

            if model.playback.status == PlaybackStatus::Playing {
                let pos = player.get_position();
                model.playback.position_ms = pos;

                // let max_pos = model.playback.duration_ms;
                // model.add_log(&format!("Player pos : {pos} / {max_pos}"));

                if model.playback.position_ms >= model.playback.duration_ms.saturating_sub(100) {
                    if model.repeat == RepeatMode::One {
                        model.add_log("Loop mode: restarting same track");
                        play_track(
                            model.current_index.unwrap_or(0),
                            model,
                            player,
                            media_controls,
                        );
                    } else {
                        model.add_log("Song ended, playing next track");
                        let next_idx = model.playlist.next_index(
                            model.current_index,
                            model.repeat.clone(),
                            model.shuffle,
                        );
                        if let Some(idx) = next_idx {
                            play_track(idx, model, player, media_controls);
                        } else {
                            model.playback.status = PlaybackStatus::Paused;
                            player.pause();
                        }
                    }
                }
            }
        }

        Message::Quit => {}
        Message::None => {}
    }

    Ok(Message::None)
}

fn play_track<T: AudioBackend>(
    idx: usize,
    model: &mut Model,
    player: &mut T,
    media_controls: &mut Option<MediaControls>,
) {
    let track = match model.playlist.get(idx) {
        Some(t) => t.clone(),
        None => return,
    };

    model.current_index = Some(idx);
    model.ui.playback_error = None;
    model.playback.position_ms = 0;

    match player.load_track(&track.path) {
        Ok(_) => (),
        Err(e) => {
            let err_msg = format!("Failed to load track: {}", e);
            model.add_log(&err_msg);
            model.ui.playback_error = Some(err_msg);

            player.stop();
            model.playback.status = PlaybackStatus::Stopped;
            let _ = update_media_controls(model, media_controls);

            return;
        }
    }

    model.playback.duration_ms = player.get_duration();
    player.set_volume(model.volume);

    match player.play() {
        Ok(_) => {
            model.playback.status = PlaybackStatus::Playing;
        }
        Err(e) => {
            let err_msg = format!("Failed to play: {}", e);
            model.add_log(&err_msg);
            model.ui.playback_error = Some(err_msg);

            player.stop();
            model.playback.status = PlaybackStatus::Stopped;
        }
    }

    // Push the new track + status to the OS media controls (lock screen,
    // media keys). Best-effort: ignore failures so playback isn't affected.
    let _ = update_media_controls(model, media_controls);
}

/// Keep `scroll` such that `selected` stays within a window of `viewport` rows.
fn keep_in_view(selected: usize, scroll: &mut usize, viewport: usize) {
    let v = viewport.max(1);
    if selected >= *scroll + v {
        *scroll = selected + 1 - v;
    } else if selected < *scroll {
        *scroll = selected;
    }
}

fn adjust_scroll(model: &mut Model) {
    let v = model.ui.playlist_viewport;
    keep_in_view(model.ui.selected, &mut model.ui.scroll, v);
}

fn adjust_search_scroll(model: &mut Model) {
    let v = model.ui.search_viewport;
    keep_in_view(model.ui.search_selected, &mut model.ui.search_scroll, v);
}

fn adjust_log_scroll(model: &mut Model) {
    let v = model.ui.log_viewport;
    keep_in_view(model.ui.log_selected, &mut model.ui.log_scroll, v);
}

pub fn read_tracks(config: &Config) -> Vec<Track> {
    use crate::playlist::is_audio_file;

    let mut tracks = Vec::new();

    // Scan every configured directory; skip ones that can't be read.
    for dir in config.resolved_dirs() {
        let Ok(read_dir) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.filter_map(|e| e.ok()) {
            let path = entry.path();
            if is_audio_file(&path) && !tracks.iter().any(|t: &Track| t.path == path) {
                tracks.push(crate::playlist::Track::new(path));
            }
        }
    }

    tracks.sort_by(|a, b| {
        a.display_name()
            .to_lowercase()
            .cmp(&b.display_name().to_lowercase())
    });

    tracks
}
