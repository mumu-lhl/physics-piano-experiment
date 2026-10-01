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
Rust call sites. The shared lookup functions take `(language, context, msgid)`:

```sh
find crates/physics-ui/src \
  crates/physics-piano/src/gui crates/physics-piano/src/presets.rs \
  crates/physics-guitar/src/gui crates/physics-guitar/src/presets.rs \
  crates/physics-bass/src/gui.rs crates/physics-bass/src/gui \
  crates/physics-drum/src/gui.rs crates/physics-drum/src/gui \
  -name '*.rs' -print0 \
  | xargs -0 xgettext --language=Rust --from-code=UTF-8 \
      --keyword=translate:2c,3 --keyword=translate_format:2c,3 \
      --output=locales/physics-instruments.pot
```

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
