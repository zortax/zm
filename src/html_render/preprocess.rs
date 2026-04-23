//! Best-effort conversion of legacy HTML email attributes to inline CSS.
//!
//! Email HTML is full of presentational attributes (`bgcolor`, `width`,
//! `cellpadding`, `<font>`, `<center>`, …) that modern renderers ignore.
//! We do a regex pass over the raw HTML string to migrate them into
//! `style="…"` before handing the markup to blitz.
//!
//! Additionally, we strip Microsoft Office / Outlook-specific markup
//! (MSO conditional comments, VML, Office namespace tags, mso-* CSS
//! properties) that would otherwise confuse the renderer.

use once_cell::sync::Lazy;
use regex::{Captures, Regex};

// ---------------------------------------------------------------------------
// Default stylesheet injected into every email
// ---------------------------------------------------------------------------

/// Baseline CSS that makes table-based email layouts work sanely in blitz.
const EMAIL_DEFAULT_STYLES: &str = r#"<style>
body { margin: 0; padding: 0; width: 100%; -webkit-text-size-adjust: 100%; }
table { border-collapse: collapse; border-spacing: 0; border: none; }
td, th { padding: 0; border: none; }
img { border: 0; display: block; outline: none; text-decoration: none; max-width: 100%; height: auto; }
a img { border: none; }
p { margin: 0; padding: 0; }
h1, h2, h3, h4, h5, h6 { margin: 0; padding: 0; }
</style>"#;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn preprocess_email_html(html: &str) -> String {
    let mut out = html.to_string();

    // Phase 1: Strip junk that will confuse the HTML parser.
    // These must run before any tag-level rewrites.
    out = strip_xml_processing_instructions(&out);
    out = strip_mso_conditional_comments(&out);
    out = strip_vml_tags(&out);
    out = strip_office_namespace_tags(&out);
    out = strip_mso_css_in_style_blocks(&out);
    out = strip_mso_css_inline(&out);

    // Phase 2: Rewrite legacy tags to modern equivalents.
    out = rewrite_center_tag(&out);
    out = rewrite_font_tag(&out);

    // Phase 3: Convert presentational HTML attributes to inline CSS.
    out = convert_bgcolor(&out);
    out = convert_background(&out);
    out = convert_width(&out);
    out = convert_height(&out);
    out = convert_align(&out);
    out = convert_valign(&out);
    out = convert_cellspacing(&out);
    out = convert_cellpadding(&out);
    out = convert_border(&out);

    // Phase 4: Force border:0 on all table/td/th/tr tags that don't already
    // have an explicit border style.  This removes the black lines that blitz
    // renders by default.
    out = force_no_borders(&out);

    // Phase 5: Inject default styles.
    out = inject_default_styles(&out);

    out
}

// ---------------------------------------------------------------------------
// Phase 1: Strip Microsoft Office / Outlook junk
// ---------------------------------------------------------------------------

/// Remove `<?xml ...?>` processing instructions.
fn strip_xml_processing_instructions(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<\?xml[^?]*\?>").unwrap());
    RE.replace_all(html, "").into_owned()
}

/// Remove MSO/IE conditional comments and everything inside them.
///
/// Handles patterns like:
///   `<!--[if mso]>...<![endif]-->`
///   `<!--[if gte mso 9]>...<![endif]-->`
///   `<!--[if !mso]><!-->...<!--<![endif]-->`  (the "not-mso" variant keeps content)
fn strip_mso_conditional_comments(html: &str) -> String {
    // "Not-mso" wrappers: keep the content but strip the comment delimiters.
    // Must run FIRST so we don't accidentally strip the content.
    //   <!--[if !mso]><!--> CONTENT <!--<![endif]-->
    static RE_NOT_MSO_OPEN: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)<!--\[if\s+!mso\]><![-\s]*>").unwrap()
    });
    static RE_NOT_MSO_CLOSE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<!--<!\[endif\]-->").unwrap());

    let out = RE_NOT_MSO_OPEN.replace_all(html, "").into_owned();
    let out = RE_NOT_MSO_CLOSE.replace_all(&out, "").into_owned();

    // Standard MSO conditional blocks – strip them and their content.
    // These contain Outlook-only markup (often VML) that we don't want.
    // Use a negative lookbehind-like approach: require no `!` before `mso`.
    //   <!--[if mso]> ... <![endif]-->
    //   <!--[if gte mso 9]> ... <![endif]-->
    static RE_MSO_BLOCK: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)<!--\[if\s(?:[^\]!]*?\s)?mso[^\]]*?\]>.*?<!\[endif\]-->").unwrap()
    });
    let out = RE_MSO_BLOCK.replace_all(&out, "").into_owned();

    // Generic IE conditional comments (non-mso) – also strip.
    //   <!--[if IE]>...<![endif]-->
    //   <!--[if gt IE 8]>...<![endif]-->
    static RE_IE_BLOCK: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)<!--\[if\s[^\]]*?\]>.*?<!\[endif\]-->").unwrap()
    });
    RE_IE_BLOCK.replace_all(&out, "").into_owned()
}

/// Remove VML tags (`<v:*>...</v:*>` and self-closing `<v:* />`).
fn strip_vml_tags(html: &str) -> String {
    // Remove paired VML tags and their content
    static RE_VML_PAIRED: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)<v:[a-z]+[^>]*>.*?</v:[a-z]+\s*>").unwrap()
    });
    let out = RE_VML_PAIRED.replace_all(html, "").into_owned();

    // Remove self-closing VML tags
    static RE_VML_SELF: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<v:[a-z]+[^>]*/\s*>").unwrap());
    RE_VML_SELF.replace_all(&out, "").into_owned()
}

/// Strip Office namespace tags: `<o:p>`, `<o:OfficeDocumentSettings>`, `<w:*>`, etc.
/// `<o:p>` is converted to a simple `<span>` since it's often used as an inline wrapper.
fn strip_office_namespace_tags(html: &str) -> String {
    // <o:p> ... </o:p> → <span>...</span>  (commonly used as a paragraph wrapper)
    static RE_OP_OPEN: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<o:p[^>]*>").unwrap());
    static RE_OP_CLOSE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)</o:p\s*>").unwrap());
    let out = RE_OP_OPEN.replace_all(html, "<span>").into_owned();
    let out = RE_OP_CLOSE.replace_all(&out, "</span>").into_owned();

    // Other Office namespace tags (o:*, w:*, x:*, st1:*) – strip entirely with content.
    static RE_OFFICE_PAIRED: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)<(?:o|w|x|st1):[a-z]+[^>]*>.*?</(?:o|w|x|st1):[a-z]+\s*>").unwrap()
    });
    let out = RE_OFFICE_PAIRED.replace_all(&out, "").into_owned();

    // Self-closing Office tags
    static RE_OFFICE_SELF: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)<(?:o|w|x|st1):[a-z]+[^>]*/\s*>").unwrap()
    });
    RE_OFFICE_SELF.replace_all(&out, "").into_owned()
}

/// Strip `mso-*` CSS properties from `<style>` blocks.
fn strip_mso_css_in_style_blocks(html: &str) -> String {
    static RE_STYLE_BLOCK: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)(<style[^>]*>)(.*?)(</style\s*>)").unwrap()
    });
    static RE_MSO_PROP: Lazy<Regex> = Lazy::new(|| {
        // Match "mso-something: value;" or "mso-something: value}" (before closing brace)
        Regex::new(r#"(?i)\bmso-[a-z\-]+\s*:[^;}"]*;?"#).unwrap()
    });

    RE_STYLE_BLOCK
        .replace_all(html, |caps: &Captures| {
            let open = &caps[1];
            let css = &caps[2];
            let close = &caps[3];
            let cleaned = RE_MSO_PROP.replace_all(css, "");
            format!("{}{}{}", open, cleaned, close)
        })
        .into_owned()
}

/// Strip `mso-*` CSS properties from inline `style="..."` attributes.
fn strip_mso_css_inline(html: &str) -> String {
    static RE_INLINE_STYLE: Lazy<Regex> = Lazy::new(|| {
        // Match style="..." capturing the value between quotes
        Regex::new(concat!(
            r#"(?is)(style\s*=\s*")"#,  // group 1: style="
            r#"([^"]*)"#,               // group 2: value
            r#"""#,                      // closing quote
        )).unwrap()
    });
    static RE_MSO_PROP: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)\bmso-[a-z\-]+\s*:[^;]*;?\s*").unwrap()
    });

    RE_INLINE_STYLE
        .replace_all(html, |caps: &Captures| {
            let prefix = &caps[1];
            let css = &caps[2];
            let cleaned = RE_MSO_PROP.replace_all(css, "");
            format!("{}{}\"", prefix, cleaned)
        })
        .into_owned()
}

// ---------------------------------------------------------------------------
// Phase 4: Default styles injection
// ---------------------------------------------------------------------------

/// Inject a default `<style>` block into `<head>` (or before `<body>`, or at the
/// very start if neither is found).
fn inject_default_styles(html: &str) -> String {
    // Try to inject right after <head...>
    static RE_HEAD: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)(<head[^>]*>)").unwrap());
    if let Some(m) = RE_HEAD.find(html) {
        let pos = m.end();
        let mut out = String::with_capacity(html.len() + EMAIL_DEFAULT_STYLES.len() + 1);
        out.push_str(&html[..pos]);
        out.push('\n');
        out.push_str(EMAIL_DEFAULT_STYLES);
        out.push('\n');
        out.push_str(&html[pos..]);
        return out;
    }

    // No <head>? Try before <body>
    static RE_BODY: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)(<body[^>]*>)").unwrap());
    if let Some(m) = RE_BODY.find(html) {
        let pos = m.start();
        let mut out = String::with_capacity(html.len() + EMAIL_DEFAULT_STYLES.len() + 1);
        out.push_str(&html[..pos]);
        out.push_str(EMAIL_DEFAULT_STYLES);
        out.push('\n');
        out.push_str(&html[pos..]);
        return out;
    }

    // No head or body? Prepend.
    format!("{}\n{}", EMAIL_DEFAULT_STYLES, html)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Append CSS text to an existing `style="…"` attribute inside a tag string,
/// or insert a new `style` attribute right before the closing `>`.
fn inject_style(tag: &str, css: &str) -> String {
    static RE_STYLE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?i)style\s*=\s*""#).unwrap());

    if let Some(m) = RE_STYLE.find(tag) {
        // Existing style="..." – append our CSS right after the opening quote.
        let pos = m.end(); // just past the "
        let (before, after) = tag.split_at(pos);
        format!("{}{} {}", before, css, after)
    } else {
        // No style attribute – add one before the closing >.
        let close = tag.rfind('>').unwrap_or(tag.len());
        let (before, after) = tag.split_at(close);
        format!("{} style=\"{}\"{}",  before, css, after)
    }
}

/// If the value is purely numeric, append "px".
/// Preserves percentage values (e.g. "100%") as-is.
fn px(val: &str) -> String {
    let v = val.trim().trim_end_matches("px");
    if v.ends_with('%') {
        // Percentage value – keep as-is
        v.to_string()
    } else if v.chars().all(|c| c.is_ascii_digit()) {
        format!("{}px", v)
    } else {
        v.to_string()
    }
}

// ---------------------------------------------------------------------------
// Individual attribute converters
// ---------------------------------------------------------------------------

// 1. bgcolor="..." on any element → background-color
fn convert_bgcolor(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?is)(<[a-z][a-z0-9]*\b[^>]*?)\s+bgcolor\s*=\s*"([^"]*)"([^>]*>)"#)
            .unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag = format!("{}{}", &caps[1], &caps[3]);
        let color = &caps[2];
        inject_style(&tag, &format!("background-color: {};", color))
    })
    .into_owned()
}

// 10. background="url" on td/body/table → background-image
fn convert_background(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)(<(?:td|th|body|table)\b[^>]*?)\s+background\s*=\s*"([^"]*)"([^>]*>)"#,
        )
        .unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag = format!("{}{}", &caps[1], &caps[3]);
        let url = &caps[2];
        inject_style(&tag, &format!("background-image: url('{}');", url))
    })
    .into_owned()
}

// 2. width="..." on table/td/th/img
fn convert_width(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)(<(?:table|td|th|img)\b[^>]*?)\s+width\s*=\s*"([^"]*)"([^>]*>)"#,
        )
        .unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag = format!("{}{}", &caps[1], &caps[3]);
        inject_style(&tag, &format!("width: {};", px(&caps[2])))
    })
    .into_owned()
}

// 3. height="..." on table/td/th/img/tr
fn convert_height(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)(<(?:table|td|th|img|tr)\b[^>]*?)\s+height\s*=\s*"([^"]*)"([^>]*>)"#,
        )
        .unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag = format!("{}{}", &caps[1], &caps[3]);
        inject_style(&tag, &format!("height: {};", px(&caps[2])))
    })
    .into_owned()
}

// 4 & 5. align="..." on table/div/p/td/th
fn convert_align(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)(<(table|div|p|td|th|tr)\b[^>]*?)\s+align\s*=\s*"([^"]*)"([^>]*>)"#,
        )
        .unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag_name = caps[2].to_ascii_lowercase();
        let value = caps[3].to_ascii_lowercase();
        let tag = format!("{}{}", &caps[1], &caps[4]);

        let css = match (tag_name.as_str(), value.as_str()) {
            ("td" | "th", v) => format!("text-align: {};", v),
            (_, "center") => "margin-left: auto; margin-right: auto; text-align: center;".into(),
            (_, v) => format!("text-align: {};", v),
        };
        inject_style(&tag, &css)
    })
    .into_owned()
}

// 6. valign="..." on td/th/tr
fn convert_valign(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)(<(?:td|th|tr)\b[^>]*?)\s+valign\s*=\s*"([^"]*)"([^>]*>)"#,
        )
        .unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag = format!("{}{}", &caps[1], &caps[3]);
        inject_style(&tag, &format!("vertical-align: {};", &caps[2]))
    })
    .into_owned()
}

// 8. cellspacing="..." on table → border-spacing
fn convert_cellspacing(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)(<table\b[^>]*?)\s+cellspacing\s*=\s*"([^"]*)"([^>]*>)"#,
        )
        .unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag = format!("{}{}", &caps[1], &caps[3]);
        inject_style(&tag, &format!("border-spacing: {};", px(&caps[2])))
    })
    .into_owned()
}

// 7. cellpadding="..." on table → best-effort: just put padding on the table
//    (ideally it goes on child td/th, but that requires DOM manipulation)
fn convert_cellpadding(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)(<table\b[^>]*?)\s+cellpadding\s*=\s*"([^"]*)"([^>]*>)"#,
        )
        .unwrap()
    });
    // We inject a CSS custom property AND a direct style hint.  Since we can't
    // easily push styles into child tds with regex alone, we set it on the
    // table and also inject a <style> block later (see bottom of preprocess fn).
    // For now, best-effort: strip the attribute so it doesn't confuse the parser.
    RE.replace_all(html, |caps: &Captures| {
        let tag = format!("{}{}", &caps[1], &caps[3]);
        // We can't target children with regex, so we add padding to the table
        // itself as a rough approximation.  Many email renderers do the same.
        inject_style(&tag, &format!("padding: {};", px(&caps[2])))
    })
    .into_owned()
}

// 9. border="..." on table
fn convert_border(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)(<table\b[^>]*?)\s+border\s*=\s*"([^"]*)"([^>]*>)"#,
        )
        .unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag = format!("{}{}", &caps[1], &caps[3]);
        let val = caps[2].trim();
        if val == "0" {
            inject_style(&tag, "border-collapse: collapse; border: 0;")
        } else {
            inject_style(&tag, &format!("border: {} solid;", px(val)))
        }
    })
    .into_owned()
}

// Phase 4: Force border removal on table elements
fn force_no_borders(html: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)<(table|td|th|tr)\b[^>]*>").unwrap()
    });
    RE.replace_all(html, |caps: &Captures| {
        let tag = &caps[0];
        inject_style(tag, "border:0 !important;border-style:none !important;")
    })
    .into_owned()
}

// 11. <center> → <div style="text-align:center;">
fn rewrite_center_tag(html: &str) -> String {
    static RE_OPEN: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<center(\s[^>]*)?>").unwrap());
    static RE_CLOSE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)</center\s*>").unwrap());

    let out = RE_OPEN.replace_all(html, |caps: &Captures| {
        let attrs = caps.get(1).map_or("", |m| m.as_str());
        format!("<div{} style=\"text-align: center;\">", attrs)
    });
    RE_CLOSE.replace_all(&out, "</div>").into_owned()
}

// 12. <font color="..." size="..." face="..."> → <span style="...">
fn rewrite_font_tag(html: &str) -> String {
    static RE_OPEN: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<font\b([^>]*)>").unwrap());
    static RE_CLOSE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)</font\s*>").unwrap());
    static RE_COLOR: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?i)color\s*=\s*"([^"]*)""#).unwrap());
    static RE_SIZE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?i)size\s*=\s*"([^"]*)""#).unwrap());
    static RE_FACE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?i)face\s*=\s*"([^"]*)""#).unwrap());

    let out = RE_OPEN.replace_all(html, |caps: &Captures| {
        let attrs = &caps[1];
        let mut css = String::new();

        if let Some(c) = RE_COLOR.captures(attrs) {
            css.push_str(&format!("color: {};", &c[1]));
        }
        if let Some(c) = RE_FACE.captures(attrs) {
            css.push_str(&format!("font-family: {};", &c[1]));
        }
        if let Some(c) = RE_SIZE.captures(attrs) {
            // HTML font size 1-7 → approximate px
            let sz = match c[1].trim() {
                "1" => "10px",
                "2" => "13px",
                "3" => "16px",
                "4" => "18px",
                "5" => "24px",
                "6" => "32px",
                "7" => "48px",
                other => {
                    // +N / -N relative sizes: just pass through as-is with px
                    if other.starts_with('+') || other.starts_with('-') {
                        // Can't do relative in inline CSS easily; skip.
                        ""
                    } else {
                        ""
                    }
                }
            };
            if !sz.is_empty() {
                css.push_str(&format!("font-size: {};", sz));
            }
        }

        if css.is_empty() {
            "<span>".to_string()
        } else {
            format!("<span style=\"{}\">", css)
        }
    });

    RE_CLOSE.replace_all(&out, "</span>").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgcolor_converted() {
        let input = r##"<td bgcolor="#ffffff">hello</td>"##;
        let out = preprocess_email_html(input);
        assert!(out.contains("background-color: #ffffff;"));
        assert!(!out.contains("bgcolor"));
    }

    #[test]
    fn width_gets_px() {
        let input = r#"<table width="600">x</table>"#;
        let out = preprocess_email_html(input);
        assert!(out.contains("width: 600px;"));
    }

    #[test]
    fn width_percentage_preserved() {
        let input = r#"<table width="100%">x</table>"#;
        let out = preprocess_email_html(input);
        assert!(out.contains("width: 100%;"));
    }

    #[test]
    fn center_tag_rewritten() {
        let input = "<center>hi</center>";
        let out = preprocess_email_html(input);
        assert!(out.contains("text-align: center;"));
        assert!(!out.contains("<center"));
    }

    #[test]
    fn font_tag_rewritten() {
        let input = r#"<font color="red" size="4" face="Arial">hi</font>"#;
        let out = preprocess_email_html(input);
        assert!(out.contains("color: red;"));
        assert!(out.contains("font-size: 18px;"));
        assert!(out.contains("font-family: Arial;"));
        assert!(out.contains("<span"));
        assert!(out.contains("</span>"));
    }

    #[test]
    fn existing_style_preserved() {
        let input = r##"<td style="color: red;" bgcolor="#000">x</td>"##;
        let out = preprocess_email_html(input);
        assert!(out.contains("color: red;"));
        assert!(out.contains("background-color: #000;"));
    }

    #[test]
    fn mso_conditional_comments_stripped() {
        let input = r#"<html><body><!--[if mso]><table><tr><td>outlook only</td></tr></table><![endif]--><p>visible</p></body></html>"#;
        let out = preprocess_email_html(input);
        assert!(!out.contains("outlook only"));
        assert!(out.contains("visible"));
    }

    #[test]
    fn mso_gte_conditional_stripped() {
        let input = r#"before<!--[if gte mso 9]><xml><o:shapedefaults></o:shapedefaults></xml><![endif]-->after"#;
        let out = preprocess_email_html(input);
        assert!(!out.contains("shapedefaults"));
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }

    #[test]
    fn not_mso_content_preserved() {
        let input = r#"<!--[if !mso]><!--><p>for everyone</p><!--<![endif]-->"#;
        let out = preprocess_email_html(input);
        assert!(out.contains("for everyone"));
        assert!(!out.contains("<!--[if"));
    }

    #[test]
    fn vml_tags_stripped() {
        let input = r#"<p>hello</p><v:rect style="width:100px"><v:fill type="tile"/></v:rect><p>world</p>"#;
        let out = preprocess_email_html(input);
        assert!(!out.contains("<v:"));
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
    }

    #[test]
    fn office_p_tag_converted() {
        let input = r#"<p>text<o:p>&nbsp;</o:p></p>"#;
        let out = preprocess_email_html(input);
        assert!(!out.contains("<o:p"));
        assert!(out.contains("<span>"));
    }

    #[test]
    fn mso_css_stripped_from_style_block() {
        let input = r#"<style>p { mso-style-name: "Normal"; color: red; mso-line-height-rule: exactly; }</style>"#;
        let out = preprocess_email_html(input);
        assert!(out.contains("color: red;"));
        assert!(!out.contains("mso-style-name"));
        assert!(!out.contains("mso-line-height-rule"));
    }

    #[test]
    fn mso_css_stripped_from_inline_style() {
        let input = r#"<p style="mso-line-height-rule: exactly; font-size: 14px; mso-style-name: test;">hi</p>"#;
        let out = preprocess_email_html(input);
        assert!(out.contains("font-size: 14px;"));
        assert!(!out.contains("mso-line-height-rule"));
        assert!(!out.contains("mso-style-name"));
    }

    #[test]
    fn xml_processing_instruction_stripped() {
        let input = r#"<?xml version="1.0" encoding="UTF-8"?><html><body>hi</body></html>"#;
        let out = preprocess_email_html(input);
        assert!(!out.contains("<?xml"));
        assert!(out.contains("<html>"));
    }

    #[test]
    fn default_styles_injected() {
        let input = "<html><head></head><body>hi</body></html>";
        let out = preprocess_email_html(input);
        assert!(out.contains("border-collapse: collapse"));
        assert!(out.contains("img { border: 0;"));
    }

    #[test]
    fn default_styles_injected_no_head() {
        let input = "<html><body>hi</body></html>";
        let out = preprocess_email_html(input);
        assert!(out.contains("border-collapse: collapse"));
        // Style should appear before <body>
        let style_pos = out.find("border-collapse").unwrap();
        let body_pos = out.find("<body>").unwrap();
        assert!(style_pos < body_pos);
    }
}
