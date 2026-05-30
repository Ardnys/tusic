use std::path::{Path, PathBuf};

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    // Waiting for rodio support of: "ogg", "webm", "wma", "opus", "flac"
    "mp3", "wav", "m4a", "aac",
];

#[derive(Debug, PartialEq, Eq, Hash, Clone, Default)]
pub enum RepeatMode {
    #[default]
    None,
    All,
    One,
}

impl RepeatMode {
    pub fn next(&self) -> Self {
        match self {
            RepeatMode::None => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
}

impl Track {
    pub fn new(path: PathBuf) -> Self {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        Self {
            path,
            title,
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
        }
    }

    pub fn display_name(&self) -> String {
        let ext = self.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let ext_suffix = if !ext.is_empty() {
            format!(".{}", ext)
        } else {
            String::new()
        };

        if self.artist.is_empty() {
            format!("{}{}", self.title, ext_suffix)
        } else {
            format!("{} - {}{}", self.artist, self.title, ext_suffix)
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Playlist {
    tracks: Vec<Track>,
}

impl Playlist {
    pub fn new() -> Self {
        Self { tracks: Vec::new() }
    }

    pub fn from_tracks(tracks: Vec<Track>) -> Self {
        Self { tracks }
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn get(&self, index: usize) -> Option<&Track> {
        self.tracks.get(index)
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn push(&mut self, track: Track) -> usize {
        if let Some(idx) = self.tracks.iter().position(|t| t.path == track.path) {
            // Already exists, don't add
            return idx;
        }

        self.tracks.push(track);

        self.tracks.len() - 1
    }

    pub fn next_index(
        &self,
        current: Option<usize>,
        repeat: RepeatMode,
        shuffle: bool,
    ) -> Option<usize> {
        let len = self.tracks.len();
        if len == 0 {
            return None;
        }

        match current {
            None => Some(0),
            Some(i) if repeat == RepeatMode::One => Some(i),
            Some(cur) if shuffle => {
                // Pick uniformly among the *other* tracks so the same song
                // doesn't repeat back-to-back.
                if len == 1 {
                    return Some(0);
                }
                let mut r = rand::random_range(0..len - 1);
                if r >= cur {
                    r += 1;
                }
                Some(r)
            }
            Some(i) if i + 1 < len => Some(i + 1),
            Some(_) if repeat == RepeatMode::All => Some(0),
            Some(_) => Some(0),
        }
    }

    pub fn prev_index(&self, current: Option<usize>, repeat: RepeatMode) -> Option<usize> {
        let len = self.tracks.len();
        if len == 0 {
            return None;
        }

        match current {
            None => Some(0),
            Some(i) if i > 0 => Some(i - 1),
            Some(_) if repeat == RepeatMode::All => Some(len - 1),
            Some(_) => Some(0),
        }
    }
}

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlist_of(n: usize) -> Playlist {
        Playlist::from_tracks(
            (0..n)
                .map(|i| Track::new(format!("/tmp/{i}.mp3").into()))
                .collect(),
        )
    }

    #[test]
    fn repeat_mode_cycles() {
        assert_eq!(RepeatMode::None.next(), RepeatMode::All);
        assert_eq!(RepeatMode::All.next(), RepeatMode::One);
        assert_eq!(RepeatMode::One.next(), RepeatMode::None);
    }

    #[test]
    fn is_audio_file_checks_extension_case_insensitively() {
        assert!(is_audio_file(Path::new("song.mp3")));
        assert!(is_audio_file(Path::new("song.MP3")));
        assert!(is_audio_file(Path::new("song.M4a")));
        assert!(!is_audio_file(Path::new("song.txt")));
        assert!(!is_audio_file(Path::new("noext")));
    }

    #[test]
    fn next_index_empty_is_none() {
        let pl = playlist_of(0);
        assert_eq!(pl.next_index(None, RepeatMode::None, false), None);
        assert_eq!(pl.next_index(Some(0), RepeatMode::All, true), None);
    }

    #[test]
    fn next_index_advances_and_wraps() {
        let pl = playlist_of(3);
        assert_eq!(pl.next_index(None, RepeatMode::None, false), Some(0));
        assert_eq!(pl.next_index(Some(0), RepeatMode::None, false), Some(1));
        assert_eq!(pl.next_index(Some(2), RepeatMode::None, false), Some(0));
        assert_eq!(pl.next_index(Some(2), RepeatMode::All, false), Some(0));
    }

    #[test]
    fn next_index_repeat_one_stays() {
        let pl = playlist_of(3);
        assert_eq!(pl.next_index(Some(1), RepeatMode::One, false), Some(1));
    }

    #[test]
    fn next_index_shuffle_never_repeats_current() {
        let pl = playlist_of(5);
        for _ in 0..1000 {
            let n = pl.next_index(Some(2), RepeatMode::None, true).unwrap();
            assert!(n < 5 && n != 2, "shuffle returned {n}");
        }
        // Single-track playlist must still yield the only index.
        assert_eq!(
            playlist_of(1).next_index(Some(0), RepeatMode::None, true),
            Some(0)
        );
    }

    #[test]
    fn prev_index_behaviour() {
        let pl = playlist_of(3);
        assert_eq!(pl.prev_index(Some(2), RepeatMode::None), Some(1));
        assert_eq!(pl.prev_index(Some(0), RepeatMode::None), Some(0));
        assert_eq!(pl.prev_index(Some(0), RepeatMode::All), Some(2));
        assert_eq!(playlist_of(0).prev_index(Some(0), RepeatMode::None), None);
    }

    #[test]
    fn push_dedups_by_path() {
        let mut pl = playlist_of(0);
        let a = pl.push(Track::new("/tmp/a.mp3".into()));
        let b = pl.push(Track::new("/tmp/b.mp3".into()));
        let a_again = pl.push(Track::new("/tmp/a.mp3".into()));
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(a_again, 0);
        assert_eq!(pl.len(), 2);
    }
}
