use std::{borrow::Cow, collections::HashMap, fmt::Display, sync::Mutex};

use fluent_templates::{LanguageIdentifier, Loader, fluent_bundle::FluentValue};
use once_cell::sync::Lazy;

fluent_templates::static_loader! {
    static LOCALES = {
        locales: "locales",
        fallback_language: "en",
    };
}
static USE_LOCALE: Lazy<Mutex<Option<LanguageIdentifier>>> = Lazy::new(|| Mutex::new(None));

/// Initialises i18n and returned the locale identifier
pub fn initialise_i18n() -> LanguageIdentifier {
    let mut locale_is_default = true;

    let avail_locales = LOCALES.locales().collect::<Vec<_>>();
    for locale in sys_locale::get_locales() {
        tracing::info!("System offers locale: {locale}");

        if let Ok(lang_id) = locale.parse::<LanguageIdentifier>() {
            for possible_locale in &avail_locales {
                if possible_locale == &&lang_id {
                    tracing::info!("This locale is available! Using: {locale}");
                    let mut use_locale = USE_LOCALE.lock().unwrap();
                    use_locale.replace((*possible_locale).clone());
                    locale_is_default = false;
                    break;
                } else if possible_locale.language == lang_id.language {
                    tracing::info!("This language is available! Using: {}", lang_id.language);
                    let mut use_locale = USE_LOCALE.lock().unwrap();
                    use_locale.replace((*possible_locale).clone());
                    locale_is_default = false;
                    break;
                }
            }
        }
    }
    if locale_is_default {
        tracing::info!("No suitable locale found, using default.");
        let mut use_locale = USE_LOCALE.lock().unwrap();
        use_locale.replace("en".parse().unwrap()); // en fallback
    }

    let use_locale = USE_LOCALE.lock().unwrap();
    use_locale.clone().unwrap()
}

/// Get the current locale identifier
pub(crate) fn current_locale() -> LanguageIdentifier {
    let use_locale = USE_LOCALE.lock().unwrap();
    use_locale.clone().unwrap()
}

/// Lookup a string
pub(crate) fn lookup<S>(text_id: S) -> String
where
    S: AsRef<str> + Display,
{
    LOCALES.lookup(&current_locale(), text_id.as_ref())
}

/// Lookup a string with args
pub(crate) fn lookup_with_args<S>(
    text_id: S,
    args: &HashMap<Cow<'static, str>, FluentValue<'_>>,
) -> String
where
    S: AsRef<str> + Display,
{
    LOCALES.lookup_with_args(&current_locale(), text_id.as_ref(), args)
}

#[macro_export]
macro_rules! lang_args {
    ($($name: expr, $val: expr),*) => {
        &{
            let mut map = ::std::collections::HashMap::new();
            $(
                map.insert($name.into(), $val.into());
            )*
            map
        }
    };
}
