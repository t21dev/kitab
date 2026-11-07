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
    order: VecDeque<PageKey>,
    bytes: usize,
    budget: usize,
    evicted: Vec<Arc<RenderImage>>,
}

impl PageCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            budget: budget_bytes,
            evicted: Vec::new(),
        }
    }

    pub fn get(&mut self, key: &PageKey) -> Option<Arc<RenderImage>> {
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
        self.map.contains_key(key)
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
            if let Some((img, b)) = self.map.remove(&oldest) {
                self.bytes -= b;
                self.evicted.push(img);
            }
        }
    }

    /// Images removed since the last call; free their atlas space.
    pub fn take_evicted(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.evicted)
    }

    /// Drop every rendering (zoom or rotation changed far enough, or the tab is hidden).
    pub fn clear(&mut self) {
        self.order.clear();
        self.bytes = 0;
        self.evicted.extend(self.map.drain().map(|(_, (i, _))| i));
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Frame, RgbaImage};

    fn img() -> Arc<RenderImage> {
        Arc::new(RenderImage::new([Frame::new(RgbaImage::new(1, 1))]))
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
