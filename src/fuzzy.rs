//! Small reusable fuzzy matcher for command palettes and completion popups.
//!
//! Matching is intentionally deterministic: candidates must contain the query as a case-insensitive
//! subsequence, higher scores prefer contiguous and word-boundary matches, and ties keep input order.

/// Score and byte positions for a single fuzzy match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyMatch {
    /// Larger scores are better.
    pub score: i64,
    /// Matched byte positions in the candidate string.
    pub positions: Vec<usize>,
}

/// A candidate selected by [`fuzzy_filter`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyFilterMatch<'a> {
    /// Original candidate index before sorting.
    pub index: usize,
    /// Borrowed candidate text.
    pub candidate: &'a str,
    /// Larger scores are better.
    pub score: i64,
    /// Matched byte positions in the candidate string.
    pub positions: Vec<usize>,
}

/// Matches `query` as a case-insensitive subsequence of `candidate`.
pub fn fuzzy_match(candidate: &str, query: &str) -> Option<FuzzyMatch> {
    let query_chars: Vec<char> = query.chars().collect();
    if query_chars.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            positions: Vec::new(),
        });
    }

    let candidate_chars: Vec<(usize, char)> = candidate.char_indices().collect();
    let mut search_from = 0usize;
    let mut previous_char_index = None;
    let mut positions = Vec::with_capacity(query_chars.len());
    let mut score = 0i64;

    for query_char in query_chars {
        let found = (search_from..candidate_chars.len())
            .find(|idx| chars_equal_folded(candidate_chars[*idx].1, query_char))?;
        let (byte_idx, _) = candidate_chars[found];

        score += 32;
        if let Some(previous) = previous_char_index {
            if found == previous + 1 {
                score += 16;
            } else {
                let gap = found.saturating_sub(previous + 1).min(8) as i64;
                score -= gap;
            }
        }
        if is_word_boundary(candidate, byte_idx, found) {
            score += 8;
        }
        score -= found.min(16) as i64;

        positions.push(byte_idx);
        previous_char_index = Some(found);
        search_from = found + 1;
    }

    Some(FuzzyMatch { score, positions })
}

/// Filters and ranks candidates by fuzzy subsequence score.
pub fn fuzzy_filter<'a>(
    candidates: &'a [String],
    query: &str,
    limit: usize,
) -> Vec<FuzzyFilterMatch<'a>> {
    if limit == 0 {
        return Vec::new();
    }

    let mut matches: Vec<_> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let matched = fuzzy_match(candidate, query)?;
            Some(FuzzyFilterMatch {
                index,
                candidate,
                score: matched.score,
                positions: matched.positions,
            })
        })
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.index.cmp(&b.index)));
    matches.truncate(limit);
    matches
}

fn chars_equal_folded(a: char, b: char) -> bool {
    if a == b || a.eq_ignore_ascii_case(&b) {
        return true;
    }
    a.to_lowercase().to_string() == b.to_lowercase().to_string()
}

fn is_word_boundary(candidate: &str, byte_idx: usize, char_idx: usize) -> bool {
    if char_idx == 0 {
        return true;
    }
    candidate[..byte_idx]
        .chars()
        .next_back()
        .is_none_or(is_separator)
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '-' | '_' | '/' | '.' | ':' | '@')
}

#[cfg(test)]
mod tests {
    use super::{fuzzy_filter, fuzzy_match};

    #[test]
    fn fuzzy_match_requires_subsequence() {
        let matched = fuzzy_match("/open-file", "of").expect("subsequence match");
        assert_eq!(matched.positions, vec![1, 6]);
        assert!(fuzzy_match("/open-file", "zz").is_none());
    }

    #[test]
    fn fuzzy_match_scores_contiguous_matches_higher() {
        let contiguous = fuzzy_match("foo-bar", "fo").expect("contiguous match");
        let gapped = fuzzy_match("fast-open", "fo").expect("gapped match");
        assert!(contiguous.score > gapped.score);
    }

    #[test]
    fn fuzzy_filter_limits_and_preserves_ties_by_input_order() {
        let candidates = vec![
            "/open-file".to_string(),
            "/search-files".to_string(),
            "/switch-project".to_string(),
        ];

        let matches = fuzzy_filter(&candidates, "/", 2);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].candidate, "/open-file");
        assert_eq!(matches[1].candidate, "/search-files");
    }

    #[test]
    fn fuzzy_filter_empty_query_returns_initial_candidates() {
        let candidates = vec!["one".to_string(), "two".to_string()];
        let matches = fuzzy_filter(&candidates, "", 8);
        assert_eq!(
            matches.iter().map(|m| m.candidate).collect::<Vec<_>>(),
            candidates
        );
    }
}
