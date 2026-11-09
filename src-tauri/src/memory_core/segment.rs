//! Text segmentation for the memory FTS index and queries.
//!
//! Index side: `segmented_text` yields space-joined word tokens for an FTS5
//! `unicode61` column. Query side: `fts_match_query` yields an OR of quoted
//! tokens (function words removed) that can never be an FTS5 syntax error.

use std::collections::HashSet;
use std::sync::OnceLock;

static JIEBA: OnceLock<jieba_rs::Jieba> = OnceLock::new();

fn jieba() -> &'static jieba_rs::Jieba {
    JIEBA.get_or_init(jieba_rs::Jieba::new)
}

const STOPWORDS: &[&str] = &[
    "我", "你", "他", "她", "它", "我们", "你们", "的", "了", "吗", "呢", "吧", "啊", "是", "在",
    "有", "和", "与", "什么", "怎么", "哪", "哪里", "哪个", "哪些", "那个", "这个", "一下", "还",
    "都", "就", "也", "请", "帮", "帮我", "知道", "记得", "还记得", "之前", "现在", "the", "a",
    "an", "is", "are", "am", "do", "does", "did", "i", "me", "my", "you", "your", "what", "which",
    "where", "who", "how", "of", "to", "in", "on", "for",
];

fn stopwords() -> &'static HashSet<&'static str> {
    static S: OnceLock<HashSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| STOPWORDS.iter().copied().collect())
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// A token is kept only if it contains at least one alphanumeric char.
fn keep(tok: &str) -> bool {
    tok.chars().any(|c| c.is_alphanumeric())
}

/// Split into ASCII identifier runs (kept whole) and other runs (to jieba).
fn push_tokens(text: &str, out: &mut Vec<String>) {
    let mut other = String::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    let flush = |other: &mut String, out: &mut Vec<String>| {
        if other.is_empty() {
            return;
        }
        for w in jieba().cut_for_search(other, true) {
            let w = w.word.trim().to_lowercase();
            if keep(&w) {
                out.push(w);
            }
        }
        other.clear();
    };
    while i < chars.len() {
        let (start, c) = chars[i];
        if is_ident_start(c) {
            let mut j = i;
            while j < chars.len() && is_ident_char(chars[j].1) {
                j += 1;
            }
            let end = chars.get(j).map(|x| x.0).unwrap_or(text.len());
            flush(&mut other, out);
            let run = text[start..end].trim_end_matches(['.', '-', '_']).to_lowercase();
            if keep(&run) {
                out.push(run);
            }
            i = j;
        } else {
            other.push(c);
            i += 1;
        }
    }
    flush(&mut other, out);
}

/// Index-side tokens: lowercased, punctuation dropped, stopwords kept.
pub fn segment(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    push_tokens(text, &mut out);
    out
}

/// Query-side tokens: like `segment`, minus stopwords, deduplicated, order kept.
pub fn segment_query(text: &str) -> Vec<String> {
    let sw = stopwords();
    let mut seen = HashSet::new();
    segment(text)
        .into_iter()
        .filter(|t| !sw.contains(t.as_str()) && seen.insert(t.clone()))
        .collect()
}

/// FTS5 MATCH expression: quoted query tokens joined by ` OR `; `None` if empty.
pub fn fts_match_query(text: &str) -> Option<String> {
    let toks = segment_query(text);
    if toks.is_empty() {
        return None;
    }
    Some(
        toks.iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

/// What gets stored in the FTS column.
pub fn segmented_text(text: &str) -> String {
    segment(text).join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_drops_function_words() {
        let t = segment_query("我儿子叫什么名字");
        assert!(t.contains(&"儿子".to_string()) && t.contains(&"名字".to_string()));
        assert!(!t.iter().any(|w| w == "我" || w == "什么"));
    }

    #[test]
    fn english_lowercased_and_identifiers_kept() {
        let t = segment_query("What app am I building, open-pandora?");
        for w in ["app", "building", "open-pandora"] {
            assert!(t.contains(&w.to_string()), "{w}");
        }
        assert!(!t.iter().any(|w| w == "what" || w == "i"));
        assert!(segment_query("put_page failed").contains(&"put_page".to_string()));
    }

    #[test]
    fn fts_query_shapes() {
        let q = fts_match_query("你好？？").unwrap();
        assert!(q.contains("\"你好\""), "{q}");
        assert_eq!(fts_match_query("？？ 😀"), None);
        assert_eq!(fts_match_query(""), None);
        let q = fts_match_query("a \"b\"").unwrap();
        assert_eq!(q.matches('"').count() % 2, 0, "{q}");
    }

    #[test]
    fn match_query_works_in_sqlite_fts5() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE VIRTUAL TABLE t USING fts5(body, tokenize='unicode61');")
            .unwrap();
        conn.execute(
            "INSERT INTO t(body) VALUES (?1)",
            [segmented_text("用户的儿子叫刘书维")],
        )
        .unwrap();
        let q = fts_match_query("我儿子叫什么名字").unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM t WHERE t MATCH ?1", [q], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
