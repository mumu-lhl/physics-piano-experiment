# Translating the instrument GUIs

The piano, guitar, bass, and drum editors use gettext PO catalogs. English
strings are the source messages, and each message has a context to distinguish
labels that happen to use the same wording in different parts of the UI.

## Weblate component

Create one Weblate component for the four instrument GUIs with these settings:

- Translation type: **Gettext PO file**
- Source language: **English (`en`)**
- Template: `locales/physics-instruments.pot`
- Translation file mask: `locales/{language}.po`
- New translations: `locales/{language}.po`

For example, Weblate creates `locales/fr.po` for French and
`locales/zh_TW.po` for Traditional Chinese. Every translation catalog is
compiled into the shared `physics-ui` crate at build time; no system gettext
installation is needed by users.

## Updating the source template

When adding or changing English UI strings, regenerate the template from the
Rust call sites with the durable extractor (it blanks comment lines instead
of stripping them, so the `#: ` refs point at the true call-site lines; its
scanner mirrors `crates/physics-ui/tests/i18n_keys.rs`). The shared lookup
functions take `(language, context, msgid)`:

```sh
python3 ~/workspace/physics-maintenance/extract_pot.py --root .
python3 ~/workspace/physics-maintenance/verify-pot.py --root .
```

The extractor rewrites only the `#: ` refs by default and refuses to add or
drop keys: if the sources gained or lost a key, review the reported diff
first, update the `.po` catalogs, then re-run with `--allow-key-changes`.
Validate the catalogs with `msgfmt --check-format` on each `.po`/`.pot`
file before committing.

Review the generated POT before committing it, then merge it into each PO
catalog with Weblate or `msgmerge --update locales/fr.po locales/physics-instruments.pot`.

## Adding a language

Add a standard gettext catalog such as `locales/fr.po`, with the usual PO
header. Add this metadata entry when the language name is translated:

```po
msgctxt "meta.language-name"
msgid "Language name"
msgstr "Français"
```

The filename stem is the locale code shown to the application, and the
metadata translation is the language's native name in the language picker. If
the metadata has not been translated yet, the picker shows the locale code.
The build discovers PO files automatically. Missing individual messages use
their English source text.
