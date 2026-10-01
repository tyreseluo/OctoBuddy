//! English or Chinese for OctoBuddy's own words: the interface, its
//! system lines. What the agents write, and what they are told (their
//! prompts), stay as they are.
//!
//! Each string is written in both languages where it is used, `t` for a
//! fixed one and `pick` for one with values in it: no key tables to keep in
//! step with the code.
use std::cell::Cell;

// The view and the loops' wiring run on the UI thread; so does this.
thread_local! {
    static ZH: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lang {
    En,
    Zh,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Zh => "zh",
        }
    }

    pub fn parse(code: &str) -> Option<Lang> {
        match code {
            "en" => Some(Lang::En),
            "zh" => Some(Lang::Zh),
            _ => None,
        }
    }

    /// The system's language when the person has not picked one: Chinese
    /// for a `zh` locale, else English.
    pub fn from_env() -> Lang {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty())).unwrap_or_default();
        if locale.to_ascii_lowercase().starts_with("zh") { Lang::Zh } else { Lang::En }
    }
}

pub fn set(lang: Lang) {
    ZH.with(|z| z.set(lang == Lang::Zh));
}

pub fn lang() -> Lang {
    if zh() { Lang::Zh } else { Lang::En }
}

pub fn zh() -> bool {
    ZH.with(Cell::get)
}

/// A fixed string in the current language.
pub fn t(en: &'static str, zh: &'static str) -> &'static str {
    if self::zh() { zh } else { en }
}

/// A string with values in it, in the current language.
pub fn pick(en: String, zh: String) -> String {
    if self::zh() { zh } else { en }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_languages() {
        set(Lang::Zh);
        assert_eq!(t("Send", "发送"), "发送");
        assert_eq!(pick("2 done".into(), "2 个完成".into()), "2 个完成");
        set(Lang::En);
        assert_eq!(t("Send", "发送"), "Send");
        assert_eq!(Lang::parse("zh"), Some(Lang::Zh));
        assert_eq!(Lang::parse("fr"), None);
    }
}
