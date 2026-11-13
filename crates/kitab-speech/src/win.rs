// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows: WinRT's speech synthesizer renders an utterance to a stream with
//! word boundaries, and a `MediaPlayer` plays it. While it plays, this thread
//! wakes every 25 ms to report the words the playback position has reached.

use crate::{Cmd, Event, Params, Sink, Voice, utf16_to_char, word_in_chars};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use windows::Foundation::TypedEventHandler;
use windows::Media::Core::{MediaSource, SpeechCue};
use windows::Media::Playback::MediaPlayer;
use windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceInformation};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::{HSTRING, IInspectable, Interface};

const TICK: Duration = Duration::from_millis(25);

/// The utterance playing now.
struct Playing {
    sink: Sink,
    /// (start time in 100 ns ticks, start, length in UTF-16 units), by time.
    cues: Vec<(i64, usize, usize)>,
    next: usize,
    map: Vec<usize>,
    paused: bool,
}

struct Backend {
    synth: SpeechSynthesizer,
    player: MediaPlayer,
    ended: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    playing: Option<Playing>,
}

pub(crate) fn run(rx: Receiver<Cmd>) {
    // SAFETY: initialises COM for this thread only, which owns every WinRT
    // object below. S_FALSE (already initialised) is fine too.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let mut backend: Option<Backend> = None;
    loop {
        let wait = if backend.as_ref().is_some_and(|b| b.playing.is_some()) {
            rx.recv_timeout(TICK)
        } else {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match wait {
            Ok(cmd) => handle(&mut backend, cmd),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if let Some(b) = backend.as_mut() {
            b.tick();
        }
    }
    if let Some(b) = backend {
        let _ = b.player.Pause();
    }
}

fn handle(backend: &mut Option<Backend>, cmd: Cmd) {
    if backend.is_none() && !matches!(cmd, Cmd::Pause | Cmd::Resume | Cmd::Stop) {
        match Backend::new() {
            Ok(b) => *backend = Some(b),
            Err(e) => {
                let msg = format!("Windows speech isn't available ({e})");
                match cmd {
                    Cmd::Voices(tx) => {
                        let _ = tx.send(Err(msg));
                    }
                    Cmd::Speak { sink, .. } => sink(Event::Failed(msg)),
                    _ => {}
                }
                return;
            }
        }
    }
    let Some(b) = backend.as_mut() else { return };
    match cmd {
        Cmd::Voices(tx) => {
            let _ = tx.send(voices().map_err(|e| e.to_string()));
        }
        Cmd::Speak {
            text,
            voice,
            params,
            sink,
        } => {
            b.stop();
            if let Err(e) = b.speak(&text, voice.as_deref(), params, sink) {
                log::warn!("read aloud: {e}");
            }
        }
        Cmd::Pause => {
            if let Some(p) = b.playing.as_mut() {
                p.paused = true;
                let _ = b.player.Pause();
            }
        }
        Cmd::Resume => {
            if let Some(p) = b.playing.as_mut() {
                p.paused = false;
                let _ = b.player.Play();
            }
        }
        Cmd::Stop => b.stop(),
    }
}

fn voices() -> windows::core::Result<Vec<Voice>> {
    let all = SpeechSynthesizer::AllVoices()?;
    let mut out = Vec::new();
    for i in 0..all.Size()? {
        let v = all.GetAt(i)?;
        out.push(Voice {
            id: v.Id()?.to_string(),
            name: v.DisplayName()?.to_string(),
            language: v.Language()?.to_string(),
        });
    }
    Ok(out)
}

fn find_voice(id: &str) -> Option<VoiceInformation> {
    let all = SpeechSynthesizer::AllVoices().ok()?;
    (0..all.Size().ok()?)
        .filter_map(|i| all.GetAt(i).ok())
        .find(|v| v.Id().is_ok_and(|v| v == id))
}

impl Backend {
    fn new() -> windows::core::Result<Self> {
        let synth = SpeechSynthesizer::new()?;
        synth.Options()?.SetIncludeWordBoundaryMetadata(true)?;
        let player = MediaPlayer::new()?;
        player.SetAutoPlay(false)?;
        let ended = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let e = ended.clone();
        player.MediaEnded(&TypedEventHandler::<MediaPlayer, IInspectable>::new(
            move |_, _| {
                e.store(true, Ordering::SeqCst);
                Ok(())
            },
        ))?;
        let f = failed.clone();
        player.MediaFailed(&TypedEventHandler::new(move |_, _| {
            f.store(true, Ordering::SeqCst);
            Ok(())
        }))?;
        Ok(Self {
            synth,
            player,
            ended,
            failed,
            playing: None,
        })
    }

    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        params: Params,
        sink: Sink,
    ) -> windows::core::Result<()> {
        let attempt = (|| -> windows::core::Result<Vec<(i64, usize, usize)>> {
            if let Some(v) = voice.and_then(find_voice) {
                self.synth.SetVoice(&v)?;
            }
            let opts = self.synth.Options()?;
            opts.SetSpeakingRate(f64::from(params.rate))?;
            opts.SetAudioPitch(f64::from(params.pitch))?;
            let stream = self
                .synth
                .SynthesizeTextToStreamAsync(&HSTRING::from(text))?
                .get()?;
            let cues = word_cues(&stream).unwrap_or_default();
            let source = MediaSource::CreateFromStream(&stream, &stream.ContentType()?)?;
            self.ended.store(false, Ordering::SeqCst);
            self.failed.store(false, Ordering::SeqCst);
            self.player.SetSource(&source)?;
            self.player.Play()?;
            Ok(cues)
        })();
        match attempt {
            Ok(cues) => {
                self.playing = Some(Playing {
                    sink,
                    cues,
                    next: 0,
                    map: utf16_to_char(text),
                    paused: false,
                });
                Ok(())
            }
            Err(e) => {
                sink(Event::Failed(e.message().to_string()));
                Err(e)
            }
        }
    }

    fn stop(&mut self) {
        if self.playing.take().is_some() {
            let _ = self.player.Pause();
            let _ = self
                .player
                .SetSource(None::<&windows::Media::Playback::IMediaPlaybackSource>);
        }
    }

    /// Report the words reached, and the end.
    fn tick(&mut self) {
        let Some(p) = self.playing.as_mut() else {
            return;
        };
        if self.failed.swap(false, Ordering::SeqCst) {
            if let Some(p) = self.playing.take() {
                (p.sink)(Event::Failed("Windows couldn't play the speech".into()));
            }
            return;
        }
        if !p.paused {
            let at = self
                .player
                .PlaybackSession()
                .and_then(|s| s.Position())
                .map_or(0, |t| t.Duration);
            // The newest word reached; ones skipped over in one tick aren't
            // worth showing for 25 ms.
            let mut reached = None;
            while p.next < p.cues.len() && p.cues[p.next].0 <= at {
                reached = Some(p.cues[p.next]);
                p.next += 1;
            }
            if let Some((_, start, len)) = reached
                && let Some(word) = word_in_chars(&p.map, start, len)
            {
                (p.sink)(word);
            }
        }
        if self.ended.swap(false, Ordering::SeqCst)
            && let Some(p) = self.playing.take()
        {
            (p.sink)(Event::Done);
        }
    }
}

/// The word boundaries the synthesizer put in the stream.
fn word_cues(
    stream: &windows::Media::SpeechSynthesis::SpeechSynthesisStream,
) -> windows::core::Result<Vec<(i64, usize, usize)>> {
    let mut out = Vec::new();
    let tracks = stream.TimedMetadataTracks()?;
    for t in 0..tracks.Size()? {
        let cues = tracks.GetAt(t)?.Cues()?;
        for c in 0..cues.Size()? {
            let Ok(cue) = cues.GetAt(c)?.cast::<SpeechCue>() else {
                continue;
            };
            let (Ok(a), Ok(b)) = (cue.StartPositionInInput(), cue.EndPositionInInput()) else {
                continue;
            };
            let (a, b) = (a.Value()?, b.Value()?);
            if a < 0 || b < a {
                continue;
            }
            out.push((cue.StartTime()?.Duration, a as usize, (b - a + 1) as usize));
        }
    }
    out.sort_by_key(|c| c.0);
    Ok(out)
}
