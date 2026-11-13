// SPDX-License-Identifier: AGPL-3.0-or-later

//! Linux: speech-dispatcher, the speech service desktops share (Orca uses
//! it). `libspeechd.so.2` is loaded when read aloud first starts, so Kitab
//! runs where it isn't installed. The text goes as SSML with a mark before
//! every word; the service calls back as the voice reaches each mark, and
//! when the message ends.

use crate::{Cmd, Event, Params, Sink, Voice, ssml_with_marks, word_from_mark};
use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::Mutex;
use std::sync::mpsc::Receiver;

type Callback = extern "C" fn(msg_id: usize, client_id: usize, state: c_int);
type CallbackIm = extern "C" fn(msg_id: usize, client_id: usize, state: c_int, mark: *mut c_char);

/// The public start of libspeechd's `SPDConnection` (unchanged since 0.6):
/// the callbacks are set by writing these fields.
#[repr(C)]
struct ConnectionHead {
    mode: c_int,
    socket: c_int,
    stream: *mut c_void,
    callback_begin: Option<Callback>,
    callback_end: Option<Callback>,
    callback_cancel: Option<Callback>,
    callback_pause: Option<Callback>,
    callback_resume: Option<Callback>,
    callback_im: Option<CallbackIm>,
}

#[repr(C)]
struct SpdVoice {
    name: *const c_char,
    language: *const c_char,
    variant: *const c_char,
}

const SPD_MODE_THREADED: c_int = 1;
const SPD_TEXT: c_int = 3;
const SPD_DATA_SSML: c_int = 1;
const SPD_END: c_int = 2;
const SPD_INDEX_MARKS: c_int = 4;
const SPD_CANCEL: c_int = 8;

struct Api {
    _lib: Library,
    say: unsafe extern "C" fn(*mut c_void, c_int, *const c_char) -> c_int,
    cancel: unsafe extern "C" fn(*mut c_void) -> c_int,
    pause: unsafe extern "C" fn(*mut c_void) -> c_int,
    resume: unsafe extern "C" fn(*mut c_void) -> c_int,
    set_rate: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    set_pitch: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    set_voice: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
    list_voices: unsafe extern "C" fn(*mut c_void) -> *mut *mut SpdVoice,
    conn: *mut c_void,
}

/// The message being spoken: its id once `spd_say` has returned it (the
/// first callbacks may come sooner) and where its events go.
struct Current {
    id: Option<usize>,
    sink: Sink,
}

static CURRENT: Mutex<Option<Current>> = Mutex::new(None);

fn deliver(msg_id: usize, event: Event, last: bool) {
    let Ok(mut cur) = CURRENT.lock() else { return };
    if !cur
        .as_ref()
        .is_some_and(|c| c.id.is_none_or(|id| id == msg_id))
    {
        return;
    }
    if last {
        if let Some(c) = cur.take() {
            (c.sink)(event);
        }
    } else if let Some(c) = cur.as_ref() {
        (c.sink)(event);
    }
}

extern "C" fn on_end(msg_id: usize, _: usize, _: c_int) {
    deliver(msg_id, Event::Done, true);
}

extern "C" fn on_mark(msg_id: usize, _: usize, _: c_int, mark: *mut c_char) {
    if mark.is_null() {
        return;
    }
    // SAFETY: speech-dispatcher passes a NUL-terminated mark name.
    let mark = unsafe { CStr::from_ptr(mark) }.to_string_lossy();
    if let Some(word) = word_from_mark(&mark) {
        deliver(msg_id, word, false);
    }
}

impl Api {
    fn load() -> Result<Self, String> {
        // SAFETY: loading the system's libspeechd; the symbols are its
        // documented C API with these signatures.
        unsafe {
            let lib = Library::new("libspeechd.so.2")
                .or_else(|_| Library::new("libspeechd.so"))
                .map_err(|_| {
                    "speech-dispatcher isn't installed (install the speech-dispatcher package)"
                        .to_string()
                })?;
            let open: libloading::Symbol<
                unsafe extern "C" fn(
                    *const c_char,
                    *const c_char,
                    *const c_char,
                    c_int,
                ) -> *mut c_void,
            > = lib.get(b"spd_open\0").map_err(|e| e.to_string())?;
            let notify: libloading::Symbol<unsafe extern "C" fn(*mut c_void, c_int) -> c_int> = lib
                .get(b"spd_set_notification_on\0")
                .map_err(|e| e.to_string())?;
            let data_mode: libloading::Symbol<unsafe extern "C" fn(*mut c_void, c_int) -> c_int> =
                lib.get(b"spd_set_data_mode\0").map_err(|e| e.to_string())?;
            let name = c"kitab";
            let conn = open(
                name.as_ptr(),
                c"main".as_ptr(),
                std::ptr::null(),
                SPD_MODE_THREADED,
            );
            if conn.is_null() {
                return Err("speech-dispatcher didn't start".into());
            }
            let head = conn as *mut ConnectionHead;
            (*head).callback_end = Some(on_end);
            (*head).callback_im = Some(on_mark);
            notify(conn, SPD_END);
            notify(conn, SPD_INDEX_MARKS);
            notify(conn, SPD_CANCEL);
            data_mode(conn, SPD_DATA_SSML);
            let api = Self {
                say: *lib.get(b"spd_say\0").map_err(|e| e.to_string())?,
                cancel: *lib.get(b"spd_cancel\0").map_err(|e| e.to_string())?,
                pause: *lib.get(b"spd_pause\0").map_err(|e| e.to_string())?,
                resume: *lib.get(b"spd_resume\0").map_err(|e| e.to_string())?,
                set_rate: *lib
                    .get(b"spd_set_voice_rate\0")
                    .map_err(|e| e.to_string())?,
                set_pitch: *lib
                    .get(b"spd_set_voice_pitch\0")
                    .map_err(|e| e.to_string())?,
                set_voice: *lib
                    .get(b"spd_set_synthesis_voice\0")
                    .map_err(|e| e.to_string())?,
                list_voices: *lib
                    .get(b"spd_list_synthesis_voices\0")
                    .map_err(|e| e.to_string())?,
                conn,
                _lib: lib,
            };
            Ok(api)
        }
    }

    fn voices(&self) -> Vec<Voice> {
        let mut out = Vec::new();
        // SAFETY: a NULL-terminated array of voices from the live connection.
        // It is not freed (free_spd_voices is missing from older versions):
        // a few hundred bytes, once per Settings visit.
        unsafe {
            let list = (self.list_voices)(self.conn);
            if list.is_null() {
                return out;
            }
            let mut i = 0;
            loop {
                let v = *list.add(i);
                if v.is_null() {
                    break;
                }
                let s = |p: *const c_char| {
                    if p.is_null() {
                        String::new()
                    } else {
                        CStr::from_ptr(p).to_string_lossy().into_owned()
                    }
                };
                let name = s((*v).name);
                out.push(Voice {
                    id: name.clone(),
                    name,
                    language: s((*v).language),
                });
                i += 1;
            }
        }
        out
    }

    fn speak(&self, text: &str, voice: Option<&str>, params: Params, sink: Sink) {
        // speech-dispatcher's -100..100, 0 being the voice's own: three
        // times as fast is 100, a third as fast -100.
        let rate = (params.rate.ln() / 3f32.ln() * 100.0)
            .round()
            .clamp(-100.0, 100.0) as c_int;
        let pitch = (params.pitch.ln() / 2f32.ln() * 100.0)
            .round()
            .clamp(-100.0, 100.0) as c_int;
        let ssml = CString::new(ssml_with_marks(text)).unwrap_or_default();
        if let Ok(mut cur) = CURRENT.lock() {
            *cur = Some(Current { id: None, sink });
        }
        // SAFETY: the live connection and NUL-terminated strings.
        let id = unsafe {
            if let Some(v) = voice.and_then(|v| CString::new(v).ok()) {
                (self.set_voice)(self.conn, v.as_ptr());
            }
            (self.set_rate)(self.conn, rate);
            (self.set_pitch)(self.conn, pitch);
            (self.say)(self.conn, SPD_TEXT, ssml.as_ptr())
        };
        let Ok(mut cur) = CURRENT.lock() else { return };
        if id < 0 {
            if let Some(c) = cur.take() {
                (c.sink)(Event::Failed(
                    "speech-dispatcher didn't take the text".into(),
                ));
            }
        } else if let Some(c) = cur.as_mut() {
            c.id = Some(id as usize);
        }
    }

    fn stop(&self) {
        if let Ok(mut cur) = CURRENT.lock() {
            cur.take();
        }
        // SAFETY: the live connection.
        unsafe { (self.cancel)(self.conn) };
    }
}

pub(crate) fn run(rx: Receiver<Cmd>) {
    let mut api: Option<Result<Api, String>> = None;
    for cmd in rx {
        if let Err(e) = api.get_or_insert_with(Api::load) {
            let e = e.clone();
            match cmd {
                Cmd::Voices(tx) => {
                    let _ = tx.send(Err(e));
                }
                Cmd::Speak { sink, .. } => sink(Event::Failed(e)),
                _ => {}
            }
            // Try again next time: it may have been installed meanwhile.
            api = None;
            continue;
        }
        let Some(Ok(a)) = api.as_ref() else { continue };
        match cmd {
            Cmd::Voices(tx) => {
                let _ = tx.send(Ok(a.voices()));
            }
            Cmd::Speak {
                text,
                voice,
                params,
                sink,
            } => {
                a.stop();
                a.speak(&text, voice.as_deref(), params, sink);
            }
            // SAFETY: the live connection.
            Cmd::Pause => unsafe {
                (a.pause)(a.conn);
            },
            Cmd::Resume => unsafe {
                (a.resume)(a.conn);
            },
            Cmd::Stop => a.stop(),
        }
    }
}
