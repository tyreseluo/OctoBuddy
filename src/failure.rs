//! What went wrong, in words a person can act on, and what still works.
use crate::i18n;

/// The kinds of failure a person can do something about, each said its own way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A program is not on this machine.
    MissingBinary,
    /// No key, or not logged in.
    NoAuth,
    /// The provider limits requests (or the account has no credit left).
    RateLimit,
    /// The network, or the provider's service, cannot be reached.
    Offline,
    Timeout,
    /// An answer OctoBuddy could not read.
    Protocol,
    /// The person said no.
    Denied,
    Other,
}

/// `code` in `t` as a number of its own, not part of a longer one.
fn number(t: &str, code: &str) -> bool {
    t.match_indices(code).any(|(i, _)| {
        !t[..i].ends_with(|c: char| c.is_ascii_digit()) && !t[i + code.len()..].starts_with(|c: char| c.is_ascii_digit())
    })
}

/// An error's kind, by what it says (any case). In order: auth, then the
/// rate limit (its message may name the key it limits), the network (a
/// refused connection to this machine is OctoBuddy's own proxy, not the
/// network), a timeout, a missing program (a model or a page not found is
/// not one), an answer it could not read.
pub fn classify(err: &str) -> Kind {
    let t = err.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| t.contains(w));
    let local = any(&["127.0.0.1", "localhost", "[::1]"]);
    if number(&t, "401") || number(&t, "403") || any(&["unauthorized", "authentication", "invalid api key", "invalid_api_key", "invalid x-api-key", "no key for",
        "not logged in", "/login", "gh auth", "token has expired"]) {
        Kind::NoAuth
    } else if number(&t, "429") || number(&t, "1302") || any(&["rate limit", "rate-limit", "rate_limit", "too many requests", "quota", "overloaded", "insufficient balance",
        "balance is too low", "usage limit", "payment required", "限流", "余额不足"]) {
        Kind::RateLimit
    } else if any(&["api key", "api_key", "login"]) {
        Kind::NoAuth
    } else if !local && any(&["dns", "failed to lookup", "could not resolve", "network is unreachable", "network error", "connection refused", "econnrefused",
        "connection error", "unable to connect", "enotfound", "getaddrinfo", "error sending request", "no route to host", "fetch failed", "offline"]) {
        Kind::Offline
    } else if any(&["timed out", "timeout", "etimedout", "deadline"]) {
        Kind::Timeout
    } else if number(&t, "os error 2") || any(&["no such file", "enoent", "command not found", "not on this machine", "not installed"])
        || (t.contains("not found") && !number(&t, "404") && !t.contains("model")) {
        Kind::MissingBinary
    } else if number(&t, "502") || any(&["bad gateway", "upstream", "parse", "parsing", "invalid json", "expected value", "unexpected"]) {
        Kind::Protocol
    } else {
        Kind::Other
    }
}

/// What happened, in plain words, what still works or what to do next, and
/// the raw error last (on one line, at most 200 characters). `who`: the
/// agent, model or program it is about; for `Denied`, `raw` is what was asked.
pub fn say(kind: Kind, who: &str, raw: &str) -> String {
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut raw: String = flat.chars().take(200).collect();
    if raw.len() < flat.len() {
        raw.push('…');
    }
    let said = match kind {
        Kind::MissingBinary => i18n::pick(format!("{who} isn't installed here: OctoBuddy installs its own copy (Settings › Tools, where you can also choose yours), then send again. Nothing was lost."),
            format!("本机没有 {who}：OctoBuddy 会安装自带的一份（设置 › 工具，也可以在那里改用你自己的），装好后再发一次。什么都没丢。")),
        Kind::NoAuth => i18n::pick(format!("No key or login for {who}: add one in Settings › AI Providers (or log in to its CLI), then send again. Your conversation is kept."),
            format!("{who} 没有 key 或没有登录：在 设置 › AI Providers 里添加（或登录它的 CLI），再发一次。对话都还在。")),
        Kind::RateLimit => i18n::pick(format!("{who}'s provider is limiting requests (or its account is out of credit): wait a minute, or pick another model in the picker. Your conversation is kept."),
            format!("{who} 的供应商在限流（或账户余额不足）：等一分钟再试，或在选择器里换个模型。对话都还在。")),
        Kind::Offline => i18n::pick(format!("OctoBuddy can't reach {who}'s service. Nothing was lost: your local files, the timeline and published apps still work; send again when you're online."),
            format!("OctoBuddy 连不上 {who} 的服务。什么都没丢：本地文件、时间线和已发布的应用照常可用；联网后再发一次。")),
        Kind::Timeout => i18n::pick(format!("{who} took too long to answer, and OctoBuddy stopped waiting. Nothing was lost: send again, or pick a faster model in the picker."),
            format!("{who} 太久没有回应，OctoBuddy 不再等了。什么都没丢：再发一次，或在选择器里换个更快的模型。")),
        Kind::Protocol => i18n::pick(format!("{who}'s provider answered something OctoBuddy couldn't read: try again, or pick another model in the picker."),
            format!("{who} 的供应商返回了 OctoBuddy 读不懂的内容：再试一次，或在选择器里换个模型。")),
        Kind::Denied => {
            let what = if raw.is_empty() { String::new() } else { i18n::pick(format!(" ({raw})"), format!("（{raw}）")) };
            return i18n::pick(format!("You denied {who}'s request{what}; it goes on without it and may report blocked."),
                format!("你拒绝了 {who} 的请求{what}；它会不带这一步继续，也可能报告受阻。"));
        }
        Kind::Other => i18n::pick(format!("{who} stopped on an error. Nothing was lost: send again, or pick another model in the picker."),
            format!("{who} 出错停下了。什么都没丢：再发一次，或在选择器里换个模型。")),
    };
    if raw.is_empty() { said } else { i18n::pick(format!("{said} Its error: {raw}"), format!("{said}原始错误：{raw}")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    #[test]
    fn real_errors_are_classified() {
        let table: &[(&str, Kind)] = &[
            // providers.rs's route; Claude Code; Codex; pi; gh.
            ("no key for zai/glm-4.6 (set it in AI providers)", Kind::NoAuth),
            ("Invalid API key · Please run /login", Kind::NoAuth),
            (r#"API Error: 401 {"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#, Kind::NoAuth),
            ("stream error: unexpected status 401 Unauthorized", Kind::NoAuth),
            ("No API key found for provider anthropic", Kind::NoAuth),
            ("To get started with GitHub CLI, please run:  gh auth login", Kind::NoAuth),
            // A rate limit that names the key it limits is a rate limit.
            ("Rate limit reached for requests on this API key", Kind::RateLimit),
            (r#"API Error: 429 {"type":"error","error":{"type":"rate_limit_error","message":"This request would exceed the rate limit for your organization"}}"#, Kind::RateLimit),
            ("exceeded retry limit, last status: 429 Too Many Requests", Kind::RateLimit),
            (r#"{"error":{"code":"1302","message":"您的账户已达到速率限制，请您控制请求频率"}}"#, Kind::RateLimit),
            ("LLM error: HTTP 402 Payment Required: Insufficient Balance", Kind::RateLimit),
            (r#"API Error: 529 {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#, Kind::RateLimit),
            // OctoBuddy's proxy's 502 for a host it could not resolve: offline, not a bad answer.
            (r#"API Error: 502 {"type":"error","error":{"type":"api_error","message":"upstream: curl: (6) Could not resolve host: open.bigmodel.cn"}}"#, Kind::Offline),
            ("stream disconnected before completion: error sending request for url (https://chatgpt.com/backend-api/codex/responses)", Kind::Offline),
            ("dns error: failed to lookup address information: nodename nor servname provided, or not known", Kind::Offline),
            ("API Error: Connection error.", Kind::Offline),
            // A refused connection to this machine: OctoBuddy's own proxy, not the network.
            ("error sending request for url (http://127.0.0.1:52311/v1/chat/completions): client error (Connect): tcp connect error: Connection refused (os error 61)", Kind::Other),
            ("upstream: curl: (28) Operation timed out after 30002 milliseconds with 0 bytes received", Kind::Timeout),
            ("timed out after 600s", Kind::Timeout),
            ("could not start claude: No such file or directory (os error 2)", Kind::MissingBinary),
            ("claude is not on this machine yet: OctoBuddy is installing its copy (2.1.4). Settings › Tools shows how it goes; send again once it is ready.", Kind::MissingBinary),
            ("pi needs Node.js and npm (not found)", Kind::MissingBinary),
            (r#"API Error: 404 {"error":{"message":"model not found"}}"#, Kind::Other),
            ("Is a directory (os error 21)", Kind::Other),
            ("codex could not start: expected value at line 1 column 1", Kind::Protocol),
            ("API Error: 502 Bad Gateway", Kind::Protocol),
            ("could not write to the lead: Broken pipe (os error 32)", Kind::Other),
            ("the lead reported an error", Kind::Other),
        ];
        for (err, kind) in table {
            assert_eq!(classify(err), *kind, "{err}");
        }
    }

    #[test]
    fn it_says_who_and_what_next_in_both_languages() {
        let raw = format!("API Error: 429 {}", "x".repeat(400));
        i18n::set(Lang::En);
        let en = say(classify(&raw), "zai/glm-4.6", &raw);
        assert!(en.contains("zai/glm-4.6") && en.contains("pick another model") && en.contains("conversation is kept"), "{en}");
        assert!(en.ends_with('…') && en.contains("Its error: API Error: 429") && !en.contains(&"x".repeat(200)), "the raw error comes last, cut: {en}");
        let offline = say(Kind::Offline, "Claude Code", "API Error: Connection error.\n");
        assert!(offline.contains("Claude Code") && offline.contains("send again when you're online") && offline.ends_with("Its error: API Error: Connection error."), "{offline}");
        assert!(say(Kind::NoAuth, "Codex", "").contains("Settings › AI Providers"));
        assert_eq!(say(Kind::Denied, "tests", "run `cargo test`"), "You denied tests's request (run `cargo test`); it goes on without it and may report blocked.");
        i18n::set(Lang::Zh);
        let zh = say(Kind::RateLimit, "zai/glm-4.6", "429 Too Many Requests");
        assert!(zh.contains("zai/glm-4.6") && zh.contains("换个模型") && zh.ends_with("原始错误：429 Too Many Requests"), "{zh}");
        assert!(say(Kind::MissingBinary, "Claude Code", "").contains("设置 › 工具"));
        assert!(say(Kind::Denied, "tests", "联网").starts_with("你拒绝了 tests 的请求（联网）"));
        i18n::set(Lang::En);
    }
}
