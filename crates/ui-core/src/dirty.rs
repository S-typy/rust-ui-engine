use std::ops::{BitOr, BitOrAssign};

/// Work invalidated by a retained-node mutation.
///
/// Flags express required work, not a scheduler: the runtime decides when to
/// consume them. Paint-only invalidation does not imply measure or layout.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DirtyFlags(u8);

impl DirtyFlags {
    pub const NONE: Self = Self(0);
    pub const MEASURE: Self = Self(1);
    pub const LAYOUT: Self = Self(2);
    pub const PAINT: Self = Self(4);
    pub const SEMANTICS: Self = Self(8);
    pub const ALL: Self = Self(15);

    pub const fn contains(self, flags: Self) -> bool {
        self.0 & flags.0 == flags.0
    }

    pub const fn intersects(self, flags: Self) -> bool {
        self.0 & flags.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for DirtyFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for DirtyFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_categories_are_independent() {
        let mut flags = DirtyFlags::PAINT;
        assert!(!flags.intersects(DirtyFlags::MEASURE | DirtyFlags::LAYOUT));
        flags |= DirtyFlags::SEMANTICS;
        assert!(flags.contains(DirtyFlags::PAINT | DirtyFlags::SEMANTICS));
        assert!(!flags.contains(DirtyFlags::ALL));
        assert!(DirtyFlags::ALL.contains(flags));
        assert!(DirtyFlags::NONE.is_empty());
        assert!(!DirtyFlags::NONE.intersects(DirtyFlags::ALL));
        assert!(flags.contains(DirtyFlags::NONE));
    }
}
