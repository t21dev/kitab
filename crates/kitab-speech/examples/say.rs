// SPDX-License-Identifier: AGPL-3.0-or-later

//! Try the system's speech: `cargo run -p kitab-speech --example say -- "text"`.
//! With no text it lists the voices.

use kitab_speech::{Event, Params, Speaker};
use std::sync::mpsc;

fn main() {
    let speaker = Speaker::new();
    let text: Vec<String> = std::env::args().skip(1).collect();
    if text.is_empty() {
        match speaker.voices() {
            Ok(v) => v
                .iter()
                .for_each(|v| println!("{}  {}  {}", v.language, v.name, v.id)),
            Err(e) => println!("no voices: {e}"),
        }
        return;
    }
    let text = text.join(" ");
    let (tx, rx) = mpsc::channel();
    let started = std::time::Instant::now();
    speaker.speak(
        text.clone(),
        None,
        Params {
            rate: 1.2,
            pitch: 1.0,
        },
        Box::new(move |e| {
            let _ = tx.send(e);
        }),
    );
    let chars: Vec<char> = text.chars().collect();
    for e in rx {
        let t = started.elapsed().as_millis();
        match e {
            Event::Word { start, len } => {
                let w: String = chars[start..(start + len).min(chars.len())]
                    .iter()
                    .collect();
                println!("{t:>6} ms  word {w:?}");
            }
            Event::Done => {
                println!("{t:>6} ms  done");
                break;
            }
            Event::Failed(m) => {
                println!("failed: {m}");
                break;
            }
        }
    }
}
