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
}
