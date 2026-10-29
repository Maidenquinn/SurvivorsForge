/// Shannon entropy in bits per character. English text is about 4, random base64 about 6.
pub fn shannon(text: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    let mut total = 0usize;
    for c in text.chars() {
        *counts.entry(c).or_insert(0usize) += 1;
        total += 1;
    }
    if total == 0 {
        return 0.0;
    }
    counts
        .values()
        .map(|&n| {
            let p = n as f64 / total as f64;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_of_known_texts() {
        assert_eq!(shannon(""), 0.0);
        assert_eq!(shannon("aaaa"), 0.0);
        assert!((shannon("abcd") - 2.0).abs() < 1e-9);
        let prose = "the quick brown fox jumps over the lazy dog and keeps running far away";
        assert!(shannon(prose) < 4.5);
    }
}
