// The matching half of docs/protocol.md "search"; search.rs owns the walk that calls it.
// A match is the query as plain text, case-insensitive, inside the entry's own name: the operator's
// ruling of 2026-10-08, after a subsequence match let "_dee" find
// "deepbratumia_model_install_handover.md" by its scattered _, d, e and e. A query holding a / is
// matched against the path below the search root instead, so "src/main" still finds src/main.rs.
// The score only orders what matched: the whole name, then a name the query starts, then a word the
// query starts, and an earlier match before a later one.
use crate::backend::sort::name_order;
use std::cmp::Ordering;

// The query is the whole name, which is the answer the operator is most likely typing toward.
const BONUS_WHOLE: i32 = 100;
// The query starts the name (or the path, for a query holding a /).
const BONUS_PREFIX: i32 = 50;
// The query starts a word: after a separator, or on a camelCase hump.
const BONUS_BOUNDARY: i32 = 20;
// The query also ends where a word does, so "note" in "my-note.txt" beats it inside "notebook".
const BONUS_WORD_END: i32 = 5;
// An earlier match beats a later one, up to this many characters in; past it they tie.
const MAX_POSITION_PENALTY: usize = 15;

// The characters a word or a path segment starts after.
fn is_separator(c: char) -> bool {
    c == '/' || c == '-' || c == '_' || c == '.' || c == ' '
}

// One candidate character, folded once per candidate so the scan compares by index rather than
// re-folding at every start position it tries.
// corner: a character whose lowercase form is more than one char (Turkish dotted capital I) keeps
// only the first, the same corner ui/js/Match.js documents for the accent run it paints.
#[derive(Clone, Copy)]
struct Folded {
    lower: char,
    upper: bool,
}

fn fold(c: char) -> Folded {
    let lower = c.to_lowercase().next().unwrap_or(c);
    Folded { lower, upper: lower != c }
}

// Holds the folded query for the whole walk and reuses one candidate buffer, so a subtree walk
// allocates twice rather than once per entry.
pub struct Matcher {
    needle: Vec<char>,
    // The query holds a /, so it is matched against the relative path and not the name alone.
    whole_path: bool,
    hay: Vec<Folded>,
}

impl Matcher {
    pub fn new(query: &str) -> Matcher {
        let needle: Vec<char> = query.chars().map(|c| fold(c).lower).collect();
        Matcher { whole_path: needle.contains(&'/'), needle, hay: Vec::new() }
    }

    // None means the query is not in the candidate at all, which is the gate every ranking sits
    // behind. Some carries the best occurrence's score, higher being the better match.
    pub fn score(&mut self, candidate: &str) -> Option<i32> {
        // An empty query matches everything, which is what an empty search line shows.
        if self.needle.is_empty() {
            return Some(0);
        }
        let text = if self.whole_path {
            candidate
        } else {
            candidate.rsplit('/').next().unwrap_or(candidate)
        };
        self.hay.clear();
        self.hay.extend(text.chars().map(fold));
        let n = self.needle.len();
        if n > self.hay.len() {
            return None;
        }
        let mut best: Option<i32> = None;
        for at in 0..=self.hay.len() - n {
            if (0..n).all(|k| self.hay[at + k].lower == self.needle[k]) {
                let s = self.occurrence_score(at);
                best = Some(best.map_or(s, |b| b.max(s)));
            }
        }
        best
    }

    fn occurrence_score(&self, at: usize) -> i32 {
        let end = at + self.needle.len();
        let mut score = -(at.min(MAX_POSITION_PENALTY) as i32);
        if at == 0 && end == self.hay.len() {
            score += BONUS_WHOLE;
        }
        if at == 0 {
            score += BONUS_PREFIX;
        } else if self.starts_a_word(at) {
            score += BONUS_BOUNDARY;
        }
        if end == self.hay.len() || self.starts_a_word(end) {
            score += BONUS_WORD_END;
        }
        score
    }

    fn starts_a_word(&self, at: usize) -> bool {
        if at == 0 {
            return true;
        }
        let before = self.hay[at - 1];
        is_separator(before.lower) || is_separator(self.hay[at].lower) || (self.hay[at].upper && !before.upper)
    }
}

// The order results are answered in: the better score first, then the shorter path, then the
// listing's own name order, so one walk over one tree always answers in exactly one order.
pub fn rank_order(a_score: i32, a_name: &str, b_score: i32, b_name: &str) -> Ordering {
    match b_score.cmp(&a_score) {
        Ordering::Equal => {}
        other => return other,
    }
    match a_name.len().cmp(&b_name.len()) {
        Ordering::Equal => {}
        other => return other,
    }
    name_order(a_name.as_bytes(), b_name.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(hay: &str, query: &str) -> Option<i32> {
        Matcher::new(query).score(hay)
    }

    #[test]
    fn a_scattered_subsequence_is_no_match() {
        // The operator's own report: every letter is there, in order, but never side by side.
        assert!(score("deepbratumia_model_install_handover.md", "_dee").is_none());
        assert!(score("downloads/helper.txt", "dwnhelp").is_none());
        assert!(score("helper.txt", "pleh").is_none());
        assert!(score("abc", "abcd").is_none());
    }

    #[test]
    fn the_query_as_plain_text_anywhere_in_the_name_matches() {
        assert!(score("deepbratumia_model_install_handover.md", "_model").is_some());
        assert!(score("deepbratumia_model_install_handover.md", "handover.md").is_some());
        assert!(score("my notes.txt", "y no").is_some());
    }

    #[test]
    fn a_parent_directory_alone_is_no_match() {
        // Otherwise "bench" would list every file under a bench folder.
        assert!(score("bench/notes.txt", "bench").is_none());
        assert!(score("notes/bench.txt", "bench").is_some());
    }

    #[test]
    fn a_query_holding_a_slash_matches_the_path() {
        assert!(score("src/main.rs", "src/main").is_some());
        assert!(score("src/backend/main.rs", "src/main").is_none());
        assert!(score("tools/src/main.rs", "src/main").is_some());
    }

    #[test]
    fn case_folds_both_ways_including_beyond_ascii() {
        assert!(score("Bench-Notes.md", "bench").is_some());
        assert!(score("BENCH", "bench").is_some());
        assert!(score("CAFÉ.txt", "café").is_some());
        assert!(score("café.txt", "CAFÉ").is_some());
    }

    #[test]
    fn an_empty_query_matches_every_candidate() {
        assert_eq!(score("anything", ""), Some(0));
        assert_eq!(score("", ""), Some(0));
    }

    #[test]
    fn the_whole_name_beats_a_prefix_beats_a_word_beats_inside_a_word() {
        let whole = score("bench", "bench").unwrap();
        let prefix = score("bench.txt", "bench").unwrap();
        let word = score("my-bench.txt", "bench").unwrap();
        let inside = score("workbench.txt", "bench").unwrap();
        assert!(whole > prefix && prefix > word && word > inside, "{} {} {} {}", whole, prefix, word, inside);
    }

    #[test]
    fn a_camel_hump_counts_as_a_word_start() {
        let hump = score("SearchStrip.qml", "strip").unwrap();
        let flat = score("searchstrip.qml", "strip").unwrap();
        assert!(hump > flat, "hump {} flat {}", hump, flat);
    }

    #[test]
    fn the_best_occurrence_is_the_one_scored() {
        // The first "note" is inside a word; the second starts one.
        let later = score("denote-note.txt", "note").unwrap();
        let only_inside = score("denote.txt", "note").unwrap();
        assert!(later > only_inside, "later {} inside {}", later, only_inside);
    }

    #[test]
    fn the_rank_order_is_total_so_one_tree_answers_in_one_order() {
        assert_eq!(rank_order(9, "a.txt", 4, "b.txt"), Ordering::Less);
        assert_eq!(rank_order(4, "a.txt", 9, "b.txt"), Ordering::Greater);
        assert_eq!(rank_order(4, "a.txt", 4, "bb.txt"), Ordering::Less);
        assert_eq!(rank_order(4, "b.txt", 4, "a.txt"), Ordering::Greater);
        assert_eq!(rank_order(4, "a.txt", 4, "a.txt"), Ordering::Equal);
    }
}
