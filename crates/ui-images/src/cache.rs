use crate::{DecodeOptions, ImageAsset, ImageError, ImageSource};
use std::collections::VecDeque;

/// Cache ownership is bounded; assets cloned by applications retain their own
/// lifetime. Failed decoding does not evict a valid prior entry.
pub struct ImageCache {
    entries: VecDeque<Entry>,
    max_entries: usize,
    max_bytes: usize,
    bytes: usize,
    hits: u64,
    misses: u64,
}
struct Entry {
    source: ImageSource,
    options: DecodeOptions,
    asset: ImageAsset,
    bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageCacheStats {
    pub entries: usize,
    pub retained_bytes: usize,
    pub hits: u64,
    pub misses: u64,
}
impl Default for ImageCache {
    fn default() -> Self {
        Self::new(128, 64 * 1024 * 1024).expect("valid default image cache")
    }
}
impl ImageCache {
    pub fn new(max_entries: usize, max_bytes: usize) -> Result<Self, ImageError> {
        if max_entries > 4096 || max_bytes > 512 * 1024 * 1024 {
            return Err(ImageError::Limit(
                "cache capacity exceeds 4096 entries / 512 MiB",
            ));
        }
        Ok(Self {
            entries: VecDeque::new(),
            max_entries,
            max_bytes,
            bytes: 0,
            hits: 0,
            misses: 0,
        })
    }
    pub fn load(
        &mut self,
        source: &ImageSource,
        options: DecodeOptions,
    ) -> Result<ImageAsset, ImageError> {
        options.validate()?;
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.source.id() == source.id() && entry.options == options)
        {
            let entry = self.entries.remove(index).expect("located cache entry");
            let asset = entry.asset.clone();
            self.entries.push_back(entry);
            self.hits = self.hits.saturating_add(1);
            return Ok(asset);
        }
        self.misses = self.misses.saturating_add(1);
        let asset = source.decode(options)?;
        let bytes = source.bytes().len() + asset.bitmap().pixels().len();
        // Valid resources larger than the configured cache budget are returned
        // uncached. Existing hot entries survive that one-off request.
        if self.max_entries > 0 && bytes <= self.max_bytes {
            while self.entries.len() >= self.max_entries || self.bytes + bytes > self.max_bytes {
                if let Some(entry) = self.entries.pop_front() {
                    self.bytes -= entry.bytes;
                }
            }
            self.entries.push_back(Entry {
                source: source.clone(),
                options,
                asset: asset.clone(),
                bytes,
            });
            self.bytes += bytes;
        }
        Ok(asset)
    }
    pub fn remove(&mut self, source: &ImageSource) {
        self.entries
            .retain(|entry| entry.source.id() != source.id());
        self.bytes = self.entries.iter().map(|entry| entry.bytes).sum();
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub fn stats(&self) -> ImageCacheStats {
        ImageCacheStats {
            entries: self.entries.len(),
            retained_bytes: self.bytes,
            hits: self.hits,
            misses: self.misses,
        }
    }
}
