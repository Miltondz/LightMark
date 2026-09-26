use ropey::Rope;

#[test]
fn test_markdown_rendering() {
    let md = "# Titulo 1\n\nEste es un parrafo con **negrita** y *cursiva*.\n\n- Item 1\n- Item 2";
    let html = pulldown_cmark::Parser::new(md);
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, html);
    assert!(out.contains("<h1>Titulo 1</h1>"));
    assert!(out.contains("<strong>negrita</strong>"));
    assert!(out.contains("<ul>"));
}

#[test]
fn test_markdown_preview_no_html_tags() {
    let md = "# Titulo 1\n\nEste es un parrafo con **negrita** y *cursiva*.\n\n- Item 1\n- Item 2";
    // Verify pulldown_cmark text events do not contain html tags
    let parser = pulldown_cmark::Parser::new(md);
    let mut preview = String::new();
    for event in parser {
        match event {
            pulldown_cmark::Event::Text(t) => preview.push_str(&t),
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Paragraph | pulldown_cmark::TagEnd::Heading(_)) => preview.push('\n'),
            _ => {}
        }
    }
    assert!(!preview.contains("<h1>"));
    assert!(!preview.contains("<p>"));
    assert!(!preview.contains("<ul>"));
    assert!(preview.contains("Titulo 1"));
    assert!(preview.contains("negrita"));
}

#[test]
fn test_search_and_replace() {
    let mut rope = Rope::from_str("Hola mundo, este es un texto de prueba. Hola de nuevo.");
    let full_text = rope.to_string();
    let re = regex::RegexBuilder::new(r"Hola").build().unwrap();
    let matches: Vec<(usize, usize)> = re.find_iter(&full_text).map(|m| (m.start(), m.end())).collect();
    assert_eq!(matches.len(), 2);

    // Replace first
    rope.remove(matches[0].0..matches[0].1);
    rope.insert(matches[0].0, "Saludos");
    assert_eq!(rope.to_string(), "Saludos mundo, este es un texto de prueba. Hola de nuevo.");
}

#[test]
fn test_conflict_resolution() {
    let dir = std::env::temp_dir().join("lightmark_test_conflicts");
    std::fs::create_dir_all(&dir).ok();
    let file1 = dir.join("test-doc.md");
    std::fs::write(&file1, "dummy").unwrap();

    let stem = file1.file_stem().unwrap().to_str().unwrap();
    let ext = file1.extension().unwrap().to_str().unwrap();
    let mut counter = 2;
    let candidate = loop {
        let name = format!("{}-{}.{}", stem, counter, ext);
        let cand = file1.with_file_name(name);
        if !cand.exists() {
            break cand;
        }
        counter += 1;
    };

    assert_eq!(candidate.file_name().unwrap().to_str().unwrap(), "test-doc-2.md");
    std::fs::remove_dir_all(&dir).ok();
}
