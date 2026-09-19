use i18n_embed::fluent::{fluent_language_loader, FluentLanguageLoader};
use i18n_embed::{DefaultLocalizer, DesktopLanguageRequester, LanguageLoader, Localizer};
use rust_embed::RustEmbed;
use std::sync::LazyLock;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("clipit: failed to load fallback language");
    loader
});

/// Loads the system language once; call before any `fl!` use.
pub fn localize() {
    static ONCE: LazyLock<()> = LazyLock::new(|| {
        let localizer = Box::from(DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations));
        if let Err(error) = localizer.select(&DesktopLanguageRequester::requested_languages()) {
            eprintln!("clipit: language selection failed: {error}");
        }
    });
    LazyLock::force(&ONCE);
}

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
    };
    ($message_id:literal, $($arg_name:ident = $arg_value:expr),+ $(,)?) => {
        i18n_embed_fl::fl!(
            $crate::i18n::LANGUAGE_LOADER,
            $message_id,
            $($arg_name = $arg_value),+
        )
    };
}

#[cfg(test)]
mod tests {
    use super::{fluent_language_loader, DefaultLocalizer, FluentLanguageLoader, Localizations};
    use i18n_embed::{LanguageLoader, Localizer};

    fn strip(s: String) -> String {
        s.replace(['\u{2068}', '\u{2069}'], "")
    }

    #[test]
    fn fallback_strings_resolve() {
        assert_eq!(crate::fl!("clear"), "Clear");
        assert_eq!(crate::fl!("copy"), "Copy");
    }

    #[test]
    fn plurals_resolve_by_number() {
        assert_eq!(
            strip(crate::fl!("stats", items = 3, pinned = 1)),
            "3 items · 1 pin"
        );
        assert_eq!(
            strip(crate::fl!("stats", items = 1, pinned = 0)),
            "1 item · 0 pins"
        );
        assert_eq!(strip(crate::fl!("expire-days", days = 7)), "7 days");
        assert_eq!(strip(crate::fl!("expire-days", days = 1)), "1 day");
    }

    #[test]
    fn all_locales_cover_every_message() {
        let ids = |path: &std::path::Path| {
            let content = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let mut set = std::collections::BTreeSet::new();
            for line in content.lines() {
                if let Some((id, _)) = line.split_once('=') {
                    let id = id.trim();
                    if !id.is_empty() && id.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                    {
                        set.insert(id.to_string());
                    }
                }
            }
            set
        };
        let base = std::path::Path::new("i18n");
        let en = ids(&base.join("en").join("clipit.ftl"));
        assert!(!en.is_empty());
        let mut checked = 0;
        for entry in std::fs::read_dir(base)
            .expect("i18n dir must exist")
            .flatten()
        {
            let path = entry.path().join("clipit.ftl");
            if !path.is_file() {
                continue;
            }
            let locale = entry.file_name();
            if locale == "en" {
                continue;
            }
            let ids = ids(&path);
            let missing: Vec<_> = en.difference(&ids).collect();
            let extra: Vec<_> = ids.difference(&en).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "locale {}: missing {missing:?}, extra {extra:?}",
                locale.to_string_lossy()
            );
            checked += 1;
        }
        assert!(checked >= 25, "expected 25+ locales, found {checked}");
    }

    #[test]
    fn german_and_russian_bundles_parse_and_select() {
        let loader: FluentLanguageLoader = fluent_language_loader!();
        loader
            .load_fallback_language(&Localizations)
            .expect("fallback must load");
        let localizer = DefaultLocalizer::new(&loader, &Localizations);

        let de: unic_langid::LanguageIdentifier = "de".parse().unwrap();
        localizer.select(&[de]).unwrap();
        assert_eq!(i18n_embed_fl::fl!(loader, "clear"), "Leeren");

        let ru: unic_langid::LanguageIdentifier = "ru".parse().unwrap();
        localizer.select(&[ru]).unwrap();
        let strip = |s: String| s.replace(['\u{2068}', '\u{2069}'], "");
        assert_eq!(strip(i18n_embed_fl::fl!(loader, "clear")), "Очистить");
        assert_eq!(
            strip(i18n_embed_fl::fl!(loader, "expire-days", days = 2)),
            "2 дня"
        );
        assert_eq!(
            strip(i18n_embed_fl::fl!(loader, "expire-days", days = 5)),
            "5 дней"
        );
    }
}
