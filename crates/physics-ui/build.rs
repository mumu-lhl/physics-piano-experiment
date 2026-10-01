use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Entry {
    context: String,
    id: String,
    translation: String,
    plural: bool,
}

#[derive(Clone)]
struct Translation {
    locale: String,
    context: String,
    id: String,
    value: String,
}

#[derive(Clone, Copy)]
enum Field {
    Context,
    Id,
    Translation,
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let locales_dir = manifest_dir.join("../../locales");
    println!("cargo:rerun-if-changed={}", locales_dir.display());

    let mut paths: Vec<_> = fs::read_dir(&locales_dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", locales_dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "po"))
        .collect();
    paths.sort();

    let mut by_locale = BTreeSet::new();
    let mut translations = Vec::new();
    for path in paths {
        let locale = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_else(|| panic!("invalid PO filename: {}", path.display()))
            .to_owned();
        let entries = parse_po(&path);
        for entry in entries {
            if entry.id.is_empty() || entry.translation.is_empty() || entry.plural {
                continue;
            }
            translations.push(Translation {
                locale: locale.clone(),
                context: entry.context,
                id: entry.id,
                value: entry.translation,
            });
        }
        by_locale.insert(locale);
    }

    if !by_locale.contains("en") {
        panic!("locales/en.po is required as the source locale");
    }
    if !by_locale.contains("zh_CN") {
        panic!("locales/zh_CN.po is required to preserve the existing Chinese translation");
    }

    let mut locales: Vec<_> = by_locale.into_iter().collect();
    locales.sort_by_key(|locale| match locale.as_str() {
        "en" => (0, String::new()),
        "zh_CN" => (1, String::new()),
        _ => (2, locale.clone()),
    });
    if locales.len() > u8::MAX as usize + 1 {
        panic!("at most 256 embedded locales are supported");
    }

    translations.sort_by(|left, right| {
        (&left.locale, &left.context, &left.id).cmp(&(&right.locale, &right.context, &right.id))
    });
    let mut unique = BTreeSet::new();
    for entry in &translations {
        let key = (&entry.locale, &entry.context, &entry.id);
        if !unique.insert(key) {
            panic!(
                "duplicate gettext entry in {}: [{}] {}",
                entry.locale, entry.context, entry.id
            );
        }
    }

    let mut generated = String::from("pub(super) const LANGUAGE_CODES: &[&str] = &[\n");
    for locale in &locales {
        generated.push_str(&format!("    {:?},\n", locale));
    }
    generated.push_str("];\n\n");
    generated.push_str("pub(super) static TRANSLATIONS: &[(&str, &str, &str, &str)] = &[\n");
    for entry in translations {
        generated.push_str(&format!(
            "    ({:?}, {:?}, {:?}, {:?}),\n",
            entry.locale, entry.context, entry.id, entry.value
        ));
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out_dir.join("locales.rs"), generated).expect("write generated locale table");
}

fn parse_po(path: &Path) -> Vec<Entry> {
    let contents = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut entries = Vec::new();
    let mut entry = Entry::default();
    let mut field = None;
    let mut has_content = false;

    let flush = |entry: &mut Entry, entries: &mut Vec<Entry>, has_content: &mut bool| {
        if *has_content {
            entries.push(std::mem::take(entry));
            *has_content = false;
        }
    };

    for (line_index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            flush(&mut entry, &mut entries, &mut has_content);
            field = None;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }

        let (target, value) = if let Some(value) = line.strip_prefix("msgctxt ") {
            flush(&mut entry, &mut entries, &mut has_content);
            field = Some(Field::Context);
            (Some(Field::Context), value)
        } else if let Some(value) = line.strip_prefix("msgid ") {
            field = Some(Field::Id);
            (Some(Field::Id), value)
        } else if let Some(value) = line.strip_prefix("msgid_plural ") {
            entry.plural = true;
            field = Some(Field::Id);
            (Some(Field::Id), value)
        } else if let Some(value) = line.strip_prefix("msgstr ") {
            field = Some(Field::Translation);
            (Some(Field::Translation), value)
        } else if line.starts_with("msgstr[") {
            entry.plural = true;
            field = Some(Field::Translation);
            (Some(Field::Translation), line.split_once(' ').unwrap().1)
        } else if line.starts_with('"') {
            (field, line)
        } else {
            panic!(
                "{}:{}: unsupported PO syntax: {}",
                path.display(),
                line_index + 1,
                line
            );
        };

        let Some(target) = target else {
            panic!(
                "{}:{}: orphan PO continuation",
                path.display(),
                line_index + 1
            );
        };
        let decoded = decode_po_string(value)
            .unwrap_or_else(|error| panic!("{}:{}: {error}", path.display(), line_index + 1));
        match target {
            Field::Context => entry.context.push_str(&decoded),
            Field::Id => entry.id.push_str(&decoded),
            Field::Translation => entry.translation.push_str(&decoded),
        }
        has_content = true;
    }
    flush(&mut entry, &mut entries, &mut has_content);
    entries
}

fn decode_po_string(input: &str) -> Result<String, String> {
    let input = input.trim();
    if !input.starts_with('"') || !input.ends_with('"') || input.len() < 2 {
        return Err(format!("expected a quoted PO string, found {input}"));
    }
    let mut chars = input[1..input.len() - 1].chars();
    let mut output = String::new();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let escaped = chars.next().ok_or("trailing backslash in PO string")?;
        output.push(match escaped {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'b' => '\u{0008}',
            'f' => '\u{000c}',
            'v' => '\u{000b}',
            '\\' => '\\',
            '"' => '"',
            other => return Err(format!("unsupported PO escape \\{other}")),
        });
    }
    Ok(output)
}
