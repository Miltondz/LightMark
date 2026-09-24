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
    let lower_text = if !options.case_sensitive {
        Some(full_text.to_lowercase())
    } else {
        None
    };

    let search_text = lower_text.as_deref().unwrap_or(&full_text);

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

    let regex_options = if options.case_sensitive {
        regex::RegexBuilder::new(&pattern).build()
    } else {
        regex::RegexBuilder::new(&pattern)
            .case_insensitive(true)
            .build()
    };

    let re = match regex_options {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    let mut byte_offset = 0usize;

    for mat in re.find_iter(search_text) {
        // Convert byte offset in search_text to byte offset in original text
        let actual_start = if options.case_sensitive {
            mat.start()
        } else {
            // We need to map from the lowercased string back to the original
            // For simplicity, we search the original string instead
            byte_offset + mat.start()
        };

        let actual_end = actual_start + (mat.end() - mat.start());

        // Calculate line and column
        let prefix = &full_text[..actual_start];
        let line = prefix.chars().filter(|&c| c == '\n').count();
        let last_newline = prefix.rfind('\n');
        let column = match last_newline {
            Some(pos) => actual_start - pos - 1,
            None => actual_start,
        };

        // Get the matched text
        let matched_text: String = full_text[actual_start..actual_end].to_string();

        results.push(SearchResult {
            start: actual_start,
            end: actual_end,
            line,
            column,
            text: matched_text,
        });

        byte_offset = actual_end;
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
        // Apply all replacements in reverse order to maintain offsets
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

pub fn find_next(
    results: &[SearchResult],
    current_pos: usize,
    wrap: bool,
    text_len: usize,
) -> Option<usize> {
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
