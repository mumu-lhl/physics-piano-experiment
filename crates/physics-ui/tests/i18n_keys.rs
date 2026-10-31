//! Regression test for P1-16: every `translate()` / `translate_format()`
//! call site must have a matching `(context, message)` entry in both
//! `locales/en.po` and `locales/zh_CN.po`.
//!
//! `translate()` looks up by `(locale, context, message)` and silently falls
//! back to the English inline default on a miss, so a missing key shows
//! English UI in the Chinese locale with no error anywhere. `build.rs`
//! catches duplicate entries but not missing ones, so this test diffs the
//! call sites extracted from the Rust sources against both `.po` files and
//! fails on any key the sources use but a locale file lacks.
//!
//! The extractor is deliberately a small line/paren scanner, not a Rust
//! parser: it handles the call shapes actually used in this tree
//! (single-line and multi-line calls, string-literal context/default, and
//! the conditional context/default pair in `theme.rs`). Anything else -- a
//! non-literal context or default, an unparseable call -- fails the test
//! loudly instead of being silently skipped, so the scanner gets extended
//! rather than going blind.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Crates whose `src/` trees contain `translate()` call sites. Drums are
/// excluded: the drum GUI is deprecated and its keys are not part of the
/// shared UI contract this test guards.
const SCAN_CRATES: &[&str] = &["physics-ui", "physics-piano", "physics-guitar", "physics-bass"];

const LOCALES: &[&str] = &["en", "zh_CN"];

type I18nKey = (String, String);

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root must exist")
}

fn locales_dir() -> PathBuf {
    workspace_root().join("locales")
}

// ---------------------------------------------------------------------------
// Rust source scanning
// ---------------------------------------------------------------------------

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .map(|entry| entry.expect("read dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Replace `//` and `/* */` comments with spaces (newlines kept, so line
/// numbers survive). String contents are left intact; a `"` inside a char
/// literal or a comment is not a string delimiter in this tree, and the
/// scanner fails loudly on non-literal call arguments if that ever changes.
fn strip_comments(source: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len());
    let mut i = 0;
    while i < source.len() {
        if source[i] == b'"' {
            let end = skip_string(source, i);
            out.extend_from_slice(&source[i..end]);
            i = end;
        } else if source[i] == b'/' && source.get(i + 1) == Some(&b'/') {
            while i < source.len() && source[i] != b'\n' {
                out.push(b' ');
                i += 1;
            }
        } else if source[i] == b'/' && source.get(i + 1) == Some(&b'*') {
            out.push(b' ');
            out.push(b' ');
            i += 2;
            while i + 1 < source.len() && !(source[i] == b'*' && source[i + 1] == b'/') {
                out.push(if source[i] == b'\n' { b'\n' } else { b' ' });
                i += 1;
            }
            if i + 1 < source.len() {
                out.push(b' ');
                out.push(b' ');
                i += 2;
            }
        } else {
            out.push(source[i]);
            i += 1;
        }
    }
    out
}

/// Index just past the closing quote of the string starting at `start`
/// (`source[start] == b'"'`), honouring backslash escapes.
fn skip_string(source: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < source.len() {
        match source[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    panic!("unterminated string literal at byte {start}");
}

/// Index just past a char literal starting at `start`, if `source[start]`
/// really opens one (`'x'`, `'\''`, `'\n'`). Lifetimes (`'a`, `'static`)
/// are not char literals: the byte after the would-be content is not a
/// closing quote, so they return `None`.
fn skip_char_literal(source: &[u8], start: usize) -> Option<usize> {
    if source[start] != b'\'' {
        return None;
    }
    let content = *source.get(start + 1)?;
    if content == b'\\' {
        let escaped = *source.get(start + 2)?;
        let end = if escaped == b'x' || escaped == b'u' {
            // \xNN or \u{...}: scan to the closing quote.
            let mut i = start + 3;
            while i < source.len() && source[i] != b'\'' {
                i += 1;
            }
            i
        } else {
            start + 3
        };
        if source.get(end) == Some(&b'\'') {
            Some(end + 1)
        } else {
            None
        }
    } else if source.get(start + 2) == Some(&b'\'') {
        Some(start + 3)
    } else {
        None
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Find every `translate(`/`translate_format(` call in `source` (already
/// comment-stripped) and return the byte range of its parenthesised
/// argument list plus the line number. The `translate`/`translate_format`
/// function definitions in `language.rs` are skipped.
fn find_calls(source: &[u8]) -> Vec<(usize, usize, usize)> {
    let mut calls = Vec::new();
    let mut i = 0;
    while i < source.len() {
        if source[i] == b'"' {
            i = skip_string(source, i);
            continue;
        }
        if let Some(end) = skip_char_literal(source, i) {
            i = end;
            continue;
        }
        if is_ident_byte(source[i]) && (i == 0 || !is_ident_byte(source[i - 1])) {
            let start = i;
            while i < source.len() && is_ident_byte(source[i]) {
                i += 1;
            }
            let name = &source[start..i];
            if name == b"translate" || name == b"translate_format" {
                let mut j = i;
                while j < source.len() && (source[j] as char).is_whitespace() {
                    j += 1;
                }
                if source.get(j) == Some(&b'(') {
                    // Skip `pub fn translate_format(` (the definition);
                    // `pub fn translate<'a>(` never reaches here.
                    let line_start = source[..start].iter().rposition(|b| *b == b'\n').map_or(0, |p| p + 1);
                    let before = String::from_utf8_lossy(&source[line_start..start]);
                    let before = before.trim_end();
                    if !(before == "fn" || before.ends_with(" fn")) {
                        let mut depth = 0;
                        let mut k = j;
                        while k < source.len() {
                            if source[k] == b'"' {
                                k = skip_string(source, k);
                                continue;
                            }
                            if let Some(end) = skip_char_literal(source, k) {
                                k = end;
                                continue;
                            }
                            match source[k] {
                                b'(' => depth += 1,
                                b')' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            k += 1;
                        }
                        assert!(depth == 0, "unbalanced parens in translate() call");
                        let line = source[..start].iter().filter(|b| **b == b'\n').count() + 1;
                        calls.push((j, k, line));
                        i = k + 1;
                        continue;
                    }
                }
            }
            continue;
        }
        i += 1;
    }
    calls
}

/// Split an argument list on top-level commas, honouring strings, char
/// literals and nested delimiters.
fn split_top_level(args: &str) -> Vec<String> {
    let bytes = args.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let end = skip_string(bytes, i);
            current.push_str(&args[i..end]);
            i = end;
            continue;
        }
        if let Some(end) = skip_char_literal(bytes, i) {
            current.push_str(&args[i..end]);
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                i += 1;
                continue;
            }
            _ => {}
        }
        // Non-delimiter bytes are copied a whole char at a time so
        // non-ASCII text outside string literals stays valid UTF-8.
        let ch = args[i..].chars().next().unwrap();
        current.push(ch);
        i += ch.len_utf8();
    }
    parts.push(current);
    parts
}

fn decode_rust_string(literal: &str) -> String {
    let body = literal
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or_else(|| panic!("expected a quoted string, found {literal:?}"));
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('0') => out.push('\0'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                let value = u32::from_str_radix(&hex, 16)
                    .unwrap_or_else(|_| panic!("bad \\x escape in {literal:?}"));
                out.push(char::from_u32(value).unwrap_or_else(|| panic!("bad \\x escape in {literal:?}")));
            }
            Some('u') => {
                let mut hex = String::new();
                assert_eq!(chars.next(), Some('{'), "bad \\u escape in {literal:?}");
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    hex.push(c);
                }
                let value = u32::from_str_radix(&hex, 16)
                    .unwrap_or_else(|_| panic!("bad \\u escape in {literal:?}"));
                out.push(char::from_u32(value).unwrap_or_else(|| panic!("bad \\u escape in {literal:?}")));
            }
            Some(other) => panic!("unsupported escape \\{other} in {literal:?}"),
            None => panic!("trailing backslash in {literal:?}"),
        }
    }
    out
}

/// String literals occurring in `expr`, in order.
fn string_literals(expr: &str) -> Vec<String> {
    let bytes = expr.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let end = skip_string(bytes, i);
            out.push(decode_rust_string(&expr[i..end]));
            i = end;
        } else if let Some(end) = skip_char_literal(bytes, i) {
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Possible string values of a context/default argument: one plain literal,
/// or both branches of an `if`/`else` whose branches are string literals
/// (the `theme.rs` toggle label). Anything else fails loudly: silently
/// skipping a call shape this scanner does not understand would let a
/// missing key through.
fn expr_values(expr: &str, path: &Path, line: usize, role: &str) -> Vec<String> {
    let trimmed = expr.trim();
    if trimmed.starts_with('"') {
        let end = skip_string(trimmed.as_bytes(), 0);
        if trimmed[end..].trim().is_empty() {
            return vec![decode_rust_string(&trimmed[..end])];
        }
    }
    let literals = string_literals(trimmed);
    if trimmed.starts_with("if ") && trimmed.contains(" else ") && literals.len() == 2 {
        return literals;
    }
    panic!(
        "{}:{line}: cannot extract {role} argument from translate() call; \
         extend the i18n scanner instead of skipping it: {trimmed:?}",
        path.display()
    );
}

fn extract_call_sites(root: &Path) -> (BTreeSet<I18nKey>, BTreeMap<I18nKey, Vec<String>>) {
    let mut keys = BTreeSet::new();
    let mut locations: BTreeMap<I18nKey, Vec<String>> = BTreeMap::new();
    for name in SCAN_CRATES {
        let src = root.join("crates").join(name).join("src");
        let mut files = Vec::new();
        collect_rs_files(&src, &mut files);
        for file in files {
            let raw = fs::read(&file)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", file.display()));
            let stripped = strip_comments(&raw);
            let text = String::from_utf8(stripped)
                .unwrap_or_else(|_| panic!("{} is not valid UTF-8", file.display()));
            for (open, close, line) in find_calls(text.as_bytes()) {
                let args = split_top_level(&text[open + 1..close]);
                assert!(
                    args.len() >= 3,
                    "{}:{line}: translate() call with {} arguments",
                    file.display(),
                    args.len()
                );
                // `translate_format` forwards to `translate(language,
                // context, message)` with bare identifiers: it introduces
                // no new key of its own, and the real call sites that
                // supply the pair are extracted separately. Any other
                // non-literal shape still fails loudly in `expr_values`.
                let is_bare_ident = |arg: &str| {
                    let arg = arg.trim();
                    !arg.is_empty()
                        && arg
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        && !arg.as_bytes()[0].is_ascii_digit()
                };
                if is_bare_ident(&args[1]) && is_bare_ident(&args[2]) {
                    continue;
                }
                let contexts = expr_values(&args[1], &file, line, "context");
                let defaults = expr_values(&args[2], &file, line, "default message");
                assert_eq!(
                    contexts.len(),
                    defaults.len(),
                    "{}:{line}: conditional translate() call with mismatched context/default branches",
                    file.display()
                );
                for (context, message) in contexts.into_iter().zip(defaults) {
                    let key = (context, message);
                    locations
                        .entry(key.clone())
                        .or_default()
                        .push(format!("{}:{line}", file.strip_prefix(root).unwrap_or(&file).display()));
                    keys.insert(key);
                }
            }
        }
    }
    (keys, locations)
}

// ---------------------------------------------------------------------------
// .po parsing (mirrors crates/physics-ui/build.rs)
// ---------------------------------------------------------------------------

struct PoFile {
    /// Entries build.rs would embed: non-empty id, non-empty msgstr, not plural.
    usable: BTreeSet<I18nKey>,
    empty_msgstr: Vec<I18nKey>,
    duplicates: Vec<I18nKey>,
}

fn decode_po_string(input: &str, path: &Path, line: usize) -> String {
    let input = input.trim();
    if !input.starts_with('"') || !input.ends_with('"') || input.len() < 2 {
        panic!("{}:{line}: expected a quoted PO string, found {input}", path.display());
    }
    let mut out = String::new();
    let mut chars = input[1..input.len() - 1].chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000c}'),
            Some('v') => out.push('\u{000b}'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => panic!("{}:{line}: unsupported PO escape \\{other}", path.display()),
            None => panic!("{}:{line}: trailing backslash in PO string", path.display()),
        }
    }
    out
}

struct PoParser {
    context: String,
    id: String,
    translation: String,
    plural: bool,
    /// 0 = context, 1 = id, 2 = translation.
    field: Option<u8>,
    has_content: bool,
    usable: BTreeSet<I18nKey>,
    empty_msgstr: Vec<I18nKey>,
    duplicates: Vec<I18nKey>,
    seen: BTreeSet<I18nKey>,
}

impl PoParser {
    fn new() -> Self {
        Self {
            context: String::new(),
            id: String::new(),
            translation: String::new(),
            plural: false,
            field: None,
            has_content: false,
            usable: BTreeSet::new(),
            empty_msgstr: Vec::new(),
            duplicates: Vec::new(),
            seen: BTreeSet::new(),
        }
    }

    fn flush(&mut self) {
        if self.has_content {
            let key = (std::mem::take(&mut self.context), std::mem::take(&mut self.id));
            let value = std::mem::take(&mut self.translation);
            let was_plural = std::mem::replace(&mut self.plural, false);
            if !key.1.is_empty() && !was_plural {
                if value.is_empty() {
                    self.empty_msgstr.push(key);
                } else if !self.seen.insert(key.clone()) {
                    self.duplicates.push(key);
                } else {
                    self.usable.insert(key);
                }
            }
            self.has_content = false;
        }
        self.field = None;
    }
}

fn parse_po(path: &Path) -> PoFile {
    let contents = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut parser = PoParser::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line_no = index + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            parser.flush();
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("msgctxt ") {
            parser.flush();
            parser.field = Some(0);
            parser.context.push_str(&decode_po_string(value, path, line_no));
            parser.has_content = true;
        } else if let Some(value) = line.strip_prefix("msgid ") {
            parser.field = Some(1);
            parser.id.push_str(&decode_po_string(value, path, line_no));
            parser.has_content = true;
        } else if let Some(value) = line.strip_prefix("msgid_plural ") {
            parser.plural = true;
            parser.field = Some(1);
            parser.id.push_str(&decode_po_string(value, path, line_no));
            parser.has_content = true;
        } else if let Some(value) = line.strip_prefix("msgstr ") {
            parser.field = Some(2);
            parser.translation.push_str(&decode_po_string(value, path, line_no));
            parser.has_content = true;
        } else if line.starts_with("msgstr[") {
            parser.plural = true;
            parser.field = Some(2);
            let value = line.split_once(' ').unwrap().1;
            parser.translation.push_str(&decode_po_string(value, path, line_no));
            parser.has_content = true;
        } else if line.starts_with('"') {
            let decoded = decode_po_string(line, path, line_no);
            match parser.field {
                Some(0) => parser.context.push_str(&decoded),
                Some(1) => parser.id.push_str(&decoded),
                Some(2) => parser.translation.push_str(&decoded),
                Some(_) => unreachable!("PO field tag is only ever 0, 1, or 2"),
                None => panic!("{}:{line_no}: orphan PO continuation", path.display()),
            }
            parser.has_content = true;
        } else {
            panic!("{}:{line_no}: unsupported PO syntax: {line}", path.display());
        }
    }
    parser.flush();
    PoFile { usable: parser.usable, empty_msgstr: parser.empty_msgstr, duplicates: parser.duplicates }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn translate_keys_exist_in_both_locales() {
    let root = workspace_root();
    let (keys, locations) = extract_call_sites(&root);
    assert!(
        !keys.is_empty(),
        "i18n scanner extracted no translate() keys; the scanner is broken, not the locales"
    );
    // The conditional pair in theme.rs is the one call shape a naive
    // line-based scanner misses; prove it was extracted.
    for key in [("gui.theme-light", "Light"), ("gui.theme-dark", "Dark")] {
        let key = (key.0.to_owned(), key.1.to_owned());
        assert!(
            keys.contains(&key),
            "i18n scanner missed the conditional theme.rs pair {key:?}; fix the scanner"
        );
    }

    let mut failures = String::new();
    for locale in LOCALES {
        let path = locales_dir().join(format!("{locale}.po"));
        let parsed = parse_po(&path);
        assert!(
            parsed.duplicates.is_empty(),
            "duplicate gettext entries in {locale}: {:?} (build.rs also rejects these)",
            parsed.duplicates
        );
        let missing: Vec<_> = keys.difference(&parsed.usable).collect();
        if !missing.is_empty() {
            failures.push_str(&format!("\n{locale}.po is missing {} key(s):\n", missing.len()));
            for key in missing {
                let sites = locations.get(key).cloned().unwrap_or_default();
                failures.push_str(&format!("  {key:?} used at {}\n", sites.join(", ")));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "translate() keys missing from .po files (add them to BOTH locales; \
         translate() falls back to the English default on a miss):{failures}"
    );
}

#[test]
fn no_empty_msgstr_in_locales() {
    // build.rs silently skips empty-msgstr entries, which is the same
    // user-visible failure as a missing key: the English default shows.
    for locale in LOCALES {
        let path = locales_dir().join(format!("{locale}.po"));
        let parsed = parse_po(&path);
        // A key counts only if no entry for it carries a translation;
        // build.rs embeds the valued entry and ignores the empty one.
        let untranslated: Vec<_> = parsed
            .empty_msgstr
            .iter()
            .filter(|key| !parsed.usable.contains(*key))
            .collect();
        assert!(
            untranslated.is_empty(),
            "{locale}.po has entries with an empty msgstr (build.rs skips them, \
             so the English default shows instead): {untranslated:?}"
        );
    }
}
