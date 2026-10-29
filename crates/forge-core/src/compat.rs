//! Does an addon version run on a game version? Game versions are compared by their `sort_key`
//! from the API, never by name.

/// `None` when the addon version states nothing; otherwise whether `game` lies in `min..=max`.
/// A missing bound is open on that side.
pub fn compatible(min: Option<i32>, max: Option<i32>, game: i32) -> Option<bool> {
    if min.is_none() && max.is_none() {
        return None;
    }
    Some(min.is_none_or(|lo| lo <= game) && max.is_none_or(|hi| game <= hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_table() {
        assert_eq!(compatible(None, None, 5), None, "unknown");
        assert_eq!(compatible(Some(3), None, 5), Some(true), "from 3 on");
        assert_eq!(compatible(Some(3), None, 3), Some(true));
        assert_eq!(compatible(Some(3), None, 2), Some(false));
        assert_eq!(compatible(Some(3), Some(4), 4), Some(true), "3 to 4");
        assert_eq!(compatible(Some(3), Some(4), 5), Some(false));
        assert_eq!(compatible(None, Some(4), 1), Some(true), "up to 4");
    }
}
