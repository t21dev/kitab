// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tabs out of sight hold little memory. A rendered page on a large high-DPI
//! screen is tens of megabytes, and a tab kept every page it had drawn (the
//! ones on screen, the ones rendered ahead, thumbnails) for as long as it
//! stayed open: nineteen tabs reached 2 GB.
//!
//! When a tab goes out of sight it keeps only the pages it showed. The one
//! left last keeps them sharp, so going back and forth between two tabs
//! never redraws anything. Older ones keep a small soft copy of each
//! (at most [`STAND_IN_PIXELS`]): coming back shows it at once, never blank
//! paper, and the sharp page replaces it as soon as it is drawn.

use super::*;

/// The most pixels a stand-in page keeps (about 1.4 MB).
const STAND_IN_PIXELS: u64 = 350_000;

impl Reader {
    /// The workspace hid this tab. `keep_sharp`: it is the one left last.
    pub fn park(&mut self, keep_sharp: bool, cx: &mut Context<Self>) {
        // Reading aloud in a tab you can't see: paused until you're back.
        self.pause_read_aloud(cx);
        let was_parked = std::mem::replace(&mut self.parked, true);
        if !was_parked && let Some(doc) = self.doc {
            // Off the UI thread: it only queues a message per helper.
            let engine = self.engine.clone();
            cx.background_spawn(async move { engine.release_extra(doc) })
                .detach();
        }
        if self.cache.is_empty() {
            return;
        }
        let keep = self.shown_keys();
        self.cache.retain_only(&keep);
        if !keep_sharp && !self.shrinking {
            self.shrink_pages(keep, cx);
        }
        cx.emit(ReaderEvent::Released);
    }

    /// The workspace shows this tab again: its pages render as usual.
    pub fn unpark(&mut self, cx: &mut Context<Self>) {
        if self.parked {
            self.parked = false;
            cx.notify();
        }
    }

    /// Pages let go of since the last call: the caller frees their GPU
    /// memory (`Window::drop_image`), which a hidden tab can't do itself.
    pub fn take_released(&mut self) -> Vec<Arc<RenderImage>> {
        self.cache.take_evicted()
    }

    /// Every rendered page, for a tab that is closing.
    pub fn release_all(&mut self) -> Vec<Arc<RenderImage>> {
        self.cache.clear();
        self.cache.take_evicted()
    }

    /// The renderings on screen when the tab was last drawn: for each page in
    /// view, the sharp one, or else whatever it showed meanwhile.
    fn shown_keys(&self) -> Vec<PageKey> {
        let count = self.page_count() as u32;
        if count == 0 {
            return Vec::new();
        }
        let v = self.visible.load(std::sync::atomic::Ordering::Relaxed);
        let (lo, hi) = ((v & 0xFFFF_FFFF) as u32, (v >> 32) as u32);
        let hi = hi.min(count - 1);
        (lo..=hi.max(lo).min(count - 1))
            .filter_map(|page| {
                let sharp = PageKey {
                    page,
                    scale_key: scale_key(self.px_per_pt(page as usize) * self.scale_factor),
                    rotation: self.rotation,
                };
                if self.cache.contains(&sharp) {
                    Some(sharp)
                } else {
                    self.cache.newest_key_for_page(page, self.rotation)
                }
            })
            .collect()
    }

    /// Swap each big page in `keys` for a small copy, made off the UI thread.
    fn shrink_pages(&mut self, keys: Vec<PageKey>, cx: &mut Context<Self>) {
        let big: Vec<(PageKey, Arc<RenderImage>)> = keys
            .into_iter()
            .filter_map(|k| self.cache.peek(&k).map(|i| (k, i)))
            .collect();
        if big.is_empty() {
            return;
        }
        self.shrinking = true;
        let (engine, doc, generation) = (self.engine.clone(), self.doc, self.generation);
        cx.spawn(async move |this, cx| {
            let small = cx
                .background_spawn(async move {
                    let small = big
                        .into_iter()
                        .filter_map(|(key, image)| {
                            if !image.kitab_is_husk() {
                                let (small, bytes, f) =
                                    crate::cache::shrink(&image, STAND_IN_PIXELS)?;
                                return Some((key, small, bytes, f));
                            }
                            // Only on the GPU (its pixels in memory were let
                            // go once it was painted): draw the small copy.
                            let size = image.size(0);
                            let pixels = u64::from(size.width.0.max(0) as u32)
                                * u64::from(size.height.0.max(0) as u32);
                            if pixels <= STAND_IN_PIXELS {
                                return None;
                            }
                            let f = ((pixels as f64 / STAND_IN_PIXELS as f64).sqrt().ceil() as u32)
                                .max(2);
                            let req = RenderRequest {
                                page: key.page,
                                scale: key.scale_key as f32 / 64.0 / f as f32,
                                rotation: key.rotation,
                                tile: None,
                                generation,
                            };
                            let bmp = engine.render(doc?, req).ok().flatten()?;
                            let bytes = bmp.byte_size();
                            let buffer = RgbaImage::from_raw(bmp.width, bmp.height, bmp.bgra)?;
                            Some((
                                key,
                                Arc::new(RenderImage::new([Frame::new(buffer)])),
                                bytes,
                                f,
                            ))
                        })
                        .collect::<Vec<_>>();
                    // Drawing the small copies filled MuPDF's caches again.
                    if let Some(doc) = doc {
                        engine.release_extra(doc);
                    }
                    small
                })
                .await;
            let _ = this.update(cx, |r, cx| {
                r.shrinking = false;
                // Shown again meanwhile: the sharp pages are worth keeping.
                if !r.parked {
                    return;
                }
                for (key, image, bytes, f) in small {
                    // Only if the sharp page is still the one kept.
                    if !r.cache.contains(&key) {
                        continue;
                    }
                    let stand_in = PageKey {
                        scale_key: (key.scale_key / f).max(1),
                        ..key
                    };
                    r.cache.remove(&key);
                    r.cache.insert(stand_in, image, bytes);
                }
                cx.emit(ReaderEvent::Released);
            });
        })
        .detach();
    }
}
