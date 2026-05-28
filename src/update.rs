use std::fs;
use std::time::Duration;

use crate::audio::AudioBackend;
use crate::model::{ActivePanel, Model, PlaybackStatus};
use crate::msg::Message;
use crate::playlist::{RepeatMode, Track};
use crate::task::Task;
use crate::youtube::{get_scan_paths, YoutubeService};
use anyhow::Result;
use souvlaki::{MediaControls, MediaMetadata};

pub fn update_media_controls(
    track: Option<&Track>,
    media_controls: &mut Option<MediaControls>,
) -> Result<()> {
    // Media controls are optional: if the platform/session didn't provide them
    // we simply skip updating metadata.
    let Some(controls) = media_controls.as_mut() else {
        return Ok(());
    };

    // Update current playing song:
    match track {
        Some(t) => controls.set_metadata(MediaMetadata {
            title: Some(&t.title),
            album: Some(&t.album),
            artist: Some(&t.artist),
            cover_url: None,
            duration: Some(Duration::from_millis(t.duration_ms)),
        })?,
        None => controls.set_metadata(MediaMetadata::default())?,
    };

    Ok(())
}

pub fn update<T: AudioBackend>(
    model: &mut Model,
    msg: Message,
    player: &mut T,
    yt_service: &YoutubeService,
    task: &Task<Message>,
    media_controls: &mut Option<MediaControls>,
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
        | Message::LogScrollBottom => {}
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
                update_media_controls(model.current_track(), media_controls)?;
            }
        }

        Message::Pause => {
            if model.playback.status == PlaybackStatus::Playing {
                player.pause();
                model.playback.position_ms = player.get_position().saturating_sub(100);
                model.playback.status = PlaybackStatus::Paused;
                update_media_controls(model.current_track(), media_controls)?;
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
                play_track(idx, model, player);
                update_media_controls(model.current_track(), media_controls)?;
            }
        }

        Message::Prev => {
            let prev_idx = model
                .playlist
                .prev_index(model.current_index, model.repeat.clone());

            if let Some(idx) = prev_idx {
                play_track(idx, model, player);
                update_media_controls(model.current_track(), media_controls)?;
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
                play_track(model.ui.selected, model, player);
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
                model.add_log(&format!("Searching YouTube: {}", &query));
                model.ui.active_panel = ActivePanel::SearchInput;

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
                    model.search.results = tracks;
                    model.ui.active_panel = ActivePanel::SearchResults;

                    model.add_log(&format!("Found {} tracks", model.search.results.len()));
                }
                Err(e) => model.add_log(&format!("Search error: {e:?}")),
            }
        }

        Message::DownloadYoutube(idx) => {
            if idx < model.search.results.len() {
                if model.search.is_downloading {
                    model.add_log("Already downloading, please wait...");
                    return Ok(Message::None);
                }

                model.search.is_downloading = true;

                let track = model.search.results[idx].clone();
                let track_title = track.title.clone();

                let service = yt_service.clone();

                task.spawn(async move {
                    let result = service.download_track(&track).await;

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
                    play_track(idx, model, player);

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
            model.add_log(&format!("{e:?}"));

            let playlist = read_tracks();
            model.set_tracks(playlist);
        }

        Message::Tick => {
            if model.playback.status == PlaybackStatus::Playing {
                let pos = player.get_position();
                model.playback.position_ms = pos;

                // let max_pos = model.playback.duration_ms;
                // model.add_log(&format!("Player pos : {pos} / {max_pos}"));

                if model.playback.position_ms >= model.playback.duration_ms.saturating_sub(100) {
                    if model.repeat == RepeatMode::One {
                        model.add_log("Loop mode: restarting same track");
                        play_track(model.current_index.unwrap_or(0), model, player);
                    } else {
                        model.add_log("Song ended, playing next track");
                        let next_idx = model.playlist.next_index(
                            model.current_index,
                            model.repeat.clone(),
                            model.shuffle,
                        );
                        if let Some(idx) = next_idx {
                            play_track(idx, model, player);
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

fn play_track<T: AudioBackend>(idx: usize, model: &mut Model, player: &mut T) {
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

pub fn read_tracks() -> Vec<Track> {
    use crate::playlist::is_audio_file;

    let mut tracks = Vec::new();

    let paths = get_scan_paths();

    for p in paths {
        let Ok(read_dir) = fs::read_dir(&p) else {
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
