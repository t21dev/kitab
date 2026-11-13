// SPDX-License-Identifier: AGPL-3.0-or-later

//! macOS: `AVSpeechSynthesizer`. It is driven from the main thread (where
//! AppKit apps use it, and where its delegate hears from it): this thread
//! takes the commands and hands each to the main queue.

use crate::{Cmd, Event, Params, Sink, Voice, utf16_to_char, word_in_chars};
use objc::declare::ClassDecl;
use objc::runtime::{BOOL, Class, NO, Object, Sel};
use objc::{Encode, Encoding, class, msg_send, sel, sel_impl};
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_void};
use std::sync::Mutex;
use std::sync::mpsc::Receiver;

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {}

unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSRange {
    location: usize,
    length: usize,
}

// SAFETY: NSRange is two NSUInteger fields, which is this encoding.
unsafe impl Encode for NSRange {
    fn encode() -> Encoding {
        // SAFETY: a valid Objective-C type encoding string.
        unsafe { Encoding::from_str("{_NSRange=QQ}") }
    }
}

/// The utterance being spoken: its object (to tell its callbacks from a
/// stopped one's), where its events go, and its UTF-16 to char map.
struct Current {
    utterance: usize,
    sink: Sink,
    map: Vec<usize>,
}

static CURRENT: Mutex<Option<Current>> = Mutex::new(None);

thread_local! {
    /// The synthesizer, made on the main thread on first use.
    static SYNTH: RefCell<Option<usize>> = const { RefCell::new(None) };
}

pub(crate) fn run(rx: Receiver<Cmd>) {
    for cmd in rx {
        match cmd {
            // A class method that is safe off the main thread.
            Cmd::Voices(tx) => {
                let _ = tx.send(Ok(voices()));
            }
            cmd => on_main(move || handle(cmd)),
        }
    }
}

fn on_main(f: impl FnOnce() + Send + 'static) {
    extern "C" fn trampoline(ctx: *mut c_void) {
        // SAFETY: `ctx` is the box leaked below, run exactly once.
        let f = unsafe { Box::from_raw(ctx as *mut Box<dyn FnOnce() + Send>) };
        f();
    }
    let f: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(f));
    // SAFETY: the main queue lives for the whole process; the trampoline
    // takes ownership of the context.
    unsafe {
        dispatch_async_f(
            &raw const _dispatch_main_q,
            Box::into_raw(f) as *mut c_void,
            trampoline,
        )
    };
}

fn ns_string(s: &str) -> *mut Object {
    let c = CString::new(s.replace('\0', " ")).unwrap_or_default();
    // SAFETY: a valid C string; the result is autoreleased.
    unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
}

fn rust_string(ns: *mut Object) -> String {
    if ns.is_null() {
        return String::new();
    }
    // SAFETY: `ns` is an NSString; UTF8String lives as long as it does.
    unsafe {
        let p: *const std::ffi::c_char = msg_send![ns, UTF8String];
        if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

fn voices() -> Vec<Voice> {
    let mut out = Vec::new();
    // SAFETY: speechVoices returns an NSArray of AVSpeechSynthesisVoice.
    unsafe {
        let all: *mut Object = msg_send![class!(AVSpeechSynthesisVoice), speechVoices];
        if all.is_null() {
            return out;
        }
        let n: usize = msg_send![all, count];
        for i in 0..n {
            let v: *mut Object = msg_send![all, objectAtIndex: i];
            out.push(Voice {
                id: rust_string(msg_send![v, identifier]),
                name: rust_string(msg_send![v, name]),
                language: rust_string(msg_send![v, language]),
            });
        }
    }
    out.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)));
    out
}

/// Words as they are spoken, and the end, from the synthesizer.
fn delegate_class() -> &'static Class {
    extern "C" fn will_speak(
        _: &Object,
        _: Sel,
        _synth: *mut Object,
        range: NSRange,
        utterance: *mut Object,
    ) {
        if let Ok(cur) = CURRENT.lock()
            && let Some(c) = cur.as_ref()
            && c.utterance == utterance as usize
            && let Some(word) = word_in_chars(&c.map, range.location, range.length)
        {
            (c.sink)(word);
        }
    }
    extern "C" fn did_finish(_: &Object, _: Sel, _synth: *mut Object, utterance: *mut Object) {
        let done = CURRENT.lock().ok().and_then(|mut cur| {
            if cur
                .as_ref()
                .is_some_and(|c| c.utterance == utterance as usize)
            {
                cur.take()
            } else {
                None
            }
        });
        if let Some(c) = done {
            (c.sink)(Event::Done);
        }
    }
    if let Some(c) = Class::get("KitabSpeechDelegate") {
        return c;
    }
    let Some(mut decl) = ClassDecl::new("KitabSpeechDelegate", class!(NSObject)) else {
        return class!(NSObject);
    };
    // SAFETY: the signatures match AVSpeechSynthesizerDelegate's methods.
    unsafe {
        decl.add_method(
            sel!(speechSynthesizer:willSpeakRangeOfSpeechString:utterance:),
            will_speak as extern "C" fn(&Object, Sel, *mut Object, NSRange, *mut Object),
        );
        decl.add_method(
            sel!(speechSynthesizer:didFinishSpeechUtterance:),
            did_finish as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        );
    }
    decl.register()
}

/// The synthesizer (main thread only), with its delegate.
fn synth() -> *mut Object {
    SYNTH.with(|s| {
        if let Some(p) = *s.borrow() {
            return p as *mut Object;
        }
        // SAFETY: both objects are retained for the life of the process
        // (alloc/init, never released); the delegate is a plain NSObject.
        let p = unsafe {
            let synth: *mut Object = msg_send![class!(AVSpeechSynthesizer), new];
            let delegate: *mut Object = msg_send![delegate_class(), new];
            let _: () = msg_send![synth, setDelegate: delegate];
            synth
        };
        *s.borrow_mut() = Some(p as usize);
        p
    })
}

/// Our speed (1.0 = the voice's own) to AVFoundation's 0..1, where 0.5 is
/// the default and 1.0 is very fast.
fn av_rate(rate: f32) -> f32 {
    if rate >= 1.0 {
        (0.5 + (rate - 1.0) * 0.25).min(1.0)
    } else {
        (0.5 - (1.0 - rate) * 0.3).max(0.1)
    }
}

fn handle(cmd: Cmd) {
    let s = synth();
    match cmd {
        Cmd::Voices(_) => {}
        Cmd::Speak {
            text,
            voice,
            params,
            sink,
        } => {
            stop(s);
            speak(s, &text, voice.as_deref(), params, sink);
        }
        // SAFETY (all three): `s` is the live synthesizer; 0 is
        // AVSpeechBoundaryImmediate.
        Cmd::Pause => unsafe {
            let _: BOOL = msg_send![s, pauseSpeakingAtBoundary: 0isize];
        },
        Cmd::Resume => unsafe {
            let _: BOOL = msg_send![s, continueSpeaking];
        },
        Cmd::Stop => stop(s),
    }
}

fn stop(s: *mut Object) {
    if let Ok(mut cur) = CURRENT.lock() {
        cur.take();
    }
    // SAFETY: `s` is the live synthesizer.
    unsafe {
        let speaking: BOOL = msg_send![s, isSpeaking];
        if speaking != NO {
            let _: BOOL = msg_send![s, stopSpeakingAtBoundary: 0isize];
        }
    }
}

fn speak(s: *mut Object, text: &str, voice: Option<&str>, params: Params, sink: Sink) {
    // SAFETY: AVSpeechUtterance and AVSpeechSynthesisVoice calls with live
    // objects; the utterance is retained by the synthesizer while it speaks.
    unsafe {
        let u: *mut Object =
            msg_send![class!(AVSpeechUtterance), speechUtteranceWithString: ns_string(text)];
        if u.is_null() {
            sink(Event::Failed("macOS couldn't make the speech".into()));
            return;
        }
        if let Some(id) = voice {
            let v: *mut Object =
                msg_send![class!(AVSpeechSynthesisVoice), voiceWithIdentifier: ns_string(id)];
            if !v.is_null() {
                let _: () = msg_send![u, setVoice: v];
            }
        }
        let _: () = msg_send![u, setRate: av_rate(params.rate)];
        let _: () = msg_send![u, setPitchMultiplier: params.pitch];
        if let Ok(mut cur) = CURRENT.lock() {
            *cur = Some(Current {
                utterance: u as usize,
                sink,
                map: utf16_to_char(text),
            });
        }
        let _: () = msg_send![s, speakUtterance: u];
    }
}
