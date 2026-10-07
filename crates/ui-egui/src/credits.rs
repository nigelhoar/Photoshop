//! The About window's credits: who built PhotoCraft and which AI models helped. Pure data and
//! logic (aggregation, sorting, name fallback); `about_ui` only draws the rows.
//!
//! The tables are compiled in by `build.rs` from `contributors/*.tsv` (refreshed by
//! `cargo xtask contributors`; standard: `../craftrules/standards/contributors.md`). Privacy: a
//! contributor is shown by GitHub login unless `people.tsv` records their consent to show a
//! display or real name.

use std::collections::{BTreeMap, BTreeSet};

pub use crate::credits_data::{Commit, Model, Person, Pr, model_ids};

include!(concat!(env!("OUT_DIR"), "/credits_tables.rs"));

/// One repository's credit tables.
#[derive(Clone, Copy, Debug)]
pub struct Credits<'a> {
    pub commits: &'a [Commit<'a>],
    pub prs: &'a [Pr<'a>],
    pub models: &'a [Model<'a>],
    pub people: &'a [Person<'a>],
}

impl Credits<'static> {
    /// The tables compiled into this build.
    pub fn compiled() -> Self {
        Credits { commits: COMMITS, prs: PRS, models: MODELS, people: PEOPLE }
    }
}

/// Which name the Contributors table shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NameMode {
    #[default]
    Login,
    Display,
    Real,
}

impl NameMode {
    pub const ALL: [NameMode; 3] = [NameMode::Login, NameMode::Display, NameMode::Real];
    pub fn key(self) -> &'static str {
        match self {
            NameMode::Login => "login",
            NameMode::Display => "display",
            NameMode::Real => "real",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            NameMode::Login => "GitHub username",
            NameMode::Display => "Display name",
            NameMode::Real => "Real name",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == s)
    }
}

/// A contributor's row: everything aggregated over their commits and merged PRs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContributorRow<'a> {
    pub login: &'a str,
    /// Only with consent (`people.tsv`), otherwise `None`.
    pub display_name: Option<&'a str>,
    pub real_name: Option<&'a str>,
    /// Earliest / latest commit date or PR merge (ISO 8601).
    pub first: &'a str,
    pub last: &'a str,
    pub prs: u32,
    pub commits: u32,
    pub additions: u64,
    pub deletions: u64,
    pub first_merge: Option<&'a str>,
    pub last_merge: Option<&'a str>,
}

impl ContributorRow<'_> {
    /// The name shown for `mode`; falls back to `@login` when that name isn't available.
    pub fn name(&self, mode: NameMode) -> String {
        let chosen = match mode {
            NameMode::Login => None,
            NameMode::Display => self.display_name,
            NameMode::Real => self.real_name,
        };
        match chosen {
            Some(n) => n.to_string(),
            None => format!("@{}", self.login),
        }
    }
}

/// The name sort key: case-insensitive, ignoring a leading `@`.
pub fn name_key(name: &str) -> String {
    name.trim_start_matches('@').to_lowercase()
}

/// Sortable Contributors columns, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContributorCol {
    Name,
    First,
    Last,
    Prs,
    Commits,
    Added,
    Removed,
    FirstMerge,
    LastMerge,
}

impl ContributorCol {
    pub const ALL: [ContributorCol; 9] = [
        ContributorCol::Name,
        ContributorCol::First,
        ContributorCol::Last,
        ContributorCol::Prs,
        ContributorCol::Commits,
        ContributorCol::Added,
        ContributorCol::Removed,
        ContributorCol::FirstMerge,
        ContributorCol::LastMerge,
    ];
    pub fn key(self) -> &'static str {
        match self {
            ContributorCol::Name => "name",
            ContributorCol::First => "first",
            ContributorCol::Last => "last",
            ContributorCol::Prs => "prs",
            ContributorCol::Commits => "commits",
            ContributorCol::Added => "added",
            ContributorCol::Removed => "removed",
            ContributorCol::FirstMerge => "firstMerge",
            ContributorCol::LastMerge => "lastMerge",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ContributorCol::Name => "Name",
            ContributorCol::First => "First contribution",
            ContributorCol::Last => "Last contribution",
            ContributorCol::Prs => "Merged PRs",
            ContributorCol::Commits => "Commits",
            ContributorCol::Added => "Lines added",
            ContributorCol::Removed => "Lines removed",
            ContributorCol::FirstMerge => "First merge",
            ContributorCol::LastMerge => "Last merge",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == s)
    }
}

fn is_bot(login: &str) -> bool {
    login.ends_with("[bot]")
}

fn min_max<'a>(slot: &mut Option<(&'a str, &'a str)>, d: &'a str) {
    *slot = Some(match *slot {
        None => (d, d),
        Some((lo, hi)) => (lo.min(d), hi.max(d)),
    });
}

/// One row per login (bots left out), in login order.
pub fn contributors<'a>(c: &Credits<'a>) -> Vec<ContributorRow<'a>> {
    #[derive(Default)]
    struct Acc<'a> {
        span: Option<(&'a str, &'a str)>,
        merges: Option<(&'a str, &'a str)>,
        prs: u32,
        commits: u32,
        additions: u64,
        deletions: u64,
    }
    let mut by: BTreeMap<&'a str, Acc<'a>> = BTreeMap::new();
    for k in c.commits.iter().filter(|k| !is_bot(k.login)) {
        let a = by.entry(k.login).or_default();
        min_max(&mut a.span, k.date);
        a.commits += 1;
        a.additions += u64::from(k.additions);
        a.deletions += u64::from(k.deletions);
    }
    for p in c.prs.iter().filter(|p| !is_bot(p.login)) {
        let a = by.entry(p.login).or_default();
        min_max(&mut a.span, p.merged_at);
        min_max(&mut a.merges, p.merged_at);
        a.prs += 1;
    }
    by.into_iter()
        .map(|(login, a)| {
            let person = c.people.iter().find(|p| p.login == login && p.consent);
            let opt = |s: &'a str| (!s.trim().is_empty()).then_some(s);
            let (first, last) = a.span.unwrap_or(("", ""));
            ContributorRow {
                login,
                display_name: person.and_then(|p| opt(p.display_name)),
                real_name: person.and_then(|p| opt(p.real_name)),
                first,
                last,
                prs: a.prs,
                commits: a.commits,
                additions: a.additions,
                deletions: a.deletions,
                first_merge: a.merges.map(|m| m.0),
                last_merge: a.merges.map(|m| m.1),
            }
        })
        .collect()
}

/// Sort by `col` (descending if `desc`); ties by name, ascending.
pub fn sort_contributors(rows: &mut [ContributorRow<'_>], col: ContributorCol, desc: bool, mode: NameMode) {
    let by_name = |a: &ContributorRow<'_>, b: &ContributorRow<'_>| name_key(&a.name(mode)).cmp(&name_key(&b.name(mode))).then(a.login.cmp(b.login));
    rows.sort_by(|a, b| {
        let o = match col {
            ContributorCol::Name => by_name(a, b),
            ContributorCol::First => a.first.cmp(b.first),
            ContributorCol::Last => a.last.cmp(b.last),
            ContributorCol::Prs => a.prs.cmp(&b.prs),
            ContributorCol::Commits => a.commits.cmp(&b.commits),
            ContributorCol::Added => a.additions.cmp(&b.additions),
            ContributorCol::Removed => a.deletions.cmp(&b.deletions),
            ContributorCol::FirstMerge => a.first_merge.cmp(&b.first_merge),
            ContributorCol::LastMerge => a.last_merge.cmp(&b.last_merge),
        };
        let o = if desc { o.reverse() } else { o };
        o.then_with(|| by_name(a, b))
    });
}

/// A model's row in the Models table.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelRow<'a> {
    pub id: &'a str,
    pub company: &'a str,
    pub model: &'a str,
    pub version: &'a str,
    pub commits: u32,
    /// Share of the commits that name any model (%).
    pub share_assisted: f64,
    /// Share of all commits (%).
    pub share_all: f64,
    pub prs: u32,
    pub additions: u64,
    pub deletions: u64,
}

/// Sortable Models columns, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelCol {
    Company,
    Model,
    Version,
    Commits,
    ShareAssisted,
    ShareAll,
    Prs,
    Added,
    Removed,
}

impl ModelCol {
    pub const ALL: [ModelCol; 9] = [
        ModelCol::Company,
        ModelCol::Model,
        ModelCol::Version,
        ModelCol::Commits,
        ModelCol::ShareAssisted,
        ModelCol::ShareAll,
        ModelCol::Prs,
        ModelCol::Added,
        ModelCol::Removed,
    ];
    pub fn key(self) -> &'static str {
        match self {
            ModelCol::Company => "company",
            ModelCol::Model => "model",
            ModelCol::Version => "version",
            ModelCol::Commits => "commits",
            ModelCol::ShareAssisted => "shareAssisted",
            ModelCol::ShareAll => "shareAll",
            ModelCol::Prs => "prs",
            ModelCol::Added => "added",
            ModelCol::Removed => "removed",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ModelCol::Company => "Company",
            ModelCol::Model => "Model",
            ModelCol::Version => "Version",
            ModelCol::Commits => "Commits",
            ModelCol::ShareAssisted => "% of AI-assisted",
            ModelCol::ShareAll => "% of all commits",
            ModelCol::Prs => "PRs",
            ModelCol::Added => "Lines added",
            ModelCol::Removed => "Lines removed",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == s)
    }
}

/// One row per model id named by any commit or listed in `models.tsv`, in id order.
pub fn models<'a>(c: &Credits<'a>) -> Vec<ModelRow<'a>> {
    let mut ids: BTreeSet<&'a str> = c.models.iter().map(|m| m.id).collect();
    for k in c.commits {
        ids.extend(model_ids(k.models));
    }
    let total = c.commits.len();
    let assisted = c.commits.iter().filter(|k| model_ids(k.models).next().is_some()).count();
    let pct = |n: u32, of: usize| if of == 0 { 0.0 } else { f64::from(n) * 100.0 / of as f64 };
    ids.into_iter()
        .map(|id| {
            let info = c.models.iter().find(|m| m.id == id);
            let mut row = ModelRow {
                id,
                company: info.map_or("", |m| m.company),
                model: info.map_or(id, |m| m.model),
                version: info.map_or("", |m| m.version),
                commits: 0,
                share_assisted: 0.0,
                share_all: 0.0,
                prs: 0,
                additions: 0,
                deletions: 0,
            };
            for k in c.commits.iter().filter(|k| model_ids(k.models).any(|m| m == id)) {
                row.commits += 1;
                row.additions += u64::from(k.additions);
                row.deletions += u64::from(k.deletions);
            }
            row.prs = c.prs.iter().filter(|p| model_ids(p.models).any(|m| m == id)).count() as u32;
            row.share_assisted = pct(row.commits, assisted);
            row.share_all = pct(row.commits, total);
            row
        })
        .collect()
}

/// Sort by `col` (descending if `desc`); ties by company, model, version, id.
pub fn sort_models(rows: &mut [ModelRow<'_>], col: ModelCol, desc: bool) {
    let text = |a: &ModelRow<'_>, b: &ModelRow<'_>| {
        a.company
            .to_lowercase()
            .cmp(&b.company.to_lowercase())
            .then(a.model.to_lowercase().cmp(&b.model.to_lowercase()))
            .then(a.version.cmp(b.version))
            .then(a.id.cmp(b.id))
    };
    rows.sort_by(|a, b| {
        let o = match col {
            ModelCol::Company => text(a, b),
            ModelCol::Model => a.model.to_lowercase().cmp(&b.model.to_lowercase()),
            ModelCol::Version => a.version.cmp(b.version),
            ModelCol::Commits => a.commits.cmp(&b.commits),
            ModelCol::ShareAssisted => a.share_assisted.total_cmp(&b.share_assisted),
            ModelCol::ShareAll => a.share_all.total_cmp(&b.share_all),
            ModelCol::Prs => a.prs.cmp(&b.prs),
            ModelCol::Added => a.additions.cmp(&b.additions),
            ModelCol::Removed => a.deletions.cmp(&b.deletions),
        };
        let o = if desc { o.reverse() } else { o };
        o.then_with(|| text(a, b))
    });
}

/// A contributor's merged PRs and commits, newest first (for the links under the table).
pub fn links_for<'a>(c: &Credits<'a>, login: &str) -> (Vec<&'a Pr<'a>>, Vec<&'a Commit<'a>>) {
    let mut prs: Vec<&'a Pr<'a>> = c.prs.iter().filter(|p| p.login == login).collect();
    prs.sort_by(|a, b| b.merged_at.cmp(a.merged_at).then(b.number.cmp(&a.number)));
    let mut commits: Vec<&'a Commit<'a>> = c.commits.iter().filter(|k| k.login == login).collect();
    commits.sort_by(|a, b| b.date.cmp(a.date).then(b.sha.cmp(a.sha)));
    (prs, commits)
}

/// `2026-10-06T15:30:10Z` → `2026-10-06`.
pub fn day(iso: &str) -> &str {
    iso.get(..10).unwrap_or(iso)
}

#[cfg(test)]
#[path = "credits_tests.rs"]
mod tests;
