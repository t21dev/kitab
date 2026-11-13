// SPDX-License-Identifier: AGPL-3.0-or-later

//! Read aloud with the system's own voices.
//!
//! - Windows: WinRT's `SpeechSynthesizer` (the OneCore voices Narrator uses),
//!   played through a `MediaPlayer`. Word boundaries come with the audio.
//! - macOS: `AVSpeechSynthesizer`, with the voices in System Settings ›
//!   Accessibility › Spoken Content (including ones you download there).
//! - Linux: speech-dispatcher, loaded when read aloud first starts. Words
//!   are SSML marks, so they come back as the voice reaches them.
//!
//! One utterance plays at a time: the reader hands over a sentence or so,
//! hears where each word starts, and hands over the next one when this one is
//! done. Nothing runs, and no library is loaded, until the first `speak`.

use std::sync::mpsc;
use std::time::Duration;

#[cfg(target_os = "linux")]
mod linux;
// objc 0.2's msg_send! checks a `cargo-clippy` feature this crate lacks.
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod mac;
#[cfg(windows)]
mod win;

/// A voice the system offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    /// What `speak` takes to pick it.
    pub id: String,
    /// Its name as the system shows it ("Microsoft Zira", "Samantha").
    pub name: String,
    /// A BCP 47 tag where the system gives one ("en-US", "bn-IN").
    pub language: String,
}

/// How to speak. Both are multiples of the voice's own: 1.0 is as it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Speed, 0.5 (half) to 3.0 (three times as fast).
    pub rate: f32,
    /// Tone, 0.5 (lower) to 1.5 (higher).
    pub pitch: f32,
}

impl Params {
    pub const RATE_MIN: f32 = 0.5;
    pub const RATE_MAX: f32 = 3.0;
    pub const PITCH_MIN: f32 = 0.5;
    pub const PITCH_MAX: f32 = 1.5;

    /// Out-of-range or broken values pulled back into range.
    pub fn clamped(self) -> Self {
        let fix = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { 1.0 };
        Self {
            rate: fix(self.rate, Self::RATE_MIN, Self::RATE_MAX),
            pitch: fix(self.pitch, Self::PITCH_MIN, Self::PITCH_MAX),
        }
    }
}

impl Default for Params {
    fn default() -> Self {
        Self {
            rate: 1.0,
            pitch: 1.0,
        }
    }
}

/// What an utterance reports, in order: words as they are spoken, then
/// `Done` (or `Failed`). A stopped utterance reports nothing more.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The word at `start..start + len` (in `char`s of the text given to
    /// `speak`) is being spoken.
    Word { start: usize, len: usize },
    /// Spoken to the end.
    Done,
    /// The system couldn't speak it (no voice, no speech service).
    Failed(String),
}

/// Where an utterance's events go. Called from a system thread.
pub type Sink = Box<dyn Fn(Event) + Send + 'static>;

pub(crate) enum Cmd {
    Voices(mpsc::Sender<Result<Vec<Voice>, String>>),
    Speak {
        text: String,
        voice: Option<String>,
        params: Params,
        sink: Sink,
    },
    Pause,
    Resume,
    Stop,
}

/// The system's speech. Cheap to create: the backend starts on first use.
pub struct Speaker {
    tx: mpsc::Sender<Cmd>,
}

impl Default for Speaker {
    fn default() -> Self {
        Self::new()
    }
}

impl Speaker {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("kitab-speech".into())
            .spawn(move || run(rx));
        if let Err(e) = spawned {
            log::warn!("read aloud: no speech thread: {e}");
        }
        Self { tx }
    }

    /// The voices the system has. Blocks (briefly) for the answer: call it
    /// off the UI thread.
    pub fn voices(&self) -> Result<Vec<Voice>, String> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Cmd::Voices(tx))
            .map_err(|_| "Speech isn't running".to_string())?;
        rx.recv_timeout(Duration::from_secs(10))
            .map_err(|_| "The speech service didn't answer".to_string())?
    }

    /// Speak `text`, stopping whatever was being spoken. `voice` is a
    /// `Voice::id` (`None`, or one that's gone: the system's default).
    pub fn speak(&self, text: String, voice: Option<String>, params: Params, sink: Sink) {
        let _ = self.tx.send(Cmd::Speak {
            text,
            voice,
            params: params.clamped(),
            sink,
        });
    }

    pub fn pause(&self) {
        let _ = self.tx.send(Cmd::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Cmd::Resume);
    }

    /// Stop at once. The utterance reports nothing more.
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

/// The speech thread: the platform's backend takes the commands.
fn run(rx: mpsc::Receiver<Cmd>) {
    #[cfg(windows)]
    win::run(rx);
    #[cfg(target_os = "macos")]
    mac::run(rx);
    #[cfg(target_os = "linux")]
    linux::run(rx);
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    for cmd in rx {
        match cmd {
            Cmd::Voices(tx) => {
                let _ = tx.send(Err("Read aloud isn't available on this system".into()));
            }
            Cmd::Speak { sink, .. } => sink(Event::Failed(
                "Read aloud isn't available on this system".into(),
            )),
            _ => {}
        }
    }
}

/// For each UTF-16 unit of `text`, the index of the `char` it belongs to
/// (Windows and macOS count words in UTF-16), and one past the end.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub(crate) fn utf16_to_char(text: &str) -> Vec<usize> {
    let mut map = Vec::with_capacity(text.len() + 1);
    let mut n = 0;
    for (i, c) in text.chars().enumerate() {
        for _ in 0..c.len_utf16() {
            map.push(i);
        }
        n = i + 1;
    }
    map.push(n);
    map
}

/// A word at UTF-16 `start..start + len`, in `char`s.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub(crate) fn word_in_chars(map: &[usize], start: usize, len: usize) -> Option<Event> {
    let last = map.len().checked_sub(1)?;
    let a = *map.get(start.min(last))?;
    let b = *map.get((start + len).min(last))?;
    (b > a).then_some(Event::Word {
        start: a,
        len: b - a,
    })
}

/// `text` as SSML with a `<mark name="start_len"/>` before every word, so a
/// speech-dispatcher voice says where it is (Linux).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn ssml_with_marks(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2 + 16);
    out.push_str("<speak>");
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            out.push(' ');
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        out.push_str(&format!("<mark name=\"{start}_{}\"/>", i - start));
        for &c in &chars[start..i] {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                c if c.is_control() => {}
                c => out.push(c),
            }
        }
    }
    out.push_str("</speak>");
    out
}

/// A mark from `ssml_with_marks` back to its word.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn word_from_mark(mark: &str) -> Option<Event> {
    let (a, b) = mark.split_once('_')?;
    Some(Event::Word {
        start: a.parse().ok()?,
        len: b.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_become_chars() {
        // "a😀 b": the emoji is two UTF-16 units, one char.
        let map = utf16_to_char("a😀 b");
        assert_eq!(map, vec![0, 1, 1, 2, 3, 4]);
        assert_eq!(
            word_in_chars(&map, 4, 1),
            Some(Event::Word { start: 3, len: 1 })
        );
        assert_eq!(
            word_in_chars(&map, 0, 3),
            Some(Event::Word { start: 0, len: 2 })
        );
        // Past the end: clipped, and nothing for an empty word.
        assert_eq!(
            word_in_chars(&map, 4, 9),
            Some(Event::Word { start: 3, len: 1 })
        );
        assert_eq!(word_in_chars(&map, 9, 1), None);
    }

    #[test]
    fn marks_round_trip() {
        let s = ssml_with_marks("Tom & <Jerry> রামু");
        assert_eq!(
            s,
            "<speak><mark name=\"0_3\"/>Tom <mark name=\"4_1\"/>&amp; \
             <mark name=\"6_7\"/>&lt;Jerry&gt; <mark name=\"14_4\"/>রামু</speak>"
        );
        assert_eq!(
            word_from_mark("14_4"),
            Some(Event::Word { start: 14, len: 4 })
        );
        assert_eq!(word_from_mark("x"), None);
    }

    #[test]
    fn params_stay_in_range() {
        let p = Params {
            rate: 9.0,
            pitch: f32::NAN,
        }
        .clamped();
        assert_eq!(
            p,
            Params {
                rate: 3.0,
                pitch: 1.0
            }
        );
    }
}
