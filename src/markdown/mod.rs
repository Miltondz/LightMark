#![allow(dead_code)]

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

pub fn render_markdown_to_html(input: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(input, options);

    let mut html = String::new();
    let mut in_paragraph = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                let heading_level = match level {
                    pulldown_cmark::HeadingLevel::H1 => 1,
                    pulldown_cmark::HeadingLevel::H2 => 2,
                    pulldown_cmark::HeadingLevel::H3 => 3,
                    pulldown_cmark::HeadingLevel::H4 => 4,
                    pulldown_cmark::HeadingLevel::H5 => 5,
                    pulldown_cmark::HeadingLevel::H6 => 6,
                };
                html.push_str(&format!("<h{}>", heading_level));
                in_paragraph = false;
            }
            Event::End(TagEnd::Heading(_)) => {
                let last_open = html.rfind("<h").unwrap_or(0);
                let heading_num = html[last_open + 2..last_open + 3].parse().unwrap_or(1);
                html.push_str(&format!("</h{}>", heading_num));
            }
            Event::Start(Tag::Paragraph) => {
                html.push_str("<p>");
                in_paragraph = true;
            }
            Event::End(TagEnd::Paragraph) => {
                html.push_str("</p>");
                in_paragraph = false;
            }
            Event::Start(Tag::Item { .. }) => {
                html.push_str("<li>");
            }
            Event::End(TagEnd::Item) => {
                html.push_str("</li>");
            }
            Event::Start(Tag::List(list_type)) => {
                let list_tag = if list_type.is_some() { "ol" } else { "ul" };
                html.push_str(&format!("<{}>", list_tag));
            }
            Event::End(TagEnd::List(_)) => {
                if html.trim_end().ends_with("</ol>") {
                    html.push_str("</ol>");
                } else {
                    html.push_str("</ul>");
                }
            }
            Event::Start(Tag::CodeBlock { .. }) => {
                html.push_str("<pre><code>");
            }
            Event::End(TagEnd::CodeBlock) => {
                html.push_str("</code></pre>");
            }
            Event::Start(Tag::BlockQuote { .. }) => {
                html.push_str("<blockquote>");
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                html.push_str("</blockquote>");
            }
            Event::Start(Tag::Strong { .. }) => {
                html.push_str("<strong>");
            }
            Event::End(TagEnd::Strong) => {
                html.push_str("</strong>");
            }
            Event::Start(Tag::Emphasis { .. }) => {
                html.push_str("<em>");
            }
            Event::End(TagEnd::Emphasis) => {
                html.push_str("</em>");
            }
            Event::Code(text) => {
                html.push_str(&format!("<code>{}</code>", escape_html(&text)));
            }
            Event::Text(text) => {
                html.push_str(&escape_html(text.as_ref()));
            }
            Event::Html(html_text) => {
                html.push_str(html_text.as_ref());
            }
            Event::Rule => {
                html.push_str("<hr/>");
            }
            _ => {}
        }
    }

    if in_paragraph {
        html.push_str("</p>");
    }

    html
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn markdown_to_plain_text(input: &str) -> String {
    let parser = Parser::new(input);
    let mut result = String::new();

    for event in parser {
        match event {
            Event::Text(text) => {
                result.push_str(&text);
            }
            Event::Rule => {
                result.push_str("\n---\n");
            }
            _ => {}
        }
    }

    result
}
