// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deep zoom: a page too large to draw whole (maps, scans at 500 % on a
//! high-DPI screen) shows a softer whole-page picture, and on top of it only
//! the parts in view, drawn sharp in tiles. The tiles have their own small
//! cache; the soft picture underneath stays while they arrive, so nothing
//! flickers.

use super::*;
use kitab_core::TilePx;
use std::collections::{HashMap, HashSet, VecDeque};

/// A page larger than this (device pixels) is drawn in tiles.
pub(super) const DEEP_PIXELS: f32 = 40.0e6;
/// The whole-page picture under the tiles is at most this large.
pub(super) const STAND_IN_PIXELS: f32 = 16.0e6;
/// Tile side in device pixels.
const TILE: u32 = 512;
/// Tiles kept (1 MB each).
const MAX_TILES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey {
    page: u32,
    scale_key: u32,
    rotation: u16,
    generation: u64,
    tx: u32,
    ty: u32,
}

#[derive(Default)]
pub(super) struct Tiles {
    /// Each page's part in view last frame, in its own logical pixels
    /// (x0, y0, x1, y1): worked out before the list draws, which can't be
    /// asked where things are while it draws them.
    pub(super) seen: HashMap<usize, (f32, f32, f32, f32)>,
    map: HashMap<TileKey, Arc<RenderImage>>,
    order: VecDeque<TileKey>,
    pending: HashSet<TileKey>,
    evicted: Vec<Arc<RenderImage>>,
}

impl Tiles {
    fn insert(&mut self, key: TileKey, image: Arc<RenderImage>) {
        if let Some(old) = self.map.insert(key, image) {
            self.evicted.push(old);
        } else {
            self.order.push_back(key);
        }
        while self.order.len() > MAX_TILES {
            if let Some(k) = self.order.pop_front()
                && let Some(old) = self.map.remove(&k)
            {
                self.evicted.push(old);
            }
        }
    }

    /// Tiles let go of since the last call (their GPU textures can go).
    pub(super) fn take_evicted(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.evicted)
    }
}

impl Reader {
    /// Before the list draws: the part of each page in view (for tiles).
    pub(super) fn note_seen_pages(&mut self) {
        self.tiles.seen.clear();
        if self.md.is_some() || self.status != Status::Open {
            return;
        }
        let view = self.list.viewport_bounds();
        let top = self.list.logical_scroll_top().item_ix;
        let last = self.page_count();
        for ix in top..(top + 12).min(last) {
            let Some(sheet) = self.sheet_bounds(ix) else {
                continue;
            };
            if sheet.top() > view.bottom() {
                break;
            }
            self.tiles.seen.insert(
                ix,
                (
                    f32::from(view.left() - sheet.left()).max(0.0),
                    f32::from(view.top() - sheet.top()).max(0.0),
                    f32::from(view.right() - sheet.left()),
                    f32::from(view.bottom() - sheet.top()),
                ),
            );
        }
    }
}

/// The scale for page pictures: `sharp`, or (a page too large to draw
/// whole) one that keeps the picture to [`STAND_IN_PIXELS`], with whether
/// tiles go on top.
pub(super) fn picture_scale(sharp: f32, w: f32, h: f32, sf: f32) -> (f32, bool) {
    let pixels = (w * sf) * (h * sf);
    if pixels > DEEP_PIXELS {
        (sharp * (STAND_IN_PIXELS / pixels).sqrt(), true)
    } else {
        (sharp, false)
    }
}

impl Reader {
    /// The sharp tiles over the part of page `ix` in view (`w` × `h` logical
    /// pixels at device `scale`); those not drawn yet are asked for.
    pub(super) fn tile_layer(
        &mut self,
        ix: usize,
        (w, h): (f32, f32),
        scale: f32,
        sf: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        // Where the sheet was last frame: close enough to pick tiles (the
        // next frame corrects a scroll).
        let Some(&(x0, y0, x1, y1)) = self.tiles.seen.get(&ix) else {
            // Not placed yet: look again next frame.
            cx.notify();
            return Vec::new();
        };
        let (x1, y1) = (x1.min(w), y1.min(h));
        if x1 <= x0 || y1 <= y0 {
            return Vec::new();
        }
        let (dw, dh) = ((w * sf).ceil() as u32, (h * sf).ceil() as u32);
        let span = |a: f32, b: f32, max: u32| {
            let first = ((a * sf) as u32) / TILE;
            let last = (((b * sf).ceil() as u32).min(max).saturating_sub(1)) / TILE;
            first..=last
        };
        let mut out = Vec::new();
        let base = TileKey {
            page: ix as u32,
            scale_key: scale_key(scale),
            rotation: self.rotation,
            generation: self.generation,
            tx: 0,
            ty: 0,
        };
        for ty in span(y0, y1, dh) {
            for tx in span(x0, x1, dw) {
                let key = TileKey { tx, ty, ..base };
                let rect = TilePx {
                    x: tx * TILE,
                    y: ty * TILE,
                    w: TILE.min(dw.saturating_sub(tx * TILE)),
                    h: TILE.min(dh.saturating_sub(ty * TILE)),
                };
                if rect.w == 0 || rect.h == 0 {
                    continue;
                }
                match self.tiles.map.get(&key) {
                    Some(image) => out.push(
                        img(ImageSource::Render(image.clone()))
                            .absolute()
                            .left(px(rect.x as f32 / sf))
                            .top(px(rect.y as f32 / sf))
                            .w(px(rect.w as f32 / sf))
                            .h(px(rect.h as f32 / sf))
                            .into_any_element(),
                    ),
                    None => self.request_tile(key, rect, scale, cx),
                }
            }
        }
        out
    }

    fn request_tile(&mut self, key: TileKey, rect: TilePx, scale: f32, cx: &mut Context<Self>) {
        let Some(doc) = self.doc else { return };
        if !self.tiles.pending.insert(key) {
            return;
        }
        let engine = self.engine.clone();
        let tint = self.page_tint(cx);
        let req = RenderRequest {
            page: key.page,
            scale,
            rotation: key.rotation,
            tile: Some(rect),
            generation: key.generation,
        };
        cx.spawn(async move |this, cx| {
            let bmp = cx
                .background_spawn(async move {
                    let mut bmp = engine.render(doc, req).ok().flatten()?;
                    if let Some(t) = tint {
                        t.apply(&mut bmp.bgra);
                    }
                    Some(bmp)
                })
                .await;
            let _ = this.update(cx, |r, cx| {
                r.tiles.pending.remove(&key);
                let Some(bmp) = bmp else { return };
                // Zoomed or turned meanwhile: not wanted any more.
                if key.generation != r.generation {
                    return;
                }
                if let Some(buffer) = RgbaImage::from_raw(bmp.width, bmp.height, bmp.bgra) {
                    r.tiles
                        .insert(key, Arc::new(RenderImage::new([Frame::new(buffer)])));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::picture_scale;

    #[test]
    fn large_pages_get_a_smaller_picture_and_tiles() {
        assert_eq!(picture_scale(2.0, 600.0, 800.0, 1.0), (2.0, false));
        let (s, deep) = picture_scale(10.0, 8000.0, 8000.0, 1.0);
        assert!(deep);
        let pixels = (8000.0 * s / 10.0) * (8000.0 * s / 10.0);
        assert!((pixels - super::STAND_IN_PIXELS).abs() / super::STAND_IN_PIXELS < 0.01);
    }
}
