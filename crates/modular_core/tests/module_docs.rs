//! Every module's reference docs follow one shape: a few-word summary, a short
//! inline-markdown description, its categories, well-formed sections and
//! tables, and titled examples. Block markdown never hides in the prose.

use std::collections::HashSet;

use modular_core::dsp::schema;

/// Markdown the editor renders as blocks, which only sections and examples
/// may carry.
fn block_markdown(text: &str) -> Option<&'static str> {
    if text.contains("```") {
        return Some("a code fence");
    }
    for line in text.lines().map(str::trim_start) {
        if line.starts_with('#') {
            return Some("a heading");
        }
        if line.starts_with('|') {
            return Some("a table row");
        }
        if line.starts_with('<') {
            return Some("HTML");
        }
    }
    None
}

#[test]
fn every_module_has_well_formed_docs() {
    let mut problems = Vec::new();
    for module in schema() {
        let name = &module.name;
        let docs = &module.docs;
        let mut fail = |what: String| problems.push(format!("{name}: {what}"));

        let words = docs.summary.split_whitespace().count();
        if docs.summary.contains('\n') || !(1..=6).contains(&words) {
            fail(format!(
                "summary must be one line of 1–6 words: {:?}",
                docs.summary
            ));
        }
        if docs.summary.ends_with('.') {
            fail("summary ends with a period".into());
        }
        if docs.description.trim().is_empty() {
            fail("empty description".into());
        }
        if let Some(what) = block_markdown(&docs.description) {
            fail(format!(
                "description contains {what}; use a section or an example"
            ));
        }

        if docs.categories.is_empty() {
            fail("no categories".into());
        }
        let unique: HashSet<_> = docs.categories.iter().collect();
        if unique.len() != docs.categories.len() {
            fail("duplicate categories".into());
        }

        let mut titles = HashSet::new();
        let examples = docs
            .examples
            .iter()
            .chain(docs.sections.iter().flat_map(|s| s.examples.iter()));
        for example in examples {
            if example.title.trim().is_empty() || example.code.trim().is_empty() {
                fail("an example needs a title and code".into());
            }
            if !titles.insert(example.title.as_str()) {
                fail(format!("duplicate example title {:?}", example.title));
            }
        }

        for section in &docs.sections {
            if section.title.trim().is_empty() {
                fail("a section needs a title".into());
            }
            if section.body.trim().is_empty()
                && section.table.is_none()
                && section.examples.is_empty()
            {
                fail(format!("section {:?} is empty", section.title));
            }
            if let Some(what) = block_markdown(&section.body) {
                fail(format!("section {:?} body contains {what}", section.title));
            }
            if let Some(table) = &section.table {
                if table.columns.len() < 2 || table.rows.is_empty() {
                    fail(format!(
                        "table in {:?} needs 2+ columns and a row",
                        section.title
                    ));
                }
                for row in &table.rows {
                    if row.len() != table.columns.len() || row.iter().any(|c| c.trim().is_empty()) {
                        fail(format!(
                            "table in {:?} has a ragged or empty row",
                            section.title
                        ));
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
