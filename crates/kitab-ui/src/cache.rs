// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rendered pages, bounded by bytes (AGENTS.md rule 5). Evicted images are
//! handed back so the caller can free their GPU atlas space (`drop_image`).

use gpui::RenderImage;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// A page rendered at one device scale. `scale_key` is the scale × 64, rounded,
/// so nearby zooms reuse the same bitmap until the sharp one arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageKey {
    pub page: u32,
    pub scale_key: u32,
    pub rotation: u16,
}

pub fn scale_key(scale: f32) -> u32 {
    (scale * 64.0).round().max(1.0) as u32
}

pub struct PageCache {
    map: HashMap<PageKey, (Arc<RenderImage>, usize)>,
    /// Pages whose look changed (a form field filled in): their pictures
    /// still show until new ones come, but count as missing.
    stale: std::collections::HashSet<PageKey>,
    order: VecDeque<PageKey>,
    bytes: usize,
    budget: usize,
    evicted: Vec<Arc<RenderImage>>,
}

impl PageCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            map: HashMap::new(),
            stale: std::collections::HashSet::new(),
            order: VecDeque::new(),
            bytes: 0,
            budget: budget_bytes,
            evicted: Vec::new(),
        }
    }

    pub fn get(&mut self, key: &PageKey) -> Option<Arc<RenderImage>> {
        if self.stale.contains(key) {
            return None;
        }
        let image = self.map.get(key).map(|(i, _)| i.clone())?;
        // Most recently used goes to the back.
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        self.order.push_back(*key);
        Some(image)
    }

    /// Whether this exact rendering is cached (no effect on eviction order).
    pub fn contains(&self, key: &PageKey) -> bool {
        self.map.contains_key(key) && !self.stale.contains(key)
    }

    /// Every picture of `page` is out of date: shown until replaced, but
    /// [`PageCache::get`] and [`PageCache::contains`] miss, so it is drawn again.
    pub fn mark_stale(&mut self, page: u32) {
        self.stale
            .extend(self.map.keys().filter(|k| k.page == page).copied());
    }

    /// Any cached rendering of this page (another zoom), to show while the sharp one renders.
    pub fn any_for_page(&self, page: u32, rotation: u16) -> Option<Arc<RenderImage>> {
        self.order
            .iter()
            .rev()
            .find(|k| k.page == page && k.rotation == rotation)
            .and_then(|k| self.map.get(k))
            .map(|(i, _)| i.clone())
    }

    pub fn insert(&mut self, key: PageKey, image: Arc<RenderImage>, bytes: usize) {
        self.stale.remove(&key);
        if let Some((old, old_bytes)) = self.map.insert(key, (image, bytes)) {
            self.bytes -= old_bytes;
            self.evicted.push(old);
            if let Some(pos) = self.order.iter().position(|k| *k == key) {
                self.order.remove(pos);
            }
        }
        self.order.push_back(key);
        self.bytes += bytes;
        while self.bytes > self.budget && self.order.len() > 1 {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.stale.remove(&oldest);
            if let Some((img, b)) = self.map.remove(&oldest) {
                self.bytes -= b;
                self.evicted.push(img);
            }
        }
    }

    /// Keep up to `bytes` (at least what is on screen); a lower budget
    /// takes effect at the next insert.
    pub fn set_budget(&mut self, bytes: usize) {
        self.budget = bytes;
    }

    /// Images removed since the last call; free their atlas space.
    pub fn take_evicted(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.evicted)
    }

    /// Drop every rendering (zoom or rotation changed far enough, or the tab is hidden).
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.order.clear();
        self.stale.clear();
        self.bytes = 0;
        self.evicted.extend(self.map.drain().map(|(_, (i, _))| i));
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The most recently used rendering of `page` (any zoom), if there is one.
    pub fn newest_key_for_page(&self, page: u32, rotation: u16) -> Option<PageKey> {
        self.order
            .iter()
            .rev()
            .find(|k| k.page == page && k.rotation == rotation && !self.stale.contains(k))
            .copied()
    }

    /// The image for `key` without touching the eviction order.
    pub fn peek(&self, key: &PageKey) -> Option<Arc<RenderImage>> {
        self.map.get(key).map(|(i, _)| i.clone())
    }

    /// The page `key` has been painted, so its pixels are on the GPU: let
    /// the copy in memory go and keep a husk (same id and size, no bytes).
    /// It still counts its full size here: the texture is as big as it was.
    pub fn slim(&mut self, key: &PageKey) {
        if let Some((image, _)) = self.map.get_mut(key)
            && !image.kitab_is_husk()
        {
            *image = Arc::new(RenderImage::kitab_husk(image));
        }
    }

    /// The GPU lost its textures: husks have nothing to show any more. Drop
    /// them, so those pages are drawn again. Whether any were dropped.
    pub fn drop_husks(&mut self) -> bool {
        let gone: Vec<PageKey> = self
            .map
            .iter()
            .filter(|(_, (i, _))| i.kitab_is_husk())
            .map(|(k, _)| *k)
            .collect();
        for key in &gone {
            self.remove(key);
        }
        !gone.is_empty()
    }

    /// Keep only `keep`; everything else is evicted (a hidden tab keeps
    /// what it shows and nothing more).
    pub fn retain_only(&mut self, keep: &[PageKey]) {
        let gone: Vec<PageKey> = self
            .map
            .keys()
            .filter(|k| !keep.contains(k))
            .copied()
            .collect();
        for key in gone {
            self.remove(&key);
        }
    }

    /// Evict one rendering, if it is there.
    pub fn remove(&mut self, key: &PageKey) {
        if let Some((img, b)) = self.map.remove(key) {
            self.bytes -= b;
            self.evicted.push(img);
            self.stale.remove(key);
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
        }
    }
}

/// A smaller copy of `image` with at most `max_pixels` pixels (box filter,
/// whole-number factor), with its size in bytes and the factor used. `None`
/// when it is already that small. The channel order is kept as it is.
pub fn shrink(image: &RenderImage, max_pixels: u64) -> Option<(Arc<RenderImage>, usize, u32)> {
    let size = image.size(0);
    let (w, h) = (size.width.0.max(0) as u32, size.height.0.max(0) as u32);
    let pixels = u64::from(w) * u64::from(h);
    if pixels <= max_pixels || w == 0 || h == 0 {
        return None;
    }
    let f = ((pixels as f64 / max_pixels as f64).sqrt().ceil() as u32).max(2);
    let (nw, nh) = (w.div_ceil(f), h.div_ceil(f));
    let src = image.as_bytes(0)?;
    if src.len() < (w as usize) * (h as usize) * 4 {
        return None;
    }
    let mut out = vec![0u8; nw as usize * nh as usize * 4];
    for ny in 0..nh {
        let y0 = ny * f;
        let y1 = (y0 + f).min(h);
        for nx in 0..nw {
            let x0 = nx * f;
            let x1 = (x0 + f).min(w);
            let mut sum = [0u32; 4];
            for y in y0..y1 {
                let row = (y as usize * w as usize + x0 as usize) * 4;
                for px in src[row..row + (x1 - x0) as usize * 4].chunks_exact(4) {
                    for c in 0..4 {
                        sum[c] += u32::from(px[c]);
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            let at = (ny as usize * nw as usize + nx as usize) * 4;
            for c in 0..4 {
                out[at + c] = (sum[c] / n) as u8;
            }
        }
    }
    let bytes = out.len();
    let buffer = image::RgbaImage::from_raw(nw, nh, out)?;
    Some((
        Arc::new(RenderImage::new([image::Frame::new(buffer)])),
        bytes,
        f,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Frame, RgbaImage};

    fn img() -> Arc<RenderImage> {
        Arc::new(RenderImage::new([Frame::new(RgbaImage::new(1, 1))]))
    }

    #[test]
    fn a_painted_page_keeps_only_a_husk() {
        let key = |page| PageKey {
            page,
            scale_key: 64,
            rotation: 0,
        };
        let mut c = PageCache::new(1000);
        let full = Arc::new(RenderImage::new([Frame::new(RgbaImage::new(3, 2))]));
        let id = full.id;
        c.insert(key(0), full, 24);
        c.insert(key(1), img(), 4);
        c.slim(&key(0));
        let husk = c.get(&key(0)).unwrap();
        // The same picture to the GPU (id, size), with no bytes in memory.
        assert!(husk.kitab_is_husk());
        assert_eq!(husk.id, id);
        assert_eq!((husk.size(0).width.0, husk.size(0).height.0), (3, 2));
        assert!(husk.as_bytes(0).is_none());
        assert_eq!(husk.frame_count(), 1, "drop_image still frees its texture");
        assert_eq!(c.bytes(), 28, "the texture is as big as it was");
        assert!(c.take_evicted().is_empty(), "its texture stays");
        // The GPU lost its textures: husks go (drawn again), full pages stay.
        assert!(c.drop_husks());
        assert!(!c.contains(&key(0)));
        assert!(c.contains(&key(1)));
        assert!(!c.drop_husks());
    }

    #[test]
    fn stays_within_budget_and_reports_evictions() {
        let mut c = PageCache::new(100);
        for p in 0..5 {
            c.insert(
                PageKey {
                    page: p,
                    scale_key: 64,
                    rotation: 0,
                },
                img(),
                40,
            );
        }
        assert!(c.bytes() <= 100);
        assert_eq!(c.take_evicted().len(), 3);
        assert!(
            c.get(&PageKey {
                page: 4,
                scale_key: 64,
                rotation: 0
            })
            .is_some()
        );
        assert!(
            c.get(&PageKey {
                page: 0,
                scale_key: 64,
                rotation: 0
            })
            .is_none()
        );
    }

    #[test]
    fn a_stale_page_still_shows_until_it_is_drawn_again() {
        let mut c = PageCache::new(1_000);
        let k = PageKey {
            page: 3,
            scale_key: 64,
            rotation: 0,
        };
        c.insert(k, img(), 40);
        c.mark_stale(3);
        assert!(
            c.get(&k).is_none() && !c.contains(&k),
            "asks for a new picture"
        );
        assert!(
            c.any_for_page(3, 0).is_some(),
            "the old one shows meanwhile"
        );
        c.insert(k, img(), 40);
        assert!(c.get(&k).is_some());
    }

    #[test]
    fn retain_only_keeps_what_shows() {
        let mut c = PageCache::new(1_000);
        let k = |page| PageKey {
            page,
            scale_key: 64,
            rotation: 0,
        };
        for p in 0..4 {
            c.insert(k(p), img(), 40);
        }
        c.retain_only(&[k(2)]);
        assert_eq!(c.bytes(), 40);
        assert_eq!(c.take_evicted().len(), 3, "evicted ones are handed back");
        assert!(c.contains(&k(2)));
        assert_eq!(c.newest_key_for_page(2, 0), Some(k(2)));
        assert_eq!(c.newest_key_for_page(1, 0), None);
    }

    #[test]
    fn shrink_averages_and_bounds_pixels() {
        let mut buf = RgbaImage::new(40, 30);
        for (x, _, p) in buf.enumerate_pixels_mut() {
            *p = image::Rgba(if x % 2 == 0 {
                [0, 0, 0, 255]
            } else {
                [200, 100, 50, 255]
            });
        }
        let big = RenderImage::new([Frame::new(buf)]);
        let (small, bytes, f) = shrink(&big, 300).expect("shrinks");
        let s = small.size(0);
        assert!(u64::from(s.width.0 as u32) * u64::from(s.height.0 as u32) <= 300);
        assert_eq!(bytes, (s.width.0 * s.height.0 * 4) as usize);
        assert!(f >= 2);
        let px = &small.as_bytes(0).unwrap()[..4];
        assert_eq!(px, &[100, 50, 25, 255], "a box average of the two colours");
        assert!(shrink(&big, 10_000).is_none(), "already small enough");
    }

    #[test]
    fn recently_used_survives() {
        let mut c = PageCache::new(100);
        let k0 = PageKey {
            page: 0,
            scale_key: 64,
            rotation: 0,
        };
        c.insert(k0, img(), 40);
        c.insert(
            PageKey {
                page: 1,
                scale_key: 64,
                rotation: 0,
            },
            img(),
            40,
        );
        assert!(c.get(&k0).is_some());
        c.insert(
            PageKey {
                page: 2,
                scale_key: 64,
                rotation: 0,
            },
            img(),
            40,
        );
        assert!(c.get(&k0).is_some(), "page 0 was used last and must stay");
    }
}
