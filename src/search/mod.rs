#![allow(dead_code)]

use ropey::Rope;

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

#[derive(Clone, Copy)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    pub wrap: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            wrap: true,
        }
    }
}

pub fn search_in_rope(rope: &Rope, query: &str, options: SearchOptions) -> Vec<SearchResult> {
    if query.is_empty() {
        return Vec::new();
    }

    let full_text = rope.to_string();

    let escaped_query = if options.use_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };

    let pattern = if options.whole_word {
        format!(r"\b{}\b", escaped_query)
    } else {
        escaped_query
    };

    let regex_builder = regex::RegexBuilder::new(&pattern);
    let mut regex_builder = regex_builder;
    if !options.case_sensitive {
        regex_builder.case_insensitive(true);
    }

    let re = match regex_builder.build() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();

    for mat in re.find_iter(&full_text) {
        let actual_start = mat.start();
        let actual_end = mat.end();

        let prefix = &full_text[..actual_start];
        let line = prefix.chars().filter(|&c| c == '\n').count();
        let last_newline = prefix.rfind('\n');
        let column = match last_newline {
            Some(pos) => actual_start - pos - 1,
            None => actual_start,
        };

        let matched_text: String = full_text[actual_start..actual_end].to_string();

        results.push(SearchResult {
            start: actual_start,
            end: actual_end,
            line,
            column,
            text: matched_text,
        });
    }

    results
}

pub fn replace_in_rope(
    rope: &mut Rope,
    results: &[SearchResult],
    replacement: &str,
    replace_all: bool,
) -> usize {
    if results.is_empty() {
        return 0;
    }

    if replace_all {
        for result in results.iter().rev() {
            rope.remove(result.start..result.end);
            rope.insert(result.start, replacement);
        }
        results.len()
    } else if !results.is_empty() {
        let first = &results[0];
        rope.remove(first.start..first.end);
        rope.insert(first.start, replacement);
        1
    } else {
        0
    }
}

pub fn find_next(results: &[SearchResult], current_pos: usize, wrap: bool) -> Option<usize> {
    for (i, result) in results.iter().enumerate() {
        if result.start > current_pos {
            return Some(i);
        }
    }
    if wrap {
        results.first().map(|_| 0)
    } else {
        None
    }
}

pub fn find_prev(results: &[SearchResult], current_pos: usize, wrap: bool) -> Option<usize> {
    for (i, result) in results.iter().enumerate().rev() {
        if result.start < current_pos {
            return Some(i);
        }
    }
    if wrap {
        results.last().map(|_| results.len() - 1)
    } else {
        None
    }
}
