//! Row types and the parser for the credit tables in `contributors/*.tsv` (format:
//! `../craftrules/standards/contributors.md`): tab-separated, UTF-8, a header row, `#` comment
//! lines and blank lines allowed.
//!
//! This file is std-only and shared verbatim (via `#[path]`) by `build.rs`, which compiles the
//! tables into the app, and by `cargo xtask contributors`, which reads `models.tsv`. Every parser
//! returns `Err` (never panics) on a malformed file; callers then fall back to an empty table.

/// `commits.tsv`: one commit on the default branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit<'a> {
    pub sha: &'a str,
    pub url: &'a str,
    /// GitHub login, or `unknown` when the author has no GitHub account.
    pub login: &'a str,
    /// Committer date, ISO 8601 UTC (`2026-10-06T15:30:10Z`): compares in time order as text.
    pub date: &'a str,
    pub additions: u32,
    pub deletions: u32,
    pub pr: Option<u32>,
    /// Model ids from `models.tsv`, `;`-separated (empty: no model trailer).
    pub models: &'a str,
}

/// `prs.tsv`: one merged pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pr<'a> {
    pub number: u32,
    pub url: &'a str,
    pub login: &'a str,
    pub merged_at: &'a str,
    pub additions: u32,
    pub deletions: u32,
    pub commits: u32,
    pub models: &'a str,
}

/// `models.tsv`: one `Co-Authored-By` trailer name mapped to a model id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model<'a> {
    pub id: &'a str,
    pub company: &'a str,
    pub model: &'a str,
    pub version: &'a str,
    pub trailer: &'a str,
}

/// `people.tsv`: consent and the opt-in name fields of one login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person<'a> {
    pub login: &'a str,
    pub consent: bool,
    pub consent_date: &'a str,
    pub consent_source: &'a str,
    pub display_name: &'a str,
    pub real_name: &'a str,
    pub email: &'a str,
}

pub const COMMITS_HEADER: &[&str] = &["sha", "url", "login", "date", "additions", "deletions", "pr", "models"];
pub const PRS_HEADER: &[&str] = &["number", "url", "login", "merged_at", "additions", "deletions", "commits", "models"];
pub const MODELS_HEADER: &[&str] = &["id", "company", "model", "version", "trailer"];
pub const PEOPLE_HEADER: &[&str] = &["login", "consent", "consent_date", "consent_source", "display_name", "real_name", "email"];

/// The data rows of a TSV with exactly the columns `header`, as (line number, fields).
fn rows<'a>(text: &'a str, header: &[&str]) -> Result<Vec<(usize, Vec<&'a str>)>, String> {
    let mut out = Vec::new();
    let mut seen_header = false;
    for (i, line) in text.lines().enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if !seen_header {
            if fields != header {
                return Err(format!("line {}: expected the header `{}`", i + 1, header.join("\\t")));
            }
            seen_header = true;
            continue;
        }
        if fields.len() != header.len() {
            return Err(format!("line {}: {} fields, expected {}", i + 1, fields.len(), header.len()));
        }
        out.push((i + 1, fields));
    }
    if !seen_header {
        return Err("no header row".into());
    }
    Ok(out)
}

fn num(line: usize, what: &str, s: &str) -> Result<u32, String> {
    s.parse().map_err(|_| format!("line {line}: `{what}` is not a number: `{s}`"))
}

fn required<'a>(line: usize, what: &str, s: &'a str) -> Result<&'a str, String> {
    if s.trim().is_empty() { Err(format!("line {line}: `{what}` is empty")) } else { Ok(s) }
}

pub fn parse_commits(text: &str) -> Result<Vec<Commit<'_>>, String> {
    rows(text, COMMITS_HEADER)?
        .into_iter()
        .map(|(l, f)| {
            Ok(Commit {
                sha: required(l, "sha", f[0])?,
                url: f[1],
                login: required(l, "login", f[2])?,
                date: required(l, "date", f[3])?,
                additions: num(l, "additions", f[4])?,
                deletions: num(l, "deletions", f[5])?,
                pr: if f[6].is_empty() { None } else { Some(num(l, "pr", f[6])?) },
                models: f[7],
            })
        })
        .collect()
}

pub fn parse_prs(text: &str) -> Result<Vec<Pr<'_>>, String> {
    rows(text, PRS_HEADER)?
        .into_iter()
        .map(|(l, f)| {
            Ok(Pr {
                number: num(l, "number", f[0])?,
                url: f[1],
                login: required(l, "login", f[2])?,
                merged_at: required(l, "merged_at", f[3])?,
                additions: num(l, "additions", f[4])?,
                deletions: num(l, "deletions", f[5])?,
                commits: num(l, "commits", f[6])?,
                models: f[7],
            })
        })
        .collect()
}

pub fn parse_models(text: &str) -> Result<Vec<Model<'_>>, String> {
    rows(text, MODELS_HEADER)?
        .into_iter()
        .map(|(l, f)| Ok(Model { id: required(l, "id", f[0])?, company: f[1], model: f[2], version: f[3], trailer: required(l, "trailer", f[4])? }))
        .collect()
}

pub fn parse_people(text: &str) -> Result<Vec<Person<'_>>, String> {
    rows(text, PEOPLE_HEADER)?
        .into_iter()
        .map(|(l, f)| {
            let consent = match f[1] {
                "yes" => true,
                "no" => false,
                other => return Err(format!("line {l}: `consent` must be yes or no, not `{other}`")),
            };
            Ok(Person {
                login: required(l, "login", f[0])?,
                consent,
                consent_date: f[2],
                consent_source: f[3],
                display_name: f[4],
                real_name: f[5],
                email: f[6],
            })
        })
        .collect()
}

/// The model ids of a `models` cell.
pub fn model_ids(cell: &str) -> impl Iterator<Item = &str> {
    cell.split(';').map(str::trim).filter(|s| !s.is_empty())
}
