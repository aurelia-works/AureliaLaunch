//! Aurelia Launch core: pure logic, no UI, no Tauri, no platform APIs.
//! Platform search (Spotlight / MDQuery / Everything / locate) lives behind
//! the `SearchProvider` trait so macOS ships first and Windows/Linux slot in later.

use std::cmp::Ordering;

/// A ranked launchable result.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub title: String,
    pub subtitle: String,
    pub score: i32,
    pub kind: ItemKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    App,
    File,
    Calc,
    WebSearch,
    SystemCommand,
}

/// Platform search backend. macOS implements this with Spotlight;
/// stubs exist for tests and future Windows/Linux backends.
pub trait SearchProvider {
    fn query(&self, input: &str, limit: usize) -> Vec<Item>;
}

/// In-memory provider for tests and fallback.
pub struct MemoryProvider {
    pub entries: Vec<Item>,
}

impl SearchProvider for MemoryProvider {
    fn query(&self, input: &str, limit: usize) -> Vec<Item> {
        let needle = input.to_lowercase();
        let mut hits: Vec<Item> = self
            .entries
            .iter()
            .filter(|e| {
                e.title.to_lowercase().contains(&needle)
                    || e.subtitle.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect();
        hits.sort_by(|a, b| {
            score_match(&b.title, &needle)
                .cmp(&score_match(&a.title, &needle))
                .then(b.score.cmp(&a.score))
        });
        hits.truncate(limit);
        hits
    }
}

fn score_match(title: &str, needle: &str) -> i32 {
    let t = title.to_lowercase();
    if t == needle {
        100
    } else if t.starts_with(needle) {
        50
    } else if t.contains(needle) {
        10
    } else {
        0
    }
}

/// Rank mixed results: exact > prefix > substring, then kind priority, then score.
pub fn rank(mut items: Vec<Item>, query: &str) -> Vec<Item> {
    let needle = query.to_lowercase();
    items.sort_by(|a, b| {
        score_match(&b.title, &needle)
            .cmp(&score_match(&a.title, &needle))
            .then(kind_rank(a.kind).cmp(&kind_rank(b.kind)))
            .then(b.score.cmp(&a.score))
            .then_with(|| a.title.cmp(&b.title))
    });
    items
}

fn kind_rank(k: ItemKind) -> u8 {
    match k {
        ItemKind::App => 0,
        ItemKind::Calc => 1,
        ItemKind::SystemCommand => 2,
        ItemKind::File => 3,
        ItemKind::WebSearch => 4,
    }
}

/// Minimal calculator: supports + - * / ( ) and decimals.
/// Returns None when the input is not a pure math expression.
pub fn try_calc(input: &str) -> Option<f64> {
    let expr: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if expr.is_empty() || !expr.chars().all(|c| "0123456789+-*/().".contains(c)) {
        return None;
    }
    if !expr.chars().any(|c| "+-*/".contains(c)) {
        return None;
    }
    let mut p = Parser {
        bytes: expr.as_bytes(),
        pos: 0,
    };
    let v = p.parse_expr().ok()?;
    if p.pos != expr.len() {
        return None;
    }
    Some(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn parse_expr(&mut self) -> Result<f64, ()> {
        let mut v = self.parse_term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.pos += 1;
                    v += self.parse_term()?;
                }
                Some(b'-') => {
                    self.pos += 1;
                    v -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(v)
    }
    fn parse_term(&mut self) -> Result<f64, ()> {
        let mut v = self.parse_factor()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.pos += 1;
                    v *= self.parse_factor()?;
                }
                Some(b'/') => {
                    self.pos += 1;
                    let d = self.parse_factor()?;
                    if d == 0.0 {
                        return Err(());
                    }
                    v /= d;
                }
                _ => break,
            }
        }
        Ok(v)
    }
    fn parse_factor(&mut self) -> Result<f64, ()> {
        match self.peek() {
            Some(b'(') => {
                self.pos += 1;
                let v = self.parse_expr()?;
                if self.peek() == Some(b')') {
                    self.pos += 1;
                    Ok(v)
                } else {
                    Err(())
                }
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => self.parse_number(),
            Some(b'-') => {
                self.pos += 1;
                Ok(-self.parse_factor()?)
            }
            _ => Err(()),
        }
    }
    fn parse_number(&mut self) -> Result<f64, ()> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == b'.') {
            self.pos += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or(())
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
}

/// Built-in web searches: `g query`, `gh query`, `yt query`, `docs query`.
pub fn web_search_items(input: &str) -> Vec<Item> {
    let (prefix, rest) = match input.split_once(' ') {
        Some((p, r)) if !r.trim().is_empty() => (p, r.trim()),
        _ => return vec![],
    };
    let url = match prefix {
        "g" => format!(
            "https://www.google.com/search?q={}",
            url_encode(rest)
        ),
        "gh" => format!("https://github.com/search?q={}", url_encode(rest)),
        "yt" => format!(
            "https://www.youtube.com/results?search_query={}",
            url_encode(rest)
        ),
        _ => return vec![],
    };
    vec![Item {
        title: format!("Search {prefix}: {rest}"),
        subtitle: url,
        score: 0,
        kind: ItemKind::WebSearch,
    }]
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Built-in system commands.
pub fn system_commands(input: &str) -> Vec<Item> {
    const CMDS: &[(&str, &str)] = &[
        ("lock", "Lock the screen"),
        ("sleep", "Put the Mac to sleep"),
        ("restart", "Restart the Mac"),
        ("shutdown", "Shut down the Mac"),
        ("emptytrash", "Empty the Trash"),
    ];
    let needle = input.to_lowercase();
    CMDS.iter()
        .filter(|(name, _)| name.contains(&needle))
        .map(|(name, desc)| Item {
            title: name.to_string(),
            subtitle: desc.to_string(),
            score: 0,
            kind: ItemKind::SystemCommand,
        })
        .collect()
}

/// Top-level query pipeline used by the Tauri shell.
pub fn query(input: &str, provider: &dyn SearchProvider) -> Vec<Item> {
    let input = input.trim();
    if input.is_empty() {
        return vec![];
    }
    let mut items = provider.query(input, 8);
    if let Some(v) = try_calc(input) {
        items.push(Item {
            title: format!("= {v}"),
            subtitle: "Calculator — Enter to copy".into(),
            score: 90,
            kind: ItemKind::Calc,
        });
    }
    items.extend(web_search_items(input));
    items.extend(system_commands(input));
    rank(items, input)
}

#[allow(clippy::len_cmp)]
pub fn _keep_ordering_link(o: Ordering) -> bool {
    o == Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_basic() {
        assert_eq!(try_calc("2+2*2"), Some(6.0));
        assert_eq!(try_calc("(1+2)*3"), Some(9.0));
        assert_eq!(try_calc("2/0"), None);
        assert_eq!(try_calc("hello"), None);
        assert_eq!(try_calc("Terminal"), None);
    }

    #[test]
    fn web_search_prefixes() {
        assert_eq!(web_search_items("g rust").len(), 1);
        assert_eq!(web_search_items("g").len(), 0);
        assert_eq!(web_search_items("unknown x").len(), 0);
    }

    #[test]
    fn rank_prefers_exact_and_apps() {
        let items = vec![
            Item {
                title: "Terminal Guide".into(),
                subtitle: "".into(),
                score: 0,
                kind: ItemKind::File,
            },
            Item {
                title: "Terminal".into(),
                subtitle: "".into(),
                score: 0,
                kind: ItemKind::App,
            },
        ];
        let out = rank(items, "Terminal");
        assert_eq!(out[0].kind, ItemKind::App);
    }

    #[test]
    fn pipeline_mixes_sources() {
        let p = MemoryProvider {
            entries: vec![Item {
                title: "Terminal".into(),
                subtitle: "/Apps".into(),
                score: 0,
                kind: ItemKind::App,
            }],
        };
        let out = query("2+2", &p);
        assert!(out.iter().any(|i| i.kind == ItemKind::Calc));
        let out2 = query("Term", &p);
        assert!(out2.iter().any(|i| i.kind == ItemKind::App));
    }
}
