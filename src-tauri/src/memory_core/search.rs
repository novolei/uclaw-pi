//! Rank fusion helpers for hybrid memory search.

/// Reciprocal-rank fusion: score = sum over lists of 1/(k + rank), rank 1-based.
/// Sorted by score desc; ties keep first-appearance order.
pub fn rrf_fuse(lists: &[Vec<String>], k: f32) -> Vec<(String, f32)> {
    use std::collections::HashMap;
    let mut idx: HashMap<&str, usize> = HashMap::new();
    let mut out: Vec<(String, f32)> = Vec::new();
    for list in lists {
        for (i, id) in list.iter().enumerate() {
            let add = 1.0 / (k + (i + 1) as f32);
            match idx.get(id.as_str()) {
                Some(&pos) => out[pos].1 += add,
                None => {
                    idx.insert(id.as_str(), out.len());
                    out.push((id.clone(), add));
                }
            }
        }
    }
    // Stable sort keeps first-appearance order for ties.
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rrf_rewards_agreement() {
        let r = rrf_fuse(&[v(&["a", "b"]), v(&["b", "c"])], 60.0);
        assert_eq!(r[0].0, "b");
        assert_eq!(r.len(), 3);
        assert!(r[0].1 > r[1].1);
    }

    #[test]
    fn rrf_ties_by_first_appearance() {
        let r = rrf_fuse(&[v(&["a"]), v(&["b"])], 60.0);
        assert_eq!(r[0].0, "a");
        assert_eq!(r[1].0, "b");
    }

    #[test]
    fn sigmoid_sane() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(-11.0) < 0.05);
        assert!(sigmoid(2.21) > 0.9);
    }
}
