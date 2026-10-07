//! `cargo xtask contributors`: refresh `contributors/commits.tsv` and `contributors/prs.tsv`
//! (format: `../craftrules/standards/contributors.md`) from the GitHub API (through `gh api
//! graphql`) and the local git history (`Co-Authored-By` trailers). This is the only step that
//! touches the network. Output is deterministic (rows sorted by date, then sha / number).
//!
//! Privacy: only GitHub logins are recorded. Git author names and emails are never read, and
//! `people.tsv` / `models.tsv` are never written. Trailer names that `models.tsv` doesn't map
//! are printed (so new models get added deliberately) but not recorded.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

#[path = "../../crates/ui-egui/src/credits_data.rs"]
#[allow(dead_code)]
mod credits_data;

const OWNER: &str = "storytold";
const NAME: &str = "photocraft";

const HISTORY_QUERY: &str = "query($owner:String!,$name:String!,$after:String){repository(owner:$owner,name:$name){defaultBranchRef{name target{... on Commit{history(first:100,after:$after){pageInfo{hasNextPage endCursor} nodes{oid committedDate additions deletions author{user{login}} associatedPullRequests(first:10){nodes{number merged mergedAt}}}}}}}}}";

const PRS_QUERY: &str = "query($owner:String!,$name:String!,$after:String){repository(owner:$owner,name:$name){pullRequests(states:MERGED,first:100,after:$after,orderBy:{field:CREATED_AT,direction:ASC}){pageInfo{hasNextPage endCursor} nodes{number url mergedAt additions deletions commits{totalCount} author{login}}}}}";

fn graphql(query: &str, after: Option<&str>) -> Result<Value, String> {
    let mut c = Command::new("gh");
    c.args(["api", "graphql", "-f", &format!("query={query}"), "-F", &format!("owner={OWNER}"), "-F", &format!("name={NAME}")]);
    if let Some(a) = after {
        c.args(["-f", &format!("after={a}")]);
    }
    let out = c.output().map_err(|e| format!("gh api graphql: {e} (is the GitHub CLI installed and logged in?)"))?;
    if !out.status.success() {
        return Err(format!("gh api graphql failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("gh api graphql: bad JSON: {e}"))?;
    if let Some(errors) = v.get("errors") {
        return Err(format!("GitHub GraphQL errors: {errors}"));
    }
    Ok(v)
}

/// Every node of a paginated connection at `path` (JSON pointer under `data`).
fn paginate(query: &str, path: &str) -> Result<Vec<Value>, String> {
    let mut nodes = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let v = graphql(query, after.as_deref())?;
        let conn = v.pointer(&format!("/data{path}")).ok_or_else(|| format!("unexpected GraphQL response (no {path})"))?;
        nodes.extend(conn["nodes"].as_array().cloned().unwrap_or_default());
        let page = &conn["pageInfo"];
        if page["hasNextPage"].as_bool() != Some(true) {
            return Ok(nodes);
        }
        after = Some(page["endCursor"].as_str().ok_or("pageInfo without endCursor")?.to_string());
    }
}

/// The `Co-Authored-By` trailer names (without `<email>`) of every commit reachable from `rev`.
fn local_trailers(root: &Path, rev: &str) -> Result<HashMap<String, Vec<String>>, String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["log", rev, "--format=%H%x1f%(trailers:key=Co-Authored-By,valueonly,separator=%x1e)%x1d"])
        .output()
        .map_err(|e| format!("git log: {e}"))?;
    if !out.status.success() {
        return Err(format!("git log {rev} failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut map = HashMap::new();
    for rec in text.split('\u{1d}') {
        let rec = rec.trim_matches(['\n', '\r']);
        let Some((sha, trailers)) = rec.split_once('\u{1f}') else { continue };
        let names = trailers.split('\u{1e}').map(|t| t.split(" <").next().unwrap_or("").trim().to_string()).filter(|t| !t.is_empty()).collect();
        map.insert(sha.to_string(), names);
    }
    Ok(map)
}

fn is_bot(login: &str) -> bool {
    login.ends_with("[bot]")
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

pub fn run(root: &Path) -> Result<(), String> {
    let dir = root.join("contributors");
    let models_text = std::fs::read_to_string(dir.join("models.tsv")).map_err(|e| format!("contributors/models.tsv: {e}"))?;
    let models = credits_data::parse_models(&models_text).map_err(|e| format!("contributors/models.tsv: {e}"))?;
    let trailer_ids: HashMap<&str, &str> = models.iter().map(|m| (m.trailer, m.id)).collect();

    eprintln!("fetching {OWNER}/{NAME} default-branch history…");
    let history = paginate(HISTORY_QUERY, "/repository/defaultBranchRef/target/history")?;
    eprintln!("fetching merged pull requests…");
    let prs = paginate(PRS_QUERY, "/repository/pullRequests")?;

    // Trailers from local git: origin's default branch if fetched, else HEAD.
    let rev = ["origin/main", "HEAD"]
        .into_iter()
        .find(|r| Command::new("git").current_dir(root).args(["rev-parse", "--verify", "--quiet", r]).output().is_ok_and(|o| o.status.success()))
        .ok_or("no git history found")?;
    let trailers = local_trailers(root, rev)?;

    let mut unmapped: BTreeMap<String, usize> = BTreeMap::new();
    let mut missing_local = 0usize;
    // (date, sha) → row
    let mut commit_rows: Vec<(String, String, String)> = Vec::new();
    let mut pr_models: HashMap<u64, BTreeSet<String>> = HashMap::new();
    for c in &history {
        let sha = c["oid"].as_str().ok_or("commit without oid")?.to_string();
        let login = c.pointer("/author/user/login").and_then(Value::as_str).unwrap_or("unknown").to_string();
        if is_bot(&login) {
            continue;
        }
        let date = c["committedDate"].as_str().unwrap_or("").to_string();
        // The merged PR that brought it in: the earliest-merged associated PR.
        let pr = c
            .pointer("/associatedPullRequests/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|p| p["merged"].as_bool() == Some(true))
            .min_by(|a, b| a["mergedAt"].as_str().cmp(&b["mergedAt"].as_str()).then(u(&a["number"]).cmp(&u(&b["number"]))))
            .map(|p| u(&p["number"]));
        let mut ids = BTreeSet::new();
        match trailers.get(&sha) {
            Some(names) => {
                for n in names {
                    match trailer_ids.get(n.as_str()) {
                        Some(id) => {
                            ids.insert(id.to_string());
                        }
                        None => *unmapped.entry(n.clone()).or_default() += 1,
                    }
                }
            }
            None => missing_local += 1,
        }
        if let Some(n) = pr {
            pr_models.entry(n).or_default().extend(ids.iter().cloned());
        }
        let row = format!(
            "{sha}\thttps://github.com/{OWNER}/{NAME}/commit/{sha}\t{login}\t{date}\t{}\t{}\t{}\t{}",
            u(&c["additions"]),
            u(&c["deletions"]),
            pr.map(|n| n.to_string()).unwrap_or_default(),
            ids.into_iter().collect::<Vec<_>>().join(";")
        );
        commit_rows.push((date, sha, row));
    }
    commit_rows.sort();

    let mut pr_rows: Vec<(String, u64, String)> = Vec::new();
    for p in &prs {
        let login = p.pointer("/author/login").and_then(Value::as_str).unwrap_or("unknown").to_string();
        if is_bot(&login) {
            continue;
        }
        let number = u(&p["number"]);
        let merged_at = p["mergedAt"].as_str().unwrap_or("").to_string();
        let models = pr_models.get(&number).map(|s| s.iter().cloned().collect::<Vec<_>>().join(";")).unwrap_or_default();
        let row = format!(
            "{number}\thttps://github.com/{OWNER}/{NAME}/pull/{number}\t{login}\t{merged_at}\t{}\t{}\t{}\t{models}",
            u(&p["additions"]),
            u(&p["deletions"]),
            u(&p.pointer("/commits/totalCount").cloned().unwrap_or(Value::Null)),
        );
        pr_rows.push((merged_at, number, row));
    }
    pr_rows.sort();

    let note = "# Generated by `cargo xtask contributors` (GitHub logins only; see ../craftrules/standards/contributors.md). Do not edit.\n";
    let write = |file: &str, header: &[&str], rows: Vec<String>| -> Result<(), String> {
        let mut s = String::from(note);
        s.push_str(&header.join("\t"));
        s.push('\n');
        for r in rows {
            s.push_str(&r);
            s.push('\n');
        }
        std::fs::write(dir.join(file), s).map_err(|e| format!("contributors/{file}: {e}"))
    };
    let (nc, np) = (commit_rows.len(), pr_rows.len());
    write("commits.tsv", credits_data::COMMITS_HEADER, commit_rows.into_iter().map(|r| r.2).collect())?;
    write("prs.tsv", credits_data::PRS_HEADER, pr_rows.into_iter().map(|r| r.2).collect())?;
    println!("wrote contributors/commits.tsv ({nc} commits) and contributors/prs.tsv ({np} merged PRs)");
    if missing_local > 0 {
        println!("note: {missing_local} commits are not in the local `{rev}`; fetch it for their model trailers");
    }
    if unmapped.is_empty() {
        println!("every Co-Authored-By trailer maps to a model in contributors/models.tsv (or there are none)");
    } else {
        println!("Co-Authored-By names not in contributors/models.tsv (treated as human co-authors; add real models there):");
        for (name, n) in &unmapped {
            println!("  {n:>4}  {name}");
        }
    }
    Ok(())
}
