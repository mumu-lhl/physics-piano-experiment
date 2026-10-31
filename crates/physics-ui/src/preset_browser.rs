use vizia_plug::vizia::prelude::*;

use physics_presets::PresetManager;

use crate::Language;
use crate::translate;
use crate::widgets::preset_display_name;

/// One row of the preset browser list, already resolved to display strings.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetBrowserItem {
    pub id: String,
    pub name: String,
    pub category: String,
    pub is_favorite: bool,
    pub tags: Vec<String>,
}

/// Actions emitted by the preset browser view. Editors map them onto their
/// own UI events, the same pattern as `PresetPanelAction`.
#[derive(Debug, Clone)]
pub enum PresetBrowserAction {
    Open,
    Close,
    SetSearch(String),
    SetCategory(Option<String>),
    ToggleFavoritesOnly,
    ToggleFavorite(String),
    ToggleTag(String),
    /// Live text of the tag editor (no disk write; commit with `SetTags`).
    SetTagsInput(String),
    /// Commit the tag editor text as the current preset's tags.
    SetTags(String),
    Select(String),
}

/// Signals the browser view reads. Each editor owns them and refreshes the
/// derived ones whenever presets or filters change.
#[derive(Clone)]
pub struct PresetBrowserSignals {
    pub open: Signal<bool>,
    pub search: Signal<String>,
    pub category: Signal<Option<String>>,
    pub favorites_only: Signal<bool>,
    pub categories: Signal<Vec<String>>,
    pub filtered: Signal<Vec<PresetBrowserItem>>,
    pub tags: Signal<Vec<String>>,
    pub selected_tags: Signal<Vec<String>>,
    /// Live text of the tag editor for the current preset.
    pub tags_input: Signal<String>,
    /// Display name of the preset the tag editor edits.
    pub current_name: Signal<String>,
}

/// Full browser rows for the current manager state (display names localized).
pub fn preset_browser_items(manager: &PresetManager, lang: Language) -> Vec<PresetBrowserItem> {
    manager
        .presets()
        .iter()
        .map(|preset| PresetBrowserItem {
            id: preset.id.clone(),
            name: preset_display_name(preset, lang),
            category: preset.category.clone(),
            is_favorite: manager.is_favorite(&preset.id),
            tags: manager.tags_for(&preset.id),
        })
        .collect()
}

/// Distinct categories in first-seen order.
pub fn preset_browser_categories(items: &[PresetBrowserItem]) -> Vec<String> {
    let mut categories = Vec::new();
    for item in items {
        if !categories.contains(&item.category) {
            categories.push(item.category.clone());
        }
    }
    categories
}

/// Distinct tags in first-seen order over the browser rows.
pub fn preset_browser_tags(items: &[PresetBrowserItem]) -> Vec<String> {
    let mut tags = Vec::new();
    for item in items {
        for tag in &item.tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
    }
    tags
}

/// Parse tag editor text into tags: comma-separated, trimmed, empties and
/// duplicates dropped, first-seen order kept. Dedup is case-folded
/// ("Warm" and "warm" are one tag, keeping the first-seen spelling for
/// display), matching the manager-side normalization rule.
pub fn parse_tags_input(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for part in text.split(',') {
        let tag = part.trim().to_string();
        if !tag.is_empty() && !tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase()) {
            tags.push(tag);
        }
    }
    tags
}

/// Pure filter: favorites-only, the selected category, and every selected
/// tag narrow the list, and the search box matches a case-insensitive
/// substring of name or category.
pub fn filter_browser_items(
    items: &[PresetBrowserItem],
    search: &str,
    category: Option<&str>,
    favorites_only: bool,
    selected_tags: &[String],
) -> Vec<PresetBrowserItem> {
    let needle = search.trim().to_lowercase();
    items
        .iter()
        .filter(|item| {
            if favorites_only && !item.is_favorite {
                return false;
            }
            if let Some(category) = category {
                if item.category != category {
                    return false;
                }
            }
            if !selected_tags.is_empty() && !selected_tags.iter().all(|tag| item.tags.contains(tag))
            {
                return false;
            }
            if !needle.is_empty() {
                let haystack = format!("{} {}", item.name, item.category).to_lowercase();
                if !haystack.contains(&needle) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

/// Modal preset browser overlay. Renders nothing while `signals.open` is
/// false; when open, a dim layer (click closes) sits behind a centered panel
/// with search, category chips, tag chips, a favorites filter, the matching
/// rows, and a tag editor for the current preset. Clicking a row loads that
/// preset and closes the browser.
pub fn preset_browser(cx: &mut Context, lang: Language, signals: PresetBrowserSignals) {
    let open = signals.open;
    Binding::new(cx, open, move |cx| {
        if !open.get() {
            Element::new(cx).width(Pixels(0.0)).height(Pixels(0.0));
            return;
        }
        let search = signals.search;
        let category = signals.category;
        let favorites_only = signals.favorites_only;
        let categories = signals.categories;
        let filtered = signals.filtered;
        let tags = signals.tags;
        let selected_tags = signals.selected_tags;
        let tags_input = signals.tags_input;
        let current_name = signals.current_name;
        VStack::new(cx, move |cx| {
            // Dim layer: a sibling of the panel (not an ancestor), so clicks
            // inside the panel never reach it.
            Element::new(cx)
                .class("browser-dim")
                .position_type(PositionType::Absolute)
                .left(Pixels(0.0))
                .top(Pixels(0.0))
                .width(Stretch(1.0))
                .height(Stretch(1.0))
                .on_press(|cx| cx.emit(PresetBrowserAction::Close));
            Element::new(cx).height(Stretch(1.0));
            VStack::new(cx, move |cx| {
                HStack::new(cx, move |cx| {
                    Label::new(cx, translate(lang, "gui.preset-browser", "Preset Browser"))
                        .class("browser-title");
                    Element::new(cx).width(Stretch(1.0));
                    Button::new(cx, |cx| Label::new(cx, "×"))
                        .class("btn-action")
                        .on_press(|cx| cx.emit(PresetBrowserAction::Close))
                        .width(Pixels(30.0));
                })
                .height(Pixels(30.0));
                Textbox::new(cx, search)
                    .placeholder(translate(lang, "gui.search-presets", "Search presets…"))
                    .on_edit(|cx, text| cx.emit(PresetBrowserAction::SetSearch(text)))
                    .width(Stretch(1.0))
                    .height(Pixels(26.0));
                Binding::new(cx, category, move |cx| {
                    let selected = category.get();
                    HStack::new(cx, move |cx| {
                        let is_all = selected.is_none();
                        Button::new(cx, move |cx| {
                            Label::new(cx, translate(lang, "gui.all", "All"))
                        })
                        .class("browser-chip")
                        .toggle_class("browser-chip-active", is_all)
                        .on_press(|cx| cx.emit(PresetBrowserAction::SetCategory(None)));
                        Binding::new(cx, categories, move |cx| {
                            for cat in categories.get() {
                                let cat_filter = Some(cat.clone());
                                let is_active = selected == cat_filter;
                                let label = cat.clone();
                                Button::new(cx, move |cx| Label::new(cx, label.clone()))
                                    .class("browser-chip")
                                    .toggle_class("browser-chip-active", is_active)
                                    .on_press(move |cx| {
                                        cx.emit(PresetBrowserAction::SetCategory(
                                            cat_filter.clone(),
                                        ))
                                    });
                            }
                        });
                        Binding::new(cx, favorites_only, move |cx| {
                            let active = favorites_only.get();
                            Button::new(cx, move |cx| {
                                Label::new(cx, translate(lang, "gui.favorites", "Favorites"))
                            })
                            .class("browser-chip")
                            .toggle_class("browser-chip-active", active)
                            .on_press(|cx| cx.emit(PresetBrowserAction::ToggleFavoritesOnly));
                        });
                    })
                    .horizontal_gap(Pixels(6.0))
                    // Chip rows wrap instead of painting past the panel
                    // border: Vizia/morphorm expose flex-wrap. No fixed
                    // height, so a wrapped second line grows the row
                    // instead of being clipped by it.
                    .wrap(LayoutWrap::Wrap)
                    .min_height(Pixels(28.0));
                });
                Binding::new(cx, selected_tags, move |cx| {
                    let selected = selected_tags.get();
                    Binding::new(cx, tags, move |cx| {
                        let all_tags = tags.get();
                        if all_tags.is_empty() {
                            Element::new(cx).height(Pixels(0.0));
                            return;
                        }
                        // The row builder runs on every rebuild; clone the
                        // selection so the Fn closure stays callable.
                        let selected = selected.clone();
                        HStack::new(cx, move |cx| {
                            for tag in all_tags {
                                let tag_filter = tag.clone();
                                let is_active = selected.contains(&tag_filter);
                                let label = tag.clone();
                                Button::new(cx, move |cx| Label::new(cx, label.clone()))
                                    .class("browser-chip")
                                    .toggle_class("browser-chip-active", is_active)
                                    .on_press(move |cx| {
                                        cx.emit(PresetBrowserAction::ToggleTag(tag_filter.clone()))
                                    });
                            }
                        })
                        .horizontal_gap(Pixels(6.0))
                        .wrap(LayoutWrap::Wrap)
                        .min_height(Pixels(28.0));
                    });
                });
                ScrollView::new(cx, move |cx| {
                    Binding::new(cx, filtered, move |cx| {
                        let items = filtered.get();
                        if items.is_empty() {
                            Label::new(
                                cx,
                                translate(lang, "gui.no-presets-match", "No presets match"),
                            )
                            .class("browser-empty");
                        } else {
                            for item in items {
                                let id = item.id.clone();
                                let star_id = item.id.clone();
                                let row_name = item.name.clone();
                                let row_category = item.category.clone();
                                let row_tags = item.tags.join(", ");
                                let star_label =
                                    if item.is_favorite { "★" } else { "☆" }.to_string();
                                HStack::new(cx, move |cx| {
                                    Button::new(cx, move |cx| Label::new(cx, star_label.clone()))
                                        .class("browser-star")
                                        .on_press(move |cx| {
                                            cx.emit(PresetBrowserAction::ToggleFavorite(
                                                star_id.clone(),
                                            ))
                                        })
                                        .width(Pixels(28.0))
                                        .height(Stretch(1.0));
                                    Button::new(cx, move |cx| {
                                        HStack::new(cx, move |cx| {
                                            Label::new(cx, row_name.clone())
                                                .class("browser-row-name")
                                                .width(Stretch(1.0));
                                            if !row_tags.is_empty() {
                                                Label::new(cx, row_tags.clone())
                                                    .class("browser-row-tags")
                                                    .width(Pixels(130.0))
                                                    .text_overflow(TextOverflow::Ellipsis);
                                            }
                                            Label::new(cx, row_category.clone())
                                                .class("browser-row-category");
                                        })
                                        .width(Stretch(1.0))
                                    })
                                    .class("browser-row-button")
                                    .on_press(move |cx| {
                                        cx.emit(PresetBrowserAction::Select(id.clone()));
                                        cx.emit(PresetBrowserAction::Close);
                                    })
                                    .width(Stretch(1.0))
                                    .height(Stretch(1.0));
                                })
                                .class("browser-row")
                                .height(Pixels(30.0));
                            }
                        }
                    });
                })
                .width(Stretch(1.0))
                .height(Pixels(300.0));
                HStack::new(cx, move |cx| {
                    Label::new(cx, translate(lang, "gui.tags", "Tags")).class("browser-tags-label");
                    Label::new(cx, current_name.clone())
                        .class("browser-tags-label")
                        .width(Pixels(110.0))
                        .text_overflow(TextOverflow::Ellipsis);
                    Textbox::new(cx, tags_input)
                        .placeholder(translate(
                            lang,
                            "gui.tags-placeholder",
                            "Comma-separated tags",
                        ))
                        .on_edit(|cx, text| cx.emit(PresetBrowserAction::SetTagsInput(text)))
                        .on_submit(|cx, text, _| cx.emit(PresetBrowserAction::SetTags(text)))
                        .width(Stretch(1.0))
                        .height(Pixels(26.0));
                })
                .height(Pixels(30.0))
                .horizontal_gap(Pixels(8.0));
            })
            .class("browser-panel")
            .width(Pixels(560.0))
            .height(Pixels(520.0));
            Element::new(cx).height(Stretch(1.0));
        })
        .position_type(PositionType::Absolute)
        .left(Pixels(0.0))
        .top(Pixels(0.0))
        .width(Stretch(1.0))
        .height(Stretch(1.0))
        .alignment(Alignment::Center);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, name: &str, category: &str, favorite: bool) -> PresetBrowserItem {
        PresetBrowserItem {
            id: id.to_string(),
            name: name.to_string(),
            category: category.to_string(),
            is_favorite: favorite,
            tags: Vec::new(),
        }
    }

    fn tagged(
        id: &str,
        name: &str,
        category: &str,
        favorite: bool,
        tags: &[&str],
    ) -> PresetBrowserItem {
        let mut item = item(id, name, category, favorite);
        item.tags = tags.iter().map(|t| t.to_string()).collect();
        item
    }

    fn sample() -> Vec<PresetBrowserItem> {
        vec![
            item("a", "Steinway D Concert Grand", "Concert Grand", true),
            item("b", "Bright Pop & Jazz Grand", "Pop Grand", false),
            item("c", "Warm Intimate Chamber", "Chamber Grand", false),
        ]
    }

    #[test]
    fn filter_matches_name_and_category_case_insensitively() {
        let items = sample();
        let out = filter_browser_items(&items, "GRAND", None, false, &[]);
        assert_eq!(out.len(), 3);
        let out = filter_browser_items(&items, "chamber", None, false, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "c");
        let out = filter_browser_items(&items, "pop", None, false, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "b");
    }

    #[test]
    fn filter_category_and_favorites_combine() {
        let items = sample();
        let out = filter_browser_items(&items, "", Some("Concert Grand"), false, &[]);
        assert_eq!(out.len(), 1);
        let out = filter_browser_items(&items, "", None, true, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "a");
        let out = filter_browser_items(&items, "", Some("Pop Grand"), true, &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn filter_selected_tags_require_all() {
        let items = vec![
            tagged("a", "Alpha", "General", false, &["warm", "jazz"]),
            tagged("b", "Beta", "General", false, &["warm"]),
            tagged("c", "Gamma", "General", false, &["bright"]),
        ];
        let warm = vec!["warm".to_string()];
        let out = filter_browser_items(&items, "", None, false, &warm);
        assert_eq!(out.len(), 2);
        let both = vec!["warm".to_string(), "jazz".to_string()];
        let out = filter_browser_items(&items, "", None, false, &both);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "a");
        // Tags combine with the other filters.
        let out = filter_browser_items(&items, "beta", None, false, &warm);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "b");
        let missing = vec!["nope".to_string()];
        let out = filter_browser_items(&items, "", None, false, &missing);
        assert!(out.is_empty());
    }

    #[test]
    fn tags_keep_first_seen_order() {
        let items = vec![
            tagged("a", "A", "General", false, &["warm", "jazz"]),
            tagged("b", "B", "General", false, &["bright", "warm"]),
        ];
        assert_eq!(preset_browser_tags(&items), vec!["warm", "jazz", "bright"]);
    }

    #[test]
    fn parse_tags_input_splits_and_dedupes() {
        assert_eq!(
            parse_tags_input("warm, jazz ,warm,,  ,bright"),
            vec!["warm", "jazz", "bright"]
        );
        assert!(parse_tags_input(" , ,").is_empty());
        assert!(parse_tags_input("").is_empty());
    }

    #[test]
    fn parse_tags_input_dedupes_case_insensitively_keeping_first_spelling() {
        assert_eq!(
            parse_tags_input("Warm, warm, WARM, Jazz, jazz"),
            vec!["Warm", "Jazz"]
        );
        assert_eq!(parse_tags_input("  Bright ,bright"), vec!["Bright"]);
    }

    #[test]
    fn categories_keep_first_seen_order() {
        let items = vec![
            item("a", "A", "Jazz", false),
            item("b", "B", "Pop", false),
            item("c", "C", "Jazz", false),
        ];
        assert_eq!(preset_browser_categories(&items), vec!["Jazz", "Pop"]);
    }
}
