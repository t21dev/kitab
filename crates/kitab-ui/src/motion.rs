// SPDX-License-Identifier: AGPL-3.0-or-later

//! Opening and closing motion (DESIGN.md § Motion). Enter and exit are a fade
//! with a short slide, strong ease-out, under 220 ms. Things people trigger from
//! the keyboard dozens of times a minute (page turns, Ctrl+G, zoom) don't animate.
//! With animations off (Settings) everything appears in its final state.
//!
//! GPUI only asks for frames while an animation runs, so idle stays at 0 % CPU.

use gpui::{
    Animation, AnimationExt, AnyElement, ElementId, IntoElement, Styled, ease_out_quint, px,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Whether the window is on screen yet. Until it is (it opens cloaked, see
/// lib.rs), entrances finish at once: the window appears whole, with nothing
/// still sliding into place.
static ON_SCREEN: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_on_screen() {
    ON_SCREEN.store(true, Ordering::Relaxed);
}

/// The entrances drawn before the window showed (a handful: the tab, the
/// dock, the navigator), so they stay finished afterwards.
static OFFSCREEN: std::sync::Mutex<Vec<ElementId>> = std::sync::Mutex::new(Vec::new());

fn started_offscreen(id: &ElementId) -> bool {
    let Ok(mut ids) = OFFSCREEN.lock() else {
        return false;
    };
    if !ON_SCREEN.load(Ordering::Relaxed) {
        if !ids.contains(id) && ids.len() < 256 {
            ids.push(id.clone());
        }
        return true;
    }
    ids.contains(id)
}

/// Menus, tooltips, popovers.
pub const FAST: Duration = Duration::from_millis(140);
/// Dialogs, panels.
pub const BASE: Duration = Duration::from_millis(200);
/// Exits run about 20 % faster than entrances (DESIGN.md § Motion).
pub const EXIT_FAST: Duration = Duration::from_millis(110);
pub const EXIT_BASE: Duration = Duration::from_millis(160);
/// Between items of a list that enters together.
pub const STAGGER: Duration = Duration::from_millis(24);
/// Lists stagger at most this many items; the rest enter with the last.
pub const STAGGER_MAX: usize = 12;

/// Where an element slides in from.
#[derive(Clone, Copy)]
pub enum From {
    /// Up from below by `px`.
    Below(f32),
    /// Down from above by `px`.
    Above(f32),
    /// In from the left by `px`.
    Left(f32),
    /// Fade only.
    Fade,
}

fn offset<E: Styled>(el: E, from: From, remaining: f32) -> E {
    match from {
        From::Below(d) => el.top(px(d * remaining)),
        From::Above(d) => el.top(px(-d * remaining)),
        From::Left(d) => el.left(px(-d * remaining)),
        From::Fade => el,
    }
}

/// Fade and slide `el` in once, after `delay`. A new `id` restarts it.
/// `el` should be `relative()` (or absolute) for the slide to apply.
pub fn enter<E>(
    el: E,
    id: impl Into<ElementId>,
    on: bool,
    from: From,
    duration: Duration,
    delay: Duration,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if !on {
        return el.into_any_element();
    }
    // Drawn before the window shows: done by the time it does. (The
    // animation still exists, so it doesn't start later instead.)
    let id: ElementId = id.into();
    // An entrance that began before the window showed is over at once, for
    // good: GPUI times an animation from its first frame with whatever
    // duration it's given now, so a longer one later would resume it.
    let (duration, delay) = if started_offscreen(&id) {
        (Duration::from_millis(1), Duration::ZERO)
    } else {
        (duration, delay)
    };
    let ease = ease_out_quint();
    if delay.is_zero() {
        return el
            .with_animation(
                id,
                Animation::new(duration).with_easing(ease),
                move |el, v| offset(el.opacity(v), from, 1.0 - v),
            )
            .into_any_element();
    }
    el.with_animations(
        id,
        vec![
            Animation::new(delay),
            Animation::new(duration).with_easing(ease),
        ],
        move |el, step, v| {
            if step == 0 {
                offset(el.opacity(0.0), from, 1.0)
            } else {
                offset(el.opacity(v), from, 1.0 - v)
            }
        },
    )
    .into_any_element()
}

/// Slide `el` in from the left by `dx` without fading (tabs): it is all
/// there from the start, only moving into place. `el` should be `relative()`.
pub fn slide_in<E>(
    el: E,
    id: impl Into<ElementId>,
    on: bool,
    dx: f32,
    duration: Duration,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if !on {
        return el.into_any_element();
    }
    let id: ElementId = id.into();
    let duration = if started_offscreen(&id) {
        Duration::from_millis(1)
    } else {
        duration
    };
    el.with_animation(
        id,
        Animation::new(duration).with_easing(ease_out_quint()),
        move |el, v| el.left(px(-dx * (1.0 - v))),
    )
    .into_any_element()
}

/// The reverse of [`enter`]: fade and slide out, ending invisible. The owner
/// removes the element when `duration` has passed.
pub fn exit<E>(el: E, id: impl Into<ElementId>, from: From, duration: Duration) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    // DESIGN.md's exit curve, cubic-bezier(0.7, 0, 0.84, 0): leaves quickly once it starts.
    let ease_in = |t: f32| t * t * t;
    el.with_animation(
        id,
        Animation::new(duration).with_easing(ease_in),
        move |el, v| offset(el.opacity(1.0 - v), from, v),
    )
    .into_any_element()
}

/// The delay of item `ix` in a staggered list.
pub fn stagger(ix: usize) -> Duration {
    STAGGER * ix.min(STAGGER_MAX) as u32
}
