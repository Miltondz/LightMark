#![allow(dead_code)]

use ropey::Rope;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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

    let regex_opts = if options.case_sensitive {
        regex::RegexBuilder::new(&pattern).build()
    } else {
        regex::RegexBuilder::new(&pattern)
            .case_insensitive(true)
            .build()
    };

    let re = match regex_opts {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    let mut byte_offset = 0usize;

    for mat in re.find_iter(search_text) {
        let actual_start = if options.case_sensitive {
            mat.start()
        } else {
            byte_offset + mat.start()
        };

        let actual_end = actual_start + (mat.end() - mat.start());

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

// Workspace file system operations

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: SystemTime,
}

pub fn list_directory(dir: &Path) -> std::io::Result<Vec<FileInfo>> {
    let mut entries: Vec<FileInfo> = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        entries.push(FileInfo {
            path: path.clone(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified: metadata.modified()?,
        });
    }

    // Sort: directories first, then files, alphabetically
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    Ok(entries)
}

pub fn read_file_to_rope(path: &Path) -> std::io::Result<Rope> {
    let content = std::fs::read_to_string(path)?;
    Ok(Rope::from_str(&content))
}

pub fn save_file(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)
}

pub fn is_supported_file_type(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md")
            | Some("markdown")
            | Some("html")
            | Some("htm")
            | Some("js")
            | Some("mjs")
            | Some("cjs")
            | Some("sql")
            | Some("json")
            | Some("xml")
            | Some("txt")
    )
}

pub fn detect_language(path: &Path) -> Option<String> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md") | Some("markdown") => Some("markdown".to_string()),
        Some("html") | Some("htm") => Some("html".to_string()),
        Some("js") | Some("mjs") | Some("cjs") => Some("javascript".to_string()),
        Some("sql") => Some("sql".to_string()),
        Some("json") => Some("json".to_string()),
        Some("xml") => Some("xml".to_string()),
        Some("txt") => Some("plaintext".to_string()),
        _ => None,
    }
}

pub fn file_modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

pub fn has_external_changes(path: &Path, last_known: SystemTime) -> bool {
    file_modified_time(path).is_some_and(|t| t > last_known)
}

pub fn is_large_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() > 10 * 1024 * 1024)
        .unwrap_or(false)
}

pub fn is_huge_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() > 100 * 1024 * 1024)
        .unwrap_or(false)
}

// Atomic save: write to temp, flush, then rename
pub fn atomic_save(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;

    let temp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));

    // Write to temp file
    let mut file = std::fs::File::create(&temp_path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;

    // Atomic replace
    std::fs::rename(&temp_path, path)?;
    Ok(())
}

// Check if file has been modified externally since last check
pub fn check_external_modification(
    path: &Path,
    last_check: SystemTime,
) -> Result<Option<SystemTime>, std::io::Error> {
    let current_time = file_modified_time(path);
    if current_time.is_some() && current_time.unwrap() > last_check {
        Ok(current_time)
    } else {
        Ok(None)
    }
}

// Get file encoding info
pub fn get_encoding_info(path: &Path) -> (String, String) {
    let mut encoding = "UTF-8".to_string();
    let mut line_endings = "LF".to_string();

    if let Ok(content) = std::fs::read(path) {
        // Check for BOM
        if content.len() >= 3 && content[0] == 0xEF && content[1] == 0xBB && content[2] == 0xBF {
            encoding = "UTF-8 BOM".to_string();
        } else if content.len() >= 2 && content[0] == 0xFF && content[1] == 0xFE {
            encoding = "UTF-16 LE".to_string();
        } else if content.len() >= 2 && content[0] == 0xFE && content[1] == 0xFF {
            encoding = "UTF-16 BE".to_string();
        }

        // Detect line endings
        let sample = std::str::from_utf8(&content).unwrap_or("");
        let sample_truncated = &sample[..sample.len().min(8192)];
        if sample_truncated.contains("\r\n") {
            line_endings = "CRLF".to_string();
        } else if sample_truncated.contains('\r') {
            line_endings = "CR".to_string();
        }
    }

    (encoding, line_endings)
}

// Walk directory respecting .gitignore
pub fn walk_project_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip hidden directories and common ignored dirs
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                files.extend(walk_project_files(&path));
            } else if is_supported_file_type(&path) {
                files.push(path);
            }
        }
    }

    files
}

// Workspace management
pub fn is_in_workspace(root: &Path, file: &Path) -> bool {
    file.starts_with(root)
}

pub fn get_relative_path(root: &Path, file: &Path) -> Option<String> {
    file.strip_prefix(root).ok().map(|p| {
        let s = p.to_string_lossy();
        s.to_string()
    })
}

pub fn time_since_modified(path: &Path) -> Option<Duration> {
    let modified = file_modified_time(path)?;
    modified.elapsed().ok()
}
