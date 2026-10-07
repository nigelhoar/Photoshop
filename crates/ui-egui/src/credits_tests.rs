use super::*;
use crate::credits_data::{parse_commits, parse_models, parse_people, parse_prs};

const COMMITS_TSV: &str = "# generated\n\
sha\turl\tlogin\tdate\tadditions\tdeletions\tpr\tmodels\n\
a1\thttps://x/a1\tzed\t2026-01-02T00:00:00Z\t10\t1\t1\tanthropic/claude-opus/5.5\n\
b2\thttps://x/b2\tAlice\t2026-01-01T00:00:00Z\t5\t0\t\t\n\
c3\thttps://x/c3\tzed\t2026-01-05T00:00:00Z\t1\t2\t2\tanthropic/claude-opus/5.5;openai/codex/\n\
d4\thttps://x/d4\tdependabot[bot]\t2026-01-06T00:00:00Z\t1\t1\t\t\n\
\n";

const PRS_TSV: &str = "number\turl\tlogin\tmerged_at\tadditions\tdeletions\tcommits\tmodels\n\
1\thttps://x/1\tzed\t2026-01-02T01:00:00Z\t10\t1\t1\tanthropic/claude-opus/5.5\n\
2\thttps://x/2\tzed\t2026-01-05T01:00:00Z\t1\t2\t3\tanthropic/claude-opus/5.5;openai/codex/\n\
3\thttps://x/3\tbob\t2026-01-07T00:00:00Z\t4\t4\t1\t\n";

const MODELS_TSV: &str = "id\tcompany\tmodel\tversion\ttrailer\r\n\
anthropic/claude-opus/5.5\tAnthropic\tClaude Opus\t5.5\tClaude Opus 5.5\r\n\
anthropic/claude-opus/5.5\tAnthropic\tClaude Opus\t5.5\tClaude Opus 5.5 (1M context)\r\n\
openai/codex/\tOpenAI\tCodex\t\tCodex\r\n";

const PEOPLE_TSV: &str = "login\tconsent\tconsent_date\tconsent_source\tdisplay_name\treal_name\temail\n\
zed\tyes\t2026-01-01\towner\tZed Display\tZed Real\t\n\
Alice\tno\t\t\tAli\tAlice Hidden\talice@example.invalid\n";

struct Owned {
    commits: Vec<Commit<'static>>,
    prs: Vec<Pr<'static>>,
    models: Vec<Model<'static>>,
    people: Vec<Person<'static>>,
}

fn fixture() -> Owned {
    Owned {
        commits: parse_commits(COMMITS_TSV).expect("commits parse"),
        prs: parse_prs(PRS_TSV).expect("prs parse"),
        models: parse_models(MODELS_TSV).expect("models parse"),
        people: parse_people(PEOPLE_TSV).expect("people parse"),
    }
}

fn credits(o: &Owned) -> Credits<'_> {
    Credits { commits: &o.commits, prs: &o.prs, models: &o.models, people: &o.people }
}

#[test]
fn good_files_parse_with_comments_blank_lines_and_crlf() {
    let o = fixture();
    assert_eq!(o.commits.len(), 4);
    assert_eq!(o.commits[0].pr, Some(1));
    assert_eq!(o.commits[1].pr, None);
    assert_eq!(model_ids(o.commits[2].models).collect::<Vec<_>>(), ["anthropic/claude-opus/5.5", "openai/codex/"]);
    assert_eq!(o.prs[1].commits, 3);
    assert_eq!(o.models[1].trailer, "Claude Opus 5.5 (1M context)");
    assert_eq!(o.models[2].version, "");
    assert!(o.people[0].consent && !o.people[1].consent);
    assert_eq!(o.people[0].email, "");
}

#[test]
fn malformed_files_are_errors_not_panics() {
    // wrong header, wrong field count, non-numbers, bad consent, empty login, nothing at all
    assert!(parse_commits("sha\turl\n").is_err());
    assert!(parse_commits("sha\turl\tlogin\tdate\tadditions\tdeletions\tpr\tmodels\na\tb\tc\n").is_err());
    assert!(parse_commits("sha\turl\tlogin\tdate\tadditions\tdeletions\tpr\tmodels\na\tu\tl\td\tmany\t0\t\t\n").is_err());
    assert!(parse_commits("sha\turl\tlogin\tdate\tadditions\tdeletions\tpr\tmodels\na\tu\t\td\t1\t0\t\t\n").is_err());
    assert!(parse_prs("number\turl\tlogin\tmerged_at\tadditions\tdeletions\tcommits\tmodels\nx\tu\tl\td\t1\t1\t1\t\n").is_err());
    assert!(parse_people("login\tconsent\tconsent_date\tconsent_source\tdisplay_name\treal_name\temail\nzed\tmaybe\t\t\t\t\t\n").is_err());
    assert!(parse_models("").is_err());
    assert!(parse_models("# only a comment\n").is_err());
    // A header and no rows is a valid, empty table.
    assert_eq!(parse_models("id\tcompany\tmodel\tversion\ttrailer\n").map(|v| v.len()), Ok(0));
}

#[test]
fn missing_data_gives_empty_tables() {
    let c = Credits { commits: &[], prs: &[], models: &[], people: &[] };
    assert!(contributors(&c).is_empty());
    assert!(models(&c).is_empty());
    let (prs, commits) = links_for(&c, "zed");
    assert!(prs.is_empty() && commits.is_empty());
}

#[test]
fn contributors_aggregate_dates_counts_and_lines() {
    let o = fixture();
    let rows = contributors(&credits(&o));
    let logins: Vec<_> = rows.iter().map(|r| r.login).collect();
    assert_eq!(logins, ["Alice", "bob", "zed"], "bots are left out");
    let zed = &rows[2];
    assert_eq!((zed.first, zed.last), ("2026-01-02T00:00:00Z", "2026-01-05T01:00:00Z"));
    assert_eq!((zed.commits, zed.prs, zed.additions, zed.deletions), (2, 2, 11, 3));
    assert_eq!((zed.first_merge, zed.last_merge), (Some("2026-01-02T01:00:00Z"), Some("2026-01-05T01:00:00Z")));
    let alice = &rows[0];
    assert_eq!((alice.commits, alice.prs, alice.first_merge), (1, 0, None));
    let bob = &rows[1];
    assert_eq!((bob.commits, bob.prs, bob.first, bob.last), (0, 1, "2026-01-07T00:00:00Z", "2026-01-07T00:00:00Z"));
}

#[test]
fn names_need_consent_and_fall_back_to_the_login() {
    let o = fixture();
    let rows = contributors(&credits(&o));
    let (alice, zed) = (&rows[0], &rows[2]);
    assert_eq!(zed.name(NameMode::Login), "@zed");
    assert_eq!(zed.name(NameMode::Display), "Zed Display");
    assert_eq!(zed.name(NameMode::Real), "Zed Real");
    // Alice said no: her names exist in the file but are never shown.
    assert_eq!((alice.display_name, alice.real_name), (None, None));
    assert_eq!(alice.name(NameMode::Display), "@Alice");
    assert_eq!(alice.name(NameMode::Real), "@Alice");
}

#[test]
fn sorting_by_name_ignores_case_and_the_at_sign() {
    let o = fixture();
    let mut rows = contributors(&credits(&o));
    sort_contributors(&mut rows, ContributorCol::Name, false, NameMode::Login);
    assert_eq!(rows.iter().map(|r| r.login).collect::<Vec<_>>(), ["Alice", "bob", "zed"]);
    sort_contributors(&mut rows, ContributorCol::Name, true, NameMode::Login);
    assert_eq!(rows.iter().map(|r| r.login).collect::<Vec<_>>(), ["zed", "bob", "Alice"]);
    // Real names: "Zed Real" vs "@Alice" / "@bob": the @ doesn't push them first.
    sort_contributors(&mut rows, ContributorCol::Name, false, NameMode::Real);
    assert_eq!(rows.iter().map(|r| r.login).collect::<Vec<_>>(), ["Alice", "bob", "zed"]);
    assert_eq!(name_key("@Bob"), "bob");
}

#[test]
fn sorting_by_columns_both_ways() {
    let o = fixture();
    let mut rows = contributors(&credits(&o));
    sort_contributors(&mut rows, ContributorCol::First, false, NameMode::Login);
    assert_eq!(rows.iter().map(|r| r.login).collect::<Vec<_>>(), ["Alice", "zed", "bob"], "default: founding contributor first");
    sort_contributors(&mut rows, ContributorCol::First, true, NameMode::Login);
    assert_eq!(rows.iter().map(|r| r.login).collect::<Vec<_>>(), ["bob", "zed", "Alice"]);
    sort_contributors(&mut rows, ContributorCol::Commits, true, NameMode::Login);
    assert_eq!(rows[0].login, "zed");
    // Ties fall back to the name, ascending, whichever way the column sorts.
    sort_contributors(&mut rows, ContributorCol::Prs, true, NameMode::Login);
    assert_eq!(rows.iter().map(|r| r.login).collect::<Vec<_>>(), ["zed", "bob", "Alice"]);
    sort_contributors(&mut rows, ContributorCol::FirstMerge, false, NameMode::Login);
    assert_eq!(rows[0].login, "Alice", "no merges sorts first ascending");
    for col in ContributorCol::ALL {
        assert_eq!(ContributorCol::from_key(col.key()), Some(col));
    }
}

#[test]
fn model_shares_counts_and_lines() {
    let o = fixture();
    let mut rows = models(&credits(&o));
    assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), ["anthropic/claude-opus/5.5", "openai/codex/"]);
    let opus = &rows[0];
    assert_eq!((opus.company, opus.model, opus.version), ("Anthropic", "Claude Opus", "5.5"));
    assert_eq!((opus.commits, opus.prs, opus.additions, opus.deletions), (2, 2, 11, 3));
    // 2 of the 2 model-assisted commits; 2 of all 4.
    assert!((opus.share_assisted - 100.0).abs() < 1e-9 && (opus.share_all - 50.0).abs() < 1e-9);
    let codex = &rows[1];
    assert_eq!((codex.commits, codex.prs), (1, 1));
    assert!((codex.share_assisted - 50.0).abs() < 1e-9 && (codex.share_all - 25.0).abs() < 1e-9);
    sort_models(&mut rows, ModelCol::Commits, false);
    assert_eq!(rows[0].id, "openai/codex/");
    sort_models(&mut rows, ModelCol::Company, true);
    assert_eq!(rows[0].company, "OpenAI");
    for col in ModelCol::ALL {
        assert_eq!(ModelCol::from_key(col.key()), Some(col));
    }
}

#[test]
fn links_are_newest_first() {
    let o = fixture();
    let (prs, commits) = links_for(&credits(&o), "zed");
    assert_eq!(prs.iter().map(|p| p.number).collect::<Vec<_>>(), [2, 1]);
    assert_eq!(commits.iter().map(|k| k.sha).collect::<Vec<_>>(), ["c3", "a1"]);
    assert_eq!(day("2026-01-05T01:00:00Z"), "2026-01-05");
}

#[test]
fn compiled_data_has_only_the_owners_consented_names() {
    let c = Credits::compiled();
    assert!(!c.commits.is_empty() && !c.prs.is_empty() && !c.models.is_empty(), "contributors/*.tsv compiled in");
    let named: Vec<_> = c.people.iter().filter(|p| !p.display_name.is_empty() || !p.real_name.is_empty() || !p.email.is_empty()).collect();
    assert_eq!(named.len(), 1, "only @echelon has consented to names: {named:?}");
    assert_eq!((named[0].login, named[0].real_name, named[0].display_name, named[0].email), ("echelon", "Brandon Thomas", "Brandon Thomas", ""));
    let rows = contributors(&c);
    let echelon = rows.iter().find(|r| r.login == "echelon").expect("echelon contributed");
    assert_eq!(echelon.name(NameMode::Real), "Brandon Thomas");
    for r in rows.iter().filter(|r| r.login != "echelon") {
        assert_eq!((r.display_name, r.real_name), (None, None), "{}", r.login);
    }
    // No email addresses anywhere in the compiled tables.
    let text: Vec<&str> = c.commits.iter().flat_map(|k| [k.login, k.url, k.models]).chain(c.prs.iter().flat_map(|p| [p.login, p.url, p.models])).collect();
    assert!(text.iter().all(|s| !s.contains('@')), "no emails");
}
