use biblatex::{Bibliography, ChunksExt, DateValue, Entry, PermissiveType};
use std::collections::HashMap;
use std::path::Path;

/// A normalized bibliographic record extracted from one BibTeX entry.
#[derive(Clone, Debug)]
pub struct BibEntry {
    pub key: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub venue: Option<String>,
    pub abstract_text: Option<String>,
    pub keywords: Vec<String>,
    /// File basenames referenced by the entry's `file`/`pdf` field (lowercased).
    pub file_basenames: Vec<String>,
}

/// Parsed BibTeX plus lookup indices for matching PDFs to entries.
pub struct BibIndex {
    pub entries: Vec<BibEntry>,
    by_doi: HashMap<String, usize>,
    by_file: HashMap<String, usize>,
    by_title: HashMap<String, usize>,
}

/// Lowercase, drop everything but alphanumerics — used for fuzzy title match.
pub fn normalize_title(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Canonical DOI form: lowercase, without a resolver prefix.
pub fn normalize_doi(s: &str) -> String {
    let s = s.trim().to_lowercase();
    let s = s
        .strip_prefix("https://doi.org/")
        .or_else(|| s.strip_prefix("http://doi.org/"))
        .or_else(|| s.strip_prefix("doi:"))
        .unwrap_or(&s);
    s.trim().to_string()
}

fn basename_lower(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_lowercase())
    }
}

/// Zotero/BetterBibTeX `file` fields cram several attachments together, e.g.
/// `Smith - 2020 - Title:files/42/Smith2020.pdf:application/pdf;...`. Pull out
/// the basename of every token that looks like a PDF path.
fn extract_file_basenames(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Unescape BetterBibTeX's "\:" so we can split cleanly, then scan tokens.
    let unescaped = raw.replace("\\:", "\u{0}");
    for chunk in unescaped.split([';', ':']) {
        let token = chunk.replace('\u{0}', ":");
        let token = token.trim();
        if token.to_lowercase().ends_with(".pdf") {
            if let Some(base) = basename_lower(token) {
                if !out.contains(&base) {
                    out.push(base);
                }
            }
        }
    }
    out
}

fn format_person(p: &biblatex::Person) -> String {
    let mut name = String::new();
    if !p.given_name.is_empty() {
        name.push_str(&p.given_name);
    }
    if !p.prefix.is_empty() {
        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(&p.prefix);
    }
    if !p.name.is_empty() {
        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(&p.name);
    }
    if !p.suffix.is_empty() {
        name.push_str(", ");
        name.push_str(&p.suffix);
    }
    name.trim().to_string()
}

fn entry_year(entry: &Entry) -> Option<i32> {
    if let Ok(PermissiveType::Typed(date)) = entry.date() {
        let dt = match date.value {
            DateValue::At(d) | DateValue::After(d) | DateValue::Before(d) | DateValue::Between(d, _) => d,
        };
        return Some(dt.year);
    }
    // Fall back to a bare `year = {2020}` field.
    let raw = entry.get("year")?.format_verbatim();
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
    digits.parse::<i32>().ok()
}

fn entry_venue(entry: &Entry) -> Option<String> {
    if let Ok(j) = entry.journal() {
        let s = j.format_verbatim();
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    if let Ok(b) = entry.book_title() {
        let s = b.format_verbatim();
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    if let Ok(pubs) = entry.publisher() {
        let s = pubs.iter().map(|c| c.format_verbatim()).collect::<Vec<_>>().join(", ");
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    None
}

fn nonempty(s: String) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn to_bib_entry(entry: &Entry) -> BibEntry {
    let title = entry.title().ok().and_then(|c| nonempty(c.format_verbatim()));
    let authors = entry
        .author()
        .map(|persons| persons.iter().map(format_person).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let doi = entry.doi().ok().map(|d| normalize_doi(&d)).filter(|s| !s.is_empty());
    let abstract_text = entry.abstract_().ok().and_then(|c| nonempty(c.format_verbatim()));
    let keywords = entry
        .keywords()
        .ok()
        .map(|c| {
            c.format_verbatim()
                .split([',', ';'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let file_basenames = entry.file().ok().map(|f| extract_file_basenames(&f)).unwrap_or_default();

    BibEntry {
        key: entry.key.clone(),
        title,
        authors,
        year: entry_year(entry),
        doi,
        venue: entry_venue(entry),
        abstract_text,
        keywords,
        file_basenames,
    }
}

impl BibIndex {
    pub fn parse(src: &str) -> Result<Self, String> {
        let bib = Bibliography::parse(src).map_err(|e| format!("BibTeX parse error: {e:?}"))?;
        let mut entries = Vec::new();
        let mut by_doi = HashMap::new();
        let mut by_file = HashMap::new();
        let mut by_title = HashMap::new();

        for entry in bib.iter() {
            let be = to_bib_entry(entry);
            let idx = entries.len();
            if let Some(doi) = &be.doi {
                by_doi.entry(doi.clone()).or_insert(idx);
            }
            for base in &be.file_basenames {
                by_file.entry(base.clone()).or_insert(idx);
            }
            if let Some(t) = &be.title {
                let n = normalize_title(t);
                if !n.is_empty() {
                    by_title.entry(n).or_insert(idx);
                }
            }
            entries.push(be);
        }

        Ok(BibIndex {
            entries,
            by_doi,
            by_file,
            by_title,
        })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::parse(&src)
    }

    /// Best match for a PDF, trying DOI → file basename → normalized title.
    pub fn match_document(
        &self,
        file_name: &str,
        doi: Option<&str>,
        title: &str,
    ) -> Option<&BibEntry> {
        if let Some(d) = doi {
            if let Some(&i) = self.by_doi.get(&normalize_doi(d)) {
                return self.entries.get(i);
            }
        }
        if let Some(base) = basename_lower(file_name) {
            if let Some(&i) = self.by_file.get(&base) {
                return self.entries.get(i);
            }
        }
        let n = normalize_title(title);
        if !n.is_empty() {
            if let Some(&i) = self.by_title.get(&n) {
                return self.entries.get(i);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    @article{shannon1948,
      title = {A Mathematical Theory of Communication},
      author = {Shannon, Claude E.},
      journaltitle = {Bell System Technical Journal},
      year = {1948},
      doi = {10.1002/j.1538-7305.1948.tb01338.x},
      keywords = {information theory, entropy},
      abstract = {The fundamental problem of communication.},
      file = {Shannon - 1948 - A Mathematical Theory.pdf:files/12/shannon1948.pdf:application/pdf}
    }

    @book{piketty2014,
      title = {Capital in the Twenty-First Century},
      author = {Piketty, Thomas},
      publisher = {Harvard University Press},
      date = {2014-03-10}
    }
    "#;

    #[test]
    fn parses_core_fields() {
        let idx = BibIndex::parse(SAMPLE).unwrap();
        assert_eq!(idx.entries.len(), 2);

        let shannon = &idx.entries[0];
        assert_eq!(shannon.key, "shannon1948");
        assert_eq!(shannon.title.as_deref(), Some("A Mathematical Theory of Communication"));
        assert_eq!(shannon.authors, vec!["Claude E. Shannon"]);
        assert_eq!(shannon.year, Some(1948));
        assert_eq!(shannon.venue.as_deref(), Some("Bell System Technical Journal"));
        assert_eq!(shannon.doi.as_deref(), Some("10.1002/j.1538-7305.1948.tb01338.x"));
        assert!(shannon.keywords.contains(&"entropy".to_string()));
    }

    #[test]
    fn parses_date_field_year_and_publisher_venue() {
        let idx = BibIndex::parse(SAMPLE).unwrap();
        let piketty = &idx.entries[1];
        assert_eq!(piketty.year, Some(2014));
        assert_eq!(piketty.venue.as_deref(), Some("Harvard University Press"));
    }

    #[test]
    fn matches_by_doi() {
        let idx = BibIndex::parse(SAMPLE).unwrap();
        let m = idx.match_document("random.pdf", Some("10.1002/J.1538-7305.1948.TB01338.X"), "Nope");
        assert_eq!(m.map(|e| e.key.as_str()), Some("shannon1948"));
    }

    #[test]
    fn matches_by_file_basename() {
        let idx = BibIndex::parse(SAMPLE).unwrap();
        let m = idx.match_document("SHANNON1948.pdf", None, "Unrelated title");
        assert_eq!(m.map(|e| e.key.as_str()), Some("shannon1948"));
    }

    #[test]
    fn matches_by_normalized_title() {
        let idx = BibIndex::parse(SAMPLE).unwrap();
        let m = idx.match_document("x.pdf", None, "capital in the twenty-first century");
        assert_eq!(m.map(|e| e.key.as_str()), Some("piketty2014"));
    }

    #[test]
    fn no_false_match() {
        let idx = BibIndex::parse(SAMPLE).unwrap();
        assert!(idx.match_document("mystery.pdf", None, "Totally Different Document").is_none());
    }

    #[test]
    fn normalize_doi_strips_resolver_prefix() {
        assert_eq!(normalize_doi("https://doi.org/10.1/AB"), "10.1/ab");
        assert_eq!(normalize_doi("doi:10.1/ab"), "10.1/ab");
    }
}
