use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    context: String,
    source: String,
}

#[derive(Debug)]
struct Language {
    code: String,
    catalog: Option<PathBuf>,
}

pub fn run(root: &Path) -> Result<(), String> {
    let i18n_dir = root.join("crates/ui-egui/src/i18n");
    let registry_path = i18n_dir.join("mod.rs");
    let registry = fs::read_to_string(&registry_path).map_err(|e| format!("{}: {e}", registry_path.display()))?;
    let languages = registered_languages(&registry)?;

    let english_path = root.join("xtask/i18n-english-keys.tsv");
    let english_text = fs::read_to_string(&english_path).map_err(|e| format!("{}: {e}", english_path.display()))?;
    let english_keys = parse_key_inventory(&english_path, &english_text)?;
    let sources = source_files(root)?;
    let literal_keys = tl_keys(&sources);
    let missing_literals: Vec<_> = literal_keys.difference(&english_keys).map(|key| key.source.clone()).collect();
    if !missing_literals.is_empty() {
        return Err(format!("{}: English-key inventory is missing UI tl! keys: {missing_literals:?}", english_path.display()));
    }
    let mut catalog_texts = BTreeMap::new();
    for language in &languages {
        if let Some(path) = &language.catalog {
            let full_path = i18n_dir.join(path);
            let text = fs::read_to_string(&full_path).map_err(|e| format!("{}: {e}", full_path.display()))?;
            catalog_texts.insert(language.code.clone(), (full_path, text));
        }
    }
    let mut translations = BTreeMap::new();
    for language in &languages {
        let entries = if language.catalog.is_some() {
            let (full_path, text) = catalog_texts.get(&language.code).ok_or_else(|| format!("missing source for registered language `{}`", language.code))?;
            parse_catalog(full_path, text, &english_keys)?
        } else {
            BTreeSet::new()
        };
        translations.insert(language.code.clone(), entries);
    }

    if english_keys.is_empty() {
        return Err("no English UI translation keys were found".into());
    }
    print!("{}", format_report(&languages, &english_keys, &translations));
    Ok(())
}

fn format_report(languages: &[Language], english_keys: &BTreeSet<Key>, translations: &BTreeMap<String, BTreeSet<Key>>) -> String {
    let mut report = String::from("UI translation coverage (catalogued English keys / English keys)\n");
    report.push_str(&format!("{:<12} {:>12} {:>9}\n", "language", "translated", "coverage"));
    for language in languages {
        let translated =
            if language.code == "en" { english_keys.len() } else { translations.get(&language.code).map_or(0, |keys| keys.intersection(english_keys).count()) };
        let coverage = 100.0 * translated as f64 / english_keys.len() as f64;
        report.push_str(&format!("{:<12} {:>5}/{:<6} {:>8.2}%\n", language.code, translated, english_keys.len(), coverage));
    }
    report
}

fn registered_languages(source: &str) -> Result<Vec<Language>, String> {
    let registry = source
        .split("pub static LANGUAGES")
        .nth(1)
        .ok_or_else(|| "could not find the LANGUAGES registry".to_string())?
        .split("impl LangInfo")
        .next()
        .ok_or_else(|| "could not find the end of the LANGUAGES registry".to_string())?;
    let mut languages = Vec::new();
    let mut codes = HashSet::new();
    for (index, section) in registry.split("LangInfo {").skip(1).enumerate() {
        let block = section.split("},").next().unwrap_or(section);
        let code = quoted_field(block, "code:").ok_or_else(|| format!("LANGUAGES entry {} has no code", index + 1))?;
        if !codes.insert(code.clone()) {
            return Err(format!("duplicate registered language code `{code}`"));
        }
        let catalog = if code == "en" {
            None
        } else {
            let marker = "include_str!(\"";
            let start = block.find(marker).ok_or_else(|| format!("language `{code}` has no include_str! catalog"))? + marker.len();
            let rest = block.get(start..).ok_or_else(|| format!("language `{code}` has a malformed catalog path"))?;
            let end = rest.find("\")").ok_or_else(|| format!("language `{code}` has a malformed catalog path"))?;
            let file = rest.get(..end).ok_or_else(|| format!("language `{code}` has an invalid catalog path"))?;
            let path = Path::new(file);
            if path.components().count() != 1 || path.extension().and_then(|ext| ext.to_str()) != Some("tsv") {
                return Err(format!("language `{code}` has unsafe catalog path `{file}`"));
            }
            Some(path.to_path_buf())
        };
        languages.push(Language { code, catalog });
    }
    if !languages.iter().any(|language| language.code == "en") {
        return Err("LANGUAGES registry does not contain English (`en`)".into());
    }
    languages.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(languages)
}

fn quoted_field(block: &str, field: &str) -> Option<String> {
    let start = block.find(field)? + field.len();
    let rest = block.get(start..)?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest.get(..end)?.to_string())
}

fn parse_key_inventory(path: &Path, text: &str) -> Result<BTreeSet<Key>, String> {
    let mut keys = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut columns = line.split('\t');
        let key = match (columns.next(), columns.next(), columns.next()) {
            (Some(context), Some(source), None) if !source.is_empty() => Key { context: unescape_tsv(context), source: unescape_tsv(source) },
            _ => return Err(format!("{}:{line_no}: expected `context<TAB>source`", path.display(), line_no = index + 1)),
        };
        if !keys.insert(key.clone()) {
            return Err(format!("{}:{line_no}: duplicate English key {:?}", path.display(), key, line_no = index + 1));
        }
    }
    if keys.is_empty() {
        return Err(format!("{}: English-key inventory is empty", path.display()));
    }
    Ok(keys)
}

fn source_files(root: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut files = Vec::new();
    let mut stack = vec![root.join("crates/ui-egui/src")];
    while let Some(path) = stack.pop() {
        let entries = fs::read_dir(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", path.display()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                files.push((path, text));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn tl_keys(sources: &[(PathBuf, String)]) -> BTreeSet<Key> {
    let mut keys = BTreeSet::new();
    for (path, source) in sources {
        if path.ends_with("i18n/mod.rs") {
            continue;
        }
        let code = source.split("#[cfg(test)]\nmod ").next().unwrap_or(source);
        let mut rest = code;
        while let Some(at) = rest.find("tl!(\"") {
            rest = rest.get(at + 5..).unwrap_or("");
            let bytes = rest.as_bytes();
            let mut end = 0;
            while end < bytes.len() && !(bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\')) {
                end += 1;
            }
            if let Some(raw) = rest.get(..end) {
                let source = unescape_rust_string(raw);
                if rest.get(end + 1..end + 2) == Some(")") {
                    keys.insert(Key { context: String::new(), source });
                }
            }
        }
    }
    keys
}

fn unescape_rust_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn parse_catalog(path: &Path, text: &str, english_keys: &BTreeSet<Key>) -> Result<BTreeSet<Key>, String> {
    let mut keys = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut columns = line.split('\t');
        let parsed = match (columns.next(), columns.next(), columns.next(), columns.next()) {
            (Some(context), Some(source), Some(translation), None) if !source.is_empty() && !translation.is_empty() => {
                Key { context: unescape_tsv(context), source: unescape_tsv(source) }
            }
            _ => return Err(format!("{}:{line_no}: expected `context<TAB>source<TAB>translation`", path.display(), line_no = index + 1)),
        };
        if !keys.insert(Key { context: parsed.context.clone(), source: parsed.source.clone() }) {
            return Err(format!("{}:{line_no}: duplicate key {:?} {:?}", path.display(), parsed.context, parsed.source, line_no = index + 1));
        }
        if !english_keys.contains(&parsed) {
            return Err(format!("{}:{line_no}: unknown English key {:?} {:?}", path.display(), parsed.context, parsed.source, line_no = index + 1));
        }
    }
    Ok(keys)
}

fn unescape_tsv(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known_keys(values: &[&str]) -> BTreeSet<Key> {
        values.iter().map(|value| Key { context: String::new(), source: (*value).to_string() }).collect()
    }

    #[test]
    fn rejects_unknown_english_keys() {
        let known = known_keys(&["Open"]);
        let error = parse_catalog(Path::new("xx.tsv"), "\tTypo\tÜbersetzung\n", &known).err();
        assert!(error.is_some_and(|message| message.contains("unknown English key")));
    }

    #[test]
    fn rejects_duplicate_keys() {
        let known = known_keys(&["Open"]);
        let error = parse_catalog(Path::new("xx.tsv"), "\tOpen\tOuvrir\n\tOpen\tOuvert\n", &known).err();
        assert!(error.is_some_and(|message| message.contains("duplicate key")));
    }

    #[test]
    fn rejects_duplicate_inventory_keys() {
        let error = parse_key_inventory(Path::new("english.tsv"), "\tOpen\n\tOpen\n").err();
        assert!(error.is_some_and(|message| message.contains("duplicate English key")));
    }

    #[test]
    fn registered_languages_and_report_are_stable() {
        let registry = r#"
            pub static LANGUAGES: [LangInfo; 3] = [
                LangInfo { code: "zh", source: include_str!("zh.tsv") },
                LangInfo { code: "en", source: "" },
                LangInfo { code: "fr", source: include_str!("fr.tsv") },
            ];
            impl LangInfo {}
        "#;
        let languages = registered_languages(registry);
        assert!(languages.is_ok());
        let languages = languages.unwrap_or_default();
        let english_keys = BTreeSet::from([Key { context: String::new(), source: "Open".into() }, Key { context: String::new(), source: "Save".into() }]);
        let translations = BTreeMap::from([("fr".to_string(), BTreeSet::from([Key { context: String::new(), source: "Open".into() }]))]);
        assert_eq!(
            format_report(&languages, &english_keys, &translations),
            "UI translation coverage (catalogued English keys / English keys)\n\
             language       translated  coverage\n\
             en               2/2        100.00%\n\
             fr               1/2         50.00%\n\
             zh               0/2          0.00%\n"
        );
    }
}
