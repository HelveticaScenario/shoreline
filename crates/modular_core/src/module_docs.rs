//! A module's reference docs: a few-word summary, a short description, its
//! categories, optional titled sections, and runnable examples.
//!
//! Each module declares one `const` [`ModuleDocs`] beside its struct and
//! names it in `#[module(docs = …)]`; that const is the only source of the
//! module's user-facing docs. Params and outputs are documented on their own
//! fields with `#[input]` and `#[output]`, never here. The editor renders the
//! serialized [`ModuleDocsSchema`] two ways: compactly in Monaco's hover and
//! completion, and in full in the Reference pane.
//!
//! The serialized shape matches `DocContent` in `src/shared/dsl/referenceDocs.ts`,
//! which globals, collection methods and video modules share.

use napi_derive::napi;
use serde::{Deserialize, Serialize};

use crate::module_category::ModuleCategory;

pub struct ModuleDocs {
    /// A few words naming what the module is, for reference lists.
    pub summary: &'static str,
    /// What the module is for, in a sentence or three of inline markdown:
    /// no headings, tables or code blocks.
    pub description: &'static str,
    pub categories: &'static [ModuleCategory],
    /// Reference material too long for the description, such as a syntax
    /// table. Most modules have none.
    pub sections: &'static [DocSection],
    pub examples: &'static [DocExample],
}

pub struct DocSection {
    pub title: &'static str,
    /// Inline markdown; may be empty when the table says it all.
    pub body: &'static str,
    pub table: Option<DocTable>,
    pub examples: &'static [DocExample],
}

/// Rows of inline-markdown cells. The first column is each row's key, which
/// narrow layouts keep as the row's heading.
pub struct DocTable {
    pub columns: &'static [&'static str],
    pub rows: &'static [&'static [&'static str]],
}

pub struct DocExample {
    /// What you hear, e.g. "Acid bass sweep".
    pub title: &'static str,
    /// A complete patch that runs as written. A multi-line example is a raw
    /// string opening on its own line and indented with the source; the
    /// schema drops that indent and the surrounding blank lines.
    pub code: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct ModuleDocsSchema {
    pub summary: String,
    pub description: String,
    /// Typed as strings in TypeScript so `schemas.json` imports, whose string
    /// literals widen, still satisfy this interface.
    #[napi(ts_type = "Array<string>")]
    pub categories: Vec<ModuleCategory>,
    pub sections: Vec<DocSectionSchema>,
    pub examples: Vec<DocExampleSchema>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct DocSectionSchema {
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<DocTableSchema>,
    pub examples: Vec<DocExampleSchema>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct DocTableSchema {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct DocExampleSchema {
    pub title: String,
    pub code: String,
}

/// `code` without its leading and trailing blank lines or the indent its
/// lines share.
fn dedent(code: &str) -> String {
    let lines: Vec<&str> = code.trim_matches('\n').trim_end().lines().collect();
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or("").trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn examples(examples: &[DocExample]) -> Vec<DocExampleSchema> {
    examples
        .iter()
        .map(|e| DocExampleSchema {
            title: e.title.to_string(),
            code: dedent(e.code),
        })
        .collect()
}

impl From<&ModuleDocs> for ModuleDocsSchema {
    fn from(docs: &ModuleDocs) -> Self {
        Self {
            summary: docs.summary.to_string(),
            description: docs.description.to_string(),
            categories: docs.categories.to_vec(),
            sections: docs
                .sections
                .iter()
                .map(|s| DocSectionSchema {
                    title: s.title.to_string(),
                    body: s.body.to_string(),
                    table: s.table.as_ref().map(|t| DocTableSchema {
                        columns: t.columns.iter().map(|c| c.to_string()).collect(),
                        rows: t
                            .rows
                            .iter()
                            .map(|r| r.iter().map(|c| c.to_string()).collect())
                            .collect(),
                    }),
                    examples: examples(s.examples),
                })
                .collect(),
            examples: examples(docs.examples),
        }
    }
}
