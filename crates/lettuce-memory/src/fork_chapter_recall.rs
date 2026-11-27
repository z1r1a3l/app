//! Fork-only: optional recall of folded chapter summaries when the scene
//! mentions them. Off by default; at most two chapters; hard token cap.

use std::sync::{Arc, RwLock};

/// Default and maximum number of folded chapters to recall.
pub const DEFAULT_LIMIT: u32 = 2;
pub const MAX_LIMIT: u32 = 2;
pub const MIN_LIMIT: u32 = 1;
/// Hard cap on recalled chapter text, not a user setting.
pub const TOKEN_BUDGET: u32 = 280;
/// Prompt heading (telegraphic English).
pub const HEADER: &str = "Earlier chapters:";

const MIN_WORD: usize = 4;
const HEADER_TOKENS: u32 = 4;

/// Global switch and 1–2 chapter limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterRecallConfig {
    pub enabled: bool,
    pub limit: u32,
}

impl Default for ChapterRecallConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            limit: DEFAULT_LIMIT,
        }
    }
}

impl ChapterRecallConfig {
    #[must_use]
    pub fn clamp(self) -> Self {
        Self {
            enabled: self.enabled,
            limit: self.limit.clamp(MIN_LIMIT, MAX_LIMIT),
        }
    }
}

/// A folded (`kind=chapter`, inactive) summary unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldedChapter {
    pub text: String,
    pub token_count: u32,
    pub window_start: u64,
    pub window_end: u64,
}

/// Process-wide lookup of recall settings and folded chapters.
pub trait ChapterRecallSource: Send + Sync {
    fn config(&self) -> ChapterRecallConfig;
    fn folded_chapters(
        &self,
        space_id: &str,
        conversation_id: &str,
        branch_id: &str,
    ) -> Vec<FoldedChapter>;
}

static SOURCE: RwLock<Option<Arc<dyn ChapterRecallSource>>> = RwLock::new(None);

pub fn set_chapter_recall_source(source: Arc<dyn ChapterRecallSource>) {
    *SOURCE.write().unwrap_or_else(|error| error.into_inner()) = Some(source);
}

pub fn clear_chapter_recall_source() {
    *SOURCE.write().unwrap_or_else(|error| error.into_inner()) = None;
}

pub fn chapter_recall_source() -> Option<Arc<dyn ChapterRecallSource>> {
    SOURCE.read().ok().and_then(|guard| guard.clone())
}

const STOP: &[&str] = &[
    "about",
    "after",
    "also",
    "been",
    "before",
    "being",
    "could",
    "does",
    "even",
    "from",
    "have",
    "here",
    "into",
    "just",
    "like",
    "more",
    "much",
    "only",
    "other",
    "over",
    "some",
    "than",
    "that",
    "them",
    "then",
    "there",
    "they",
    "this",
    "those",
    "through",
    "under",
    "very",
    "well",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "will",
    "with",
    "would",
    "your",
    "этот",
    "этого",
    "этой",
    "этом",
    "чтобы",
    "когда",
    "только",
    "после",
    "было",
    "будет",
    "была",
    "были",
    "меня",
    "тебя",
    "него",
    "нее",
    "нее",
    "них",
    "если",
    "даже",
    "уже",
    "еще",
    "еще",
    "здесь",
    "сейчас",
    "потом",
    "также",
    "может",
    "можно",
    "надо",
    "есть",
    "себя",
    "своей",
    "своих",
    "всех",
    "всего",
    "или",
    "между",
    "перед",
    "через",
    "более",
    "меньше",
    "очень",
    "тогда",
    "потому",
    "поэтому",
    "однако",
];

fn fold_word(word: &str) -> String {
    word.chars()
        .flat_map(char::to_lowercase)
        .map(|ch| if ch == 'ё' { 'е' } else { ch })
        .collect()
}

fn is_stop(word: &str) -> bool {
    STOP.iter().any(|stop| *stop == word)
}

/// Keywords from recent scene text: letters, length at least 4, minus a small stoplist.
#[must_use]
pub fn keywords(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in text.split(|ch: char| !ch.is_alphabetic()) {
        let word = fold_word(raw);
        if word.chars().count() < MIN_WORD || is_stop(&word) || !seen.insert(word.clone()) {
            continue;
        }
        out.push(word);
    }
    out
}

/// Overlap between scene keywords and the chapter text.
#[must_use]
pub fn score(chapter: &str, keys: &[String]) -> u32 {
    if keys.is_empty() {
        return 0;
    }
    let chapter_keys: std::collections::HashSet<String> = keywords(chapter).into_iter().collect();
    keys.iter()
        .filter(|key| chapter_keys.contains(*key))
        .count() as u32
}

fn est_tokens(text: &str) -> u32 {
    let chars = text.chars().count() as u32;
    if chars == 0 { 0 } else { (chars + 3) / 4 }
}

fn chapter_tokens(chapter: &FoldedChapter) -> u32 {
    if chapter.token_count > 0 {
        chapter.token_count
    } else {
        est_tokens(&chapter.text)
    }
}

fn trim_to_tokens(text: &str, max: u32) -> String {
    if max == 0 || text.trim().is_empty() {
        return String::new();
    }
    if est_tokens(text) <= max {
        return text.trim().to_owned();
    }
    let cap = (max as usize).saturating_mul(4).saturating_sub(1);
    if cap == 0 {
        return String::new();
    }
    let mut out: String = text.chars().take(cap).collect();
    if let Some(index) = out.rfind(|ch: char| ch.is_whitespace()) {
        if index > cap / 2 {
            out.truncate(index);
        }
    }
    out.push('…');
    out
}

/// Pick up to `limit` matching chapters that fit the hard token cap.
#[must_use]
pub fn select_chapters(chapters: &[FoldedChapter], recent: &str, limit: u32) -> Vec<FoldedChapter> {
    let limit = limit.clamp(MIN_LIMIT, MAX_LIMIT) as usize;
    let keys = keywords(recent);
    if keys.is_empty() {
        return Vec::new();
    }
    let mut ranked: Vec<&FoldedChapter> = chapters
        .iter()
        .filter(|chapter| !chapter.text.trim().is_empty() && score(&chapter.text, &keys) > 0)
        .collect();
    ranked.sort_by(|left, right| {
        score(&right.text, &keys)
            .cmp(&score(&left.text, &keys))
            .then(right.window_end.cmp(&left.window_end))
    });
    ranked.truncate(limit);

    let mut remaining = TOKEN_BUDGET.saturating_sub(HEADER_TOKENS);
    let mut picked = Vec::new();
    for chapter in ranked {
        if remaining == 0 {
            break;
        }
        let tokens = chapter_tokens(chapter);
        if tokens <= remaining {
            remaining = remaining.saturating_sub(tokens);
            picked.push(chapter.clone());
            continue;
        }
        if picked.is_empty() {
            let text = trim_to_tokens(&chapter.text, remaining);
            if !text.is_empty() {
                let tokens = est_tokens(&text);
                remaining = remaining.saturating_sub(tokens);
                picked.push(FoldedChapter {
                    text,
                    token_count: tokens,
                    window_start: chapter.window_start,
                    window_end: chapter.window_end,
                });
            }
        }
    }
    picked
}

/// Prompt block, or `None` when nothing was picked.
#[must_use]
pub fn render_block(chapters: &[FoldedChapter]) -> Option<String> {
    let texts: Vec<&str> = chapters
        .iter()
        .map(|chapter| chapter.text.trim())
        .filter(|text| !text.is_empty())
        .collect();
    if texts.is_empty() {
        return None;
    }
    Some(format!("{HEADER}\n{}", texts.join("\n\n")))
}

#[must_use]
pub fn append_block(summary: Option<String>, block: Option<String>) -> Option<String> {
    match (summary, block) {
        (Some(summary), Some(block)) => {
            let summary = summary.trim();
            if summary.is_empty() {
                Some(block)
            } else {
                Some(format!("{summary}\n\n{block}"))
            }
        }
        (summary, block) => block.or(summary),
    }
}

/// Looks up the process source and appends a block when recall is on.
#[must_use]
pub fn append_recalled_chapters(
    summary: Option<String>,
    space_id: &str,
    conversation_id: &str,
    branch_id: &str,
    recent: &str,
) -> Option<String> {
    let Some(source) = chapter_recall_source() else {
        return summary;
    };
    let config = source.config().clamp();
    if !config.enabled {
        return summary;
    }
    let chapters = source.folded_chapters(space_id, conversation_id, branch_id);
    let picked = select_chapters(&chapters, recent, config.limit);
    append_block(summary, render_block(&picked))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(text: &str, tokens: u32, window_end: u64) -> FoldedChapter {
        FoldedChapter {
            text: text.into(),
            token_count: tokens,
            window_start: 0,
            window_end,
        }
    }

    #[test]
    fn keywords_drop_short_words_and_stops_and_fold_yo() {
        let keys = keywords("The tavern and ещё Кассиан met after the storm");
        assert!(keys.contains(&"tavern".into()), "{keys:?}");
        assert!(keys.contains(&"кассиан".into()), "{keys:?}");
        assert!(keys.contains(&"storm".into()), "{keys:?}");
        assert!(
            !keys
                .iter()
                .any(|k| matches!(k.as_str(), "after" | "the" | "and" | "еще" | "ещё")),
            "{keys:?}"
        );
    }

    #[test]
    fn no_keywords_selects_nothing() {
        let chapters = [chapter("The tavern burns.", 8, 10)];
        assert!(select_chapters(&chapters, "a the an", 2).is_empty());
    }

    #[test]
    fn picks_the_best_match_and_respects_limit() {
        let chapters = [
            chapter("The quiet garden near the river.", 10, 5),
            chapter("Кассиан waits in the tavern after the storm.", 12, 20),
            chapter("Unrelated orchard trees.", 8, 30),
        ];
        let picked = select_chapters(&chapters, "Кассиан in the tavern during the storm", 1);
        assert_eq!(picked.len(), 1);
        assert!(picked[0].text.contains("Кассиан"), "{picked:?}");
        let two = select_chapters(&chapters, "Кассиан tavern river garden", 2);
        assert_eq!(two.len(), 2);
        assert!(two[0].text.contains("Кассиан") || two[1].text.contains("Кассиан"));
    }

    #[test]
    fn trims_a_too_long_first_chapter_and_skips_a_second_that_does_not_fit() {
        let long = "tavern cassian ".repeat(400);
        let chapters = [
            chapter(&long, 400, 10),
            chapter("The later tavern fight.", 50, 2),
        ];
        // Higher overlap ranks the long chapter first; after it is trimmed to
        // the budget, the second chapter no longer fits.
        let picked = select_chapters(&chapters, "tavern cassian brawl tonight", 2);
        assert_eq!(picked.len(), 1, "{picked:?}");
        assert!(picked[0].text.contains("cassian"), "{}", picked[0].text);
        assert!(picked[0].text.ends_with('…'), "{}", picked[0].text);
        assert!(est_tokens(&picked[0].text) + HEADER_TOKENS <= TOKEN_BUDGET);
    }

    #[test]
    fn render_and_append_are_token_conservative() {
        let block = render_block(&[chapter("Kassian held the pass.", 8, 1)]).expect("block");
        assert!(block.starts_with("Earlier chapters:\nKassian held the pass."));
        let summary = append_block(Some("Current chapter.".into()), Some(block.clone()));
        assert_eq!(
            summary.as_deref(),
            Some("Current chapter.\n\nEarlier chapters:\nKassian held the pass.")
        );
        assert_eq!(
            append_recalled_chapters(Some("kept".into()), "s", "c", "b", "tavern"),
            Some("kept".into())
        );
    }

    struct OnSource {
        config: ChapterRecallConfig,
        chapters: Vec<FoldedChapter>,
    }

    impl ChapterRecallSource for OnSource {
        fn config(&self) -> ChapterRecallConfig {
            self.config.clone()
        }
        fn folded_chapters(&self, _: &str, _: &str, _: &str) -> Vec<FoldedChapter> {
            self.chapters.clone()
        }
    }

    #[test]
    fn source_appends_only_when_enabled() {
        clear_chapter_recall_source();
        set_chapter_recall_source(Arc::new(OnSource {
            config: ChapterRecallConfig {
                enabled: true,
                limit: 2,
            },
            chapters: vec![chapter("Kassian held the mountain pass.", 10, 1)],
        }));
        let out = append_recalled_chapters(
            Some("Now in the valley.".into()),
            "s",
            "c",
            "b",
            "Kassian returns to the mountain",
        )
        .expect("appended");
        assert!(out.contains("Now in the valley."));
        assert!(out.contains("Earlier chapters:"));
        assert!(out.contains("mountain pass"));
        clear_chapter_recall_source();
        set_chapter_recall_source(Arc::new(OnSource {
            config: ChapterRecallConfig::default(),
            chapters: vec![chapter("Kassian held the mountain pass.", 10, 1)],
        }));
        assert_eq!(
            append_recalled_chapters(
                Some("Now in the valley.".into()),
                "s",
                "c",
                "b",
                "Kassian mountain"
            ),
            Some("Now in the valley.".into())
        );
        clear_chapter_recall_source();
    }
}
