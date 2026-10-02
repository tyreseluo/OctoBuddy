//! Settings › AI Providers in the window. Inside OctoSense it shows the
//! shell's AI providers, read-only (`providers.rs`). On the host it is
//! OctoBuddy's own (`own_providers.rs`), set up the way Cindy does it: a
//! list (primary first) and a three-step wizard — pick a provider, connect
//! it (its endpoint, its key, a test), pick its models — plus what
//! OctoSense's AI providers has here (import it, or use it directly) and
//! the agents' own sign-ins. Which agents can run on a provider is said by
//! its endpoint's protocol, row by row.
use crate::events::{self, LoopEvent};
use crate::own_providers::{self, Group, Source};
use crate::{i18n, providers, system, tapped, workspace, OctoBuddyView, SettingsTab};
use makepad_widgets::*;
use octosense_llm_config::catalog::{CatalogFamily, Model, Route};
use octosense_llm_config::Provider;
use std::collections::BTreeSet;

/// What a background job of the page was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvidersJob {
    /// The wizard's connection test.
    Test,
    /// The wizard's providers added.
    Add,
    Import,
    Remove(String),
    /// A row's test (its label).
    TestRow(String),
}

/// The add wizard, open.
#[derive(Default)]
pub(crate) struct Wizard {
    /// 1 pick a provider, 2 connect it, 3 pick its models.
    step: u8,
    family: Option<&'static CatalogFamily>,
    /// Into the family's `routes()`.
    route: usize,
    checked: BTreeSet<String>,
    status: Option<Result<String, String>>,
    busy: bool,
}

/// A row of the wizard's list.
enum Row {
    Header(String),
    Family(&'static CatalogFamily),
    Route(usize, Route),
    Model(&'static Model),
}

impl Wizard {
    fn route(&self) -> Option<Route> {
        self.family.and_then(|f| f.routes().into_iter().nth(self.route))
    }

    /// The family's models served on the chosen endpoint.
    fn models(&self) -> Vec<&'static Model> {
        let (Some(f), Some(route)) = (self.family, self.route()) else { return Vec::new() };
        f.models.iter().filter(|m| m.routes().iter().any(|r| r.id == route.id)).collect()
    }

    /// The model a test runs on: the family's default, else its first.
    fn test_model(&self) -> Option<&'static Model> {
        let models = self.models();
        models.iter().find(|m| m.default).or(models.first()).copied()
    }

    /// Its endpoint is one's own (the catalog knows no URL for it).
    fn needs_base(&self) -> bool {
        self.family.is_some_and(|f| f.family.default_base_url.is_none()) && self.route().is_some_and(|r| r.base_url.is_none())
    }

    fn rows(&self) -> Vec<Row> {
        let Some(f) = self.family.filter(|_| self.step >= 2) else {
            let mut out = Vec::new();
            for (group, title) in [
                (Group::Coding, i18n::t("Coding plans · recommended for coding agents", "Coding Plan · 推荐给编程 agent")),
                (Group::More, i18n::t("More providers", "更多供应商")),
                (Group::Local, i18n::t("On this computer or your own server", "本机 · 自托管")),
            ] {
                let fams: Vec<_> = own_providers::families().into_iter().filter(|f| own_providers::group_of(f) == group).collect();
                if !fams.is_empty() {
                    out.push(Row::Header(title.to_string()));
                    out.extend(fams.into_iter().map(Row::Family));
                }
            }
            return out;
        };
        if self.step == 2 {
            let mut out = vec![Row::Header(i18n::t("Endpoint", "接入点").to_string())];
            out.extend(f.routes().into_iter().enumerate().map(|(i, r)| Row::Route(i, r)));
            return out;
        }
        let on = self.route().map(|r| r.label).unwrap_or_default();
        let mut out = vec![Row::Header(i18n::pick(format!("Models on {on}"), format!("{on} 上的模型")))];
        out.extend(self.models().into_iter().map(Row::Model));
        out
    }

    /// The provider `model` makes, with the base URL entered for one's own endpoint.
    fn provider(&self, model: &Model, base: &str) -> Option<Provider> {
        let (f, route) = (self.family?, self.route()?);
        let mut p = own_providers::provider(f, model, &route);
        if self.needs_base() {
            p.base_url = Some(base.trim().to_string()).filter(|b| !b.is_empty());
        }
        Some(p)
    }
}

/// The agents that can run on `p`, said.
fn agents_text(p: &Provider) -> String {
    let agents = own_providers::agents_for(p).join(" · ");
    i18n::pick(format!("Runs: {agents}"), format!("可运行：{agents}"))
}

fn base_url_of(p: &Provider) -> String {
    p.base_url.clone()
        .or_else(|| octosense_llm_config::registry::lookup(&p.family).and_then(|f| f.default_base_url).map(String::from))
        .unwrap_or_default()
}

fn open_url(url: &str) {
    let program = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
    let _ = std::process::Command::new(program).arg(url).spawn();
}

/// The agents' own sign-ins (a Claude or ChatGPT subscription), said
/// without who is signed in. Blocks: off the UI thread.
fn probe_logins() -> Vec<(String, String)> {
    let run = |name: &str, args: &[&str]| -> Option<String> {
        let bin = workspace::find_bin(name);
        let mut child = std::process::Command::new(&bin).args(args)
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
            .spawn().ok()?;
        let started = std::time::Instant::now();
        while child.try_wait().ok()?.is_none() {
            if started.elapsed().as_secs() > 15 {
                let _ = child.kill();
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let out = child.wait_with_output().ok()?;
        Some(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
    };
    let not_found = || i18n::t("not installed", "未安装").to_string();
    let claude = match run("claude", &["auth", "status"]) {
        None => not_found(),
        Some(out) => match serde_json::from_str::<serde_json::Value>(out.trim()) {
            Ok(v) if v["loggedIn"].as_bool() == Some(true) => {
                let how = [v["authMethod"].as_str(), v["subscriptionType"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(", ");
                i18n::pick(format!("signed in ({how})"), format!("已登录（{how}）"))
            }
            Ok(_) => i18n::t("not signed in", "未登录").to_string(),
            Err(_) => i18n::t("could not tell", "无法判断").to_string(),
        },
    };
    let codex = match run("codex", &["login", "status"]) {
        None => not_found(),
        // Its first line, up to anything of a key ("… - sk-…").
        Some(out) => out.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.split(" - ").next().unwrap_or(l))
            .map(|l| l.chars().take(80).collect()).unwrap_or_else(|| i18n::t("could not tell", "无法判断").to_string()),
    };
    vec![("Claude Code".into(), claude), ("Codex".into(), codex)]
}

impl OctoBuddyView {
    /// Settings › AI Providers, as the state says.
    pub(crate) fn sync_providers_page(&mut self, cx: &mut Cx) {
        let hosted = system::hosted();
        let named = std::env::var_os("OCTOBUDDY_PROVIDERS").is_some();
        let host = !hosted && !named;
        let own = host && own_providers::source() == Source::Own;
        let path = self.providers.profile_path.display().to_string();
        let hint = if hosted {
            i18n::pick(format!("Enabled in OctoSense's AI providers app ({path}). OctoBuddy only reads them: change them there, then press Reload."),
                format!("来自 OctoSense 的 AI Providers 应用（{path}）。OctoBuddy 只读取：请在那里修改，然后点「重新载入」。"))
        } else if named {
            i18n::pick(format!("Read from the profile OCTOBUDDY_PROVIDERS names ({path}), read-only."),
                format!("读取 OCTOBUDDY_PROVIDERS 指定的配置（{path}），只读。"))
        } else if own {
            i18n::pick(format!("OctoBuddy's own ({path}), in the same form as OctoSense's AI providers (inside OctoSense, OctoBuddy uses OctoSense's). Keys stay in the system keychain; agents never get them: OctoBuddy's local proxy adds them."),
                format!("OctoBuddy 自己的（{path}），格式与 OctoSense 的 AI Providers 相同（在 OctoSense 里运行时改用 OctoSense 的）。Key 存在系统钥匙串里，从不交给 agent：由 OctoBuddy 的本机代理加上。"))
        } else {
            i18n::pick(format!("Read from OctoSense's AI providers ({path}), read-only: change them in OctoSense, then press Reload. Or use OctoBuddy's own."),
                format!("读取 OctoSense 的 AI Providers（{path}），只读：请在 OctoSense 里修改，然后点「重新载入」。也可以改用 OctoBuddy 自己的。"))
        };
        self.view.label(cx, ids!(providers_hint)).set_text(cx, &hint);

        let open = self.wizard.is_some();
        let theirs = &self.providers.octosense;
        self.view.view(cx, ids!(os_found)).set_visible(cx, host && !open && !theirs.is_empty());
        if !theirs.is_empty() {
            let (n, labels) = (theirs.len(), theirs.iter().take(4).cloned().collect::<Vec<_>>().join(", "));
            let more = if theirs.len() > 4 { " …" } else { "" };
            self.view.label(cx, ids!(os_found_text)).set_text(cx, &i18n::pick(
                format!("OctoSense's AI providers on this computer has {n} OctoBuddy's own does not: {labels}{more}. Import adds them (keys into OctoBuddy's own keychain items; OctoSense's stay as they are)."),
                format!("这台电脑上 OctoSense 的 AI Providers 有 {n} 个 OctoBuddy 自己没有的：{labels}{more}。「导入」会把它们加进来（key 存入 OctoBuddy 自己的钥匙串条目，OctoSense 的不动）。")));
        }
        self.view.view(cx, ids!(own_bar)).set_visible(cx, host && !open);
        self.view.label(cx, ids!(src_label)).set_text(cx, i18n::t("Use:", "使用："));
        for (id, show) in [(ids!(src_own_on), own), (ids!(src_own), !own), (ids!(src_os_on), !own), (ids!(src_os), own), (ids!(add_provider), own)] {
            self.view.button(cx, id).set_visible(cx, show);
        }
        for id in [ids!(provider_box), ids!(inner_box)] {
            self.view.view(cx, id).set_visible(cx, !open);
        }
        self.view.label(cx, ids!(providers_hint)).set_visible(cx, !open);
        let note = self.own_note.as_ref().map(|r| match r { Ok(s) => format!("✓ {s}"), Err(e) => format!("✗ {e}") });
        let status = self.view.label(cx, ids!(own_status));
        status.set_visible(cx, !open && note.is_some());
        status.set_text(cx, note.as_deref().unwrap_or(""));
        self.view.view(cx, ids!(wizard)).set_visible(cx, open);
        self.sync_wizard(cx);

        // The agents' own sign-ins: probed once the page is seen.
        if self.logins.is_none() && !self.logins_probing && self.settings_tab == SettingsTab::Providers && self.page == crate::Page::Settings {
            self.logins_probing = true;
            let inbox = self.rt.inbox.clone();
            std::thread::spawn(move || events::post(&inbox, LoopEvent::LoginsProbed(probe_logins())));
        }
        let logins = self.logins.as_ref().map(|l| l.iter().map(|(a, s)| format!("{a}: {s}")).collect::<Vec<_>>().join("\n"))
            .unwrap_or_else(|| i18n::t("Checking…", "正在检查…").to_string());
        let logins = i18n::pick(format!("{logins}\nAn agent with no provider picked runs on its own sign-in (Claude Code: `claude auth login`; Codex: `codex login`)."),
            format!("{logins}\n没选 provider 时，agent 用它自己的登录（Claude Code：`claude auth login`；Codex：`codex login`）。"));
        self.view.label(cx, ids!(logins_title)).set_text(cx, i18n::t("The agents' own sign-in", "Agent 自己的登录"));
        self.view.label(cx, ids!(logins)).set_text(cx, &logins);
        for id in [ids!(logins_title), ids!(logins)] {
            self.view.label(cx, id).set_visible(cx, !open);
        }
        self.view.portal_list(cx, ids!(provider_list)).redraw(cx);
        self.view.portal_list(cx, ids!(wz_list)).redraw(cx);
    }

    fn sync_wizard(&mut self, cx: &mut Cx) {
        let Some(w) = &self.wizard else { return };
        let step = w.step;
        let title = match w.family.filter(|_| step >= 2) {
            Some(f) => f.label().to_string(),
            None => i18n::t("Add a provider", "添加供应商").to_string(),
        };
        let steps = [i18n::t("Provider", "选择供应商"), i18n::t("Connect", "连接"), i18n::t("Models", "选择模型")];
        let steps = steps.iter().enumerate().map(|(i, s)| if i + 1 == step as usize { format!("[{} {s}]", i + 1) } else { format!("{} {s}", i + 1) })
            .collect::<Vec<_>>().join("  ›  ");
        self.view.label(cx, ids!(wz_title)).set_text(cx, &title);
        self.view.label(cx, ids!(wz_steps)).set_text(cx, &steps);
        self.view.view(cx, ids!(wz_connect)).set_visible(cx, step == 2);
        if step == 2 {
            let needs_base = w.needs_base();
            self.view.view(cx, ids!(wz_base_box)).set_visible(cx, needs_base);
            self.view.label(cx, ids!(wz_base_label)).set_text(cx, i18n::t("Its base URL (an OpenAI-compatible /v1)", "接入地址（OpenAI 兼容的 /v1）"));
            let probe = w.test_model().and_then(|m| w.provider(m, &self.view.text_input(cx, ids!(wz_base)).text()));
            let (required, env) = (w.family.is_some_and(|f| f.family.key_required), probe.as_ref().map(|p| p.key_env.clone()).unwrap_or_default());
            let kept = !env.is_empty() && own_providers::has_key(&env);
            let label = match (required, kept) {
                (_, true) => i18n::pick(format!("API key ({env}) · leave it empty to keep the one OctoBuddy has"), format!("API key（{env}）· 留空则沿用已保存的")),
                (true, false) => i18n::pick(format!("API key ({env})"), format!("API key（{env}）")),
                (false, false) => i18n::t("API key (optional)", "API key（可选）").to_string(),
            };
            self.view.label(cx, ids!(wz_key_label)).set_text(cx, &label);
            let page = w.family.and_then(|f| own_providers::key_page(f.id()));
            self.view.button(cx, ids!(wz_key_link)).set_visible(cx, page.is_some());
            self.view.button(cx, ids!(wz_key_link)).set_text(cx, i18n::t("Get an API key…", "获取 API Key…"));
            // Where its key goes, said.
            let note = if env.is_empty() { String::new() } else if cfg!(target_os = "macos") {
                let account = format!("{env}{}", own_providers::ACCOUNT_SUFFIX);
                i18n::pick(format!("The key goes to the macOS keychain (octos · {account}); agents get a placeholder, OctoBuddy's local proxy adds it."),
                    format!("Key 存入 macOS 钥匙串（octos · {account}）；agent 只拿到占位符，由 OctoBuddy 的本机代理加上。"))
            } else {
                i18n::t("The key goes into OctoBuddy's profile (only you can read it); agents get a placeholder, OctoBuddy's local proxy adds it.",
                    "Key 存入 OctoBuddy 的配置文件（只有你能读）；agent 只拿到占位符，由 OctoBuddy 的本机代理加上。").to_string()
            };
            self.view.label(cx, ids!(wz_note)).set_text(cx, &note);
        }
        let n = w.checked.len();
        for (id, show) in [(ids!(wz_back), step >= 2), (ids!(wz_test), step == 2), (ids!(wz_next), step == 2), (ids!(wz_finish), step == 3)] {
            self.view.button(cx, id).set_visible(cx, show);
        }
        self.view.button(cx, ids!(wz_cancel)).set_text(cx, i18n::t("Cancel", "取消"));
        self.view.button(cx, ids!(wz_back)).set_text(cx, i18n::t("Back", "上一步"));
        self.view.button(cx, ids!(wz_test)).set_text(cx, i18n::t("Test connection", "测试连接"));
        self.view.button(cx, ids!(wz_next)).set_text(cx, i18n::t("Next", "下一步"));
        self.view.button(cx, ids!(wz_finish)).set_text(cx, &i18n::pick(format!("Add {n}"), format!("添加 {n} 个")));
        let status = match (&w.status, w.busy) {
            (_, true) => i18n::t("Working…", "处理中…").to_string(),
            (Some(Ok(s)), _) => format!("✓ {s}"),
            (Some(Err(e)), _) => format!("✗ {e}"),
            (None, _) => String::new(),
        };
        self.view.label(cx, ids!(wz_status)).set_text(cx, &status);
    }

    /// The wizard's list.
    pub(crate) fn draw_wizard(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let Some(w) = &self.wizard else {
            list.set_item_range(cx, 0, 0);
            while list.next_visible_item(cx).is_some() {}
            return;
        };
        let rows = w.rows();
        let mine: Vec<String> = own_providers::set().iter().map(|p| p.family.clone()).collect();
        list.set_item_range(cx, 0, rows.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(row) = rows.get(index) else { continue };
            match row {
                Row::Header(text) => {
                    let item = list.item(cx, index, id!(Header));
                    item.label(cx, ids!(label)).set_text(cx, text);
                    item.draw_all_unscoped(cx);
                }
                Row::Family(f) => {
                    let item = list.item(cx, index, id!(Family));
                    let key = if f.family.key_required { i18n::t("API key", "API key") } else { i18n::t("no key needed", "无需 key") };
                    let probe = Provider::new(f.id(), None);
                    let n = f.models.len();
                    let meta = i18n::pick(format!("{key} · {n} model{} · {}", if n == 1 { "" } else { "s" }, own_providers::agents_for(&probe).join(" · ")),
                        format!("{key} · {n} 个模型 · {}", own_providers::agents_for(&probe).join(" · ")));
                    item.label(cx, ids!(name)).set_text(cx, f.label());
                    item.label(cx, ids!(meta)).set_text(cx, &meta);
                    item.label(cx, ids!(tag)).set_text(cx, if mine.iter().any(|m| m == f.id()) { i18n::t("added", "已添加") } else { "" });
                    item.draw_all_unscoped(cx);
                }
                Row::Route(i, r) => {
                    let item = list.item(cx, index, id!(Model));
                    let probe = w.family.and_then(|f| f.models.iter().find(|m| m.routes().iter().any(|x| x.id == r.id)).map(|m| own_providers::provider(f, m, r)));
                    item.label(cx, ids!(mark)).set_text(cx, if *i == w.route { "✓" } else { "" });
                    item.label(cx, ids!(name)).set_text(cx, &r.label);
                    item.label(cx, ids!(meta)).set_text(cx, &probe.as_ref().map(|p| format!("{} · {}", base_url_of(p), agents_text(p))).unwrap_or_default());
                    item.label(cx, ids!(tag)).set_text(cx, if r.is_official() { i18n::t("official", "官方") } else { "" });
                    item.draw_all_unscoped(cx);
                }
                Row::Model(m) => {
                    let item = list.item(cx, index, id!(Model));
                    let meta = [m.tier.as_str().to_string(), m.context_text(), m.price_text()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
                    item.label(cx, ids!(mark)).set_text(cx, if w.checked.contains(&m.id) { "✓" } else { "" });
                    item.label(cx, ids!(name)).set_text(cx, &m.label);
                    item.label(cx, ids!(meta)).set_text(cx, &format!("{} · {meta}", m.id));
                    item.label(cx, ids!(tag)).set_text(cx, if m.default { i18n::t("recommended", "推荐") } else { "" });
                    item.draw_all_unscoped(cx);
                }
            }
        }
    }

    /// The providers list: its rows (OctoBuddy's own can be changed).
    pub(crate) fn draw_providers(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let editable = !system::hosted() && own_providers::is_own(&self.providers.profile_path);
        if self.providers.rows.is_empty() {
            let why = match &self.providers.error {
                Some(err) => i18n::pick(format!("Could not read the AI providers profile: {err}"), format!("无法读取 AI Providers 配置：{err}")),
                None if editable => i18n::pick(format!("No provider yet. Press “+ Add a provider”{}. Until then an agent runs on its own sign-in, and the inner loop on the octos profile “{}”.",
                    if self.providers.octosense.is_empty() { "" } else { ", or import OctoSense's" }, providers::fallback_profile()),
                    format!("还没有 provider。点「+ 添加供应商」{}。在此之前 agent 用它自己的登录，inner 使用 octos 配置“{}”。",
                    if self.providers.octosense.is_empty() { "" } else { "，或导入 OctoSense 的" }, providers::fallback_profile())),
                None => i18n::pick(format!("No provider is enabled in OctoSense yet. Open Start → Settings → AI providers in OctoSense to add one. Until then the inner loop uses the octos profile “{}”.", providers::fallback_profile()),
                    format!("OctoSense 里还没有启用任何 provider。请在 OctoSense 的「开始 → 设置 → AI providers」中添加。在此之前 inner 使用 octos 配置“{}”。", providers::fallback_profile())),
            };
            list.set_item_range(cx, 0, 1);
            while let Some(index) = list.next_visible_item(cx) {
                if index == 0 {
                    let item = list.item(cx, index, id!(Empty));
                    item.label(cx, ids!(empty_text)).set_text(cx, &why);
                    item.draw_all_unscoped(cx);
                }
            }
            return;
        }
        list.set_item_range(cx, 0, self.providers.rows.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(row) = self.providers.rows.get(index) else { continue };
            let template = match (editable, row.role == "primary") {
                (false, _) => id!(Provider),
                (true, true) => id!(OwnPrimary),
                (true, false) => id!(OwnFallback),
            };
            let item = list.item(cx, index, template);
            item.label(cx, ids!(name)).set_text(cx, &row.label);
            item.label(cx, ids!(detail)).set_text(cx, &format!("route: {} · {}", row.route, row.key.label()));
            item.label(cx, ids!(agents)).set_text(cx, &i18n::pick(format!("Runs: {}", row.agents.join(" · ")), format!("可运行：{}", row.agents.join(" · "))));
            item.label(cx, ids!(role)).set_text(cx, &row.role);
            if editable {
                let confirming = self.confirm_remove.as_deref() == Some(row.label.as_str());
                item.button(cx, ids!(row_remove)).set_text(cx, if confirming { i18n::t("Click again to remove", "再点一次删除") } else { i18n::t("Remove", "删除") });
                item.button(cx, ids!(row_primary)).set_text(cx, i18n::t("Make primary", "设为主模型"));
                item.button(cx, ids!(row_test)).set_text(cx, i18n::t("Test", "测试"));
            }
            item.draw_all_unscoped(cx);
        }
    }

    fn reread_providers(&mut self, cx: &mut Cx) {
        self.providers = providers::read();
        self.relayout(cx);
    }

    /// Runs `job` off the UI thread; its result comes back as `ProvidersDone`.
    fn providers_job(&mut self, job: ProvidersJob, work: impl FnOnce() -> Result<String, String> + Send + 'static) {
        let inbox = self.rt.inbox.clone();
        std::thread::spawn(move || {
            let result = work();
            events::post(&inbox, LoopEvent::ProvidersDone { job, result });
        });
    }

    pub(crate) fn providers_page_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.page != crate::Page::Settings || self.settings_tab != SettingsTab::Providers {
            return;
        }
        for (id, source) in [(ids!(src_own), Source::Own), (ids!(src_os), Source::OctoSense)] {
            if self.view.button(cx, id).clicked(actions) {
                own_providers::set_source(source);
                self.own_note = None;
                self.reread_providers(cx);
            }
        }
        if self.view.button(cx, ids!(import_providers)).clicked(actions) {
            self.own_note = Some(Ok(i18n::t("Importing…", "正在导入…").to_string()));
            self.providers_job(ProvidersJob::Import, || {
                let n = own_providers::import_octosense()?;
                own_providers::set_source(Source::Own);
                Ok(i18n::pick(format!("Imported {n} from OctoSense's AI providers."), format!("已从 OctoSense 的 AI Providers 导入 {n} 个。")))
            });
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(add_provider)).clicked(actions) {
            self.wizard = Some(Wizard { step: 1, ..Default::default() });
            self.own_note = None;
            self.relayout(cx);
        }
        self.provider_row_actions(cx, actions);
        self.wizard_actions(cx, actions);
    }

    fn provider_row_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let list = self.view.portal_list(cx, ids!(provider_list));
        let mut hit = None;
        for (index, item) in list.items_with_actions(actions) {
            for (id, what) in [(ids!(row_primary), 0), (ids!(row_test), 1), (ids!(row_remove), 2)] {
                if item.button(cx, id).clicked(actions) {
                    hit = Some((index, what));
                }
            }
        }
        let Some((index, what)) = hit else { return };
        let Some(label) = self.providers.rows.get(index).map(|r| r.label.clone()) else { return };
        if what != 2 {
            self.confirm_remove = None;
        }
        match what {
            0 => {
                self.own_note = Some(own_providers::make_primary(&label)
                    .map(|_| i18n::pick(format!("{label} is the primary now."), format!("{label} 已设为主模型。"))));
                self.reread_providers(cx);
            }
            1 => {
                let Some(p) = own_providers::set().iter().find(|p| p.label() == label).cloned() else { return };
                self.own_note = Some(Ok(i18n::pick(format!("Testing {label}…"), format!("正在测试 {label}…"))));
                self.providers_job(ProvidersJob::TestRow(label), move || own_providers::test(&p, None));
                self.relayout(cx);
            }
            _ if self.confirm_remove.as_deref() != Some(label.as_str()) => {
                self.confirm_remove = Some(label);
                self.relayout(cx);
            }
            _ => {
                self.confirm_remove = None;
                let gone = label.clone();
                self.providers_job(ProvidersJob::Remove(label), move || own_providers::remove(&gone).map(|_| String::new()));
            }
        }
    }

    fn wizard_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.wizard.is_none() {
            return;
        }
        if self.view.button(cx, ids!(wz_cancel)).clicked(actions) {
            self.close_wizard(cx);
            return;
        }
        let list = self.view.portal_list(cx, ids!(wz_list));
        let tapped_row = list.items_with_actions(actions).into_iter()
            .find(|(_, item)| tapped(&item.view(cx, ids!(card)), actions)).map(|(index, _)| index);
        let Some(w) = self.wizard.as_mut() else { return };
        if w.busy {
            return;
        }
        if let Some(index) = tapped_row {
            match w.rows().into_iter().nth(index) {
                Some(Row::Family(f)) => {
                    *w = Wizard { step: 2, family: Some(f), ..Default::default() };
                    list.set_first_id_and_scroll(0, 0.0);
                }
                Some(Row::Route(i, _)) => {
                    w.route = i;
                    w.status = None;
                }
                // Checked, or not any more.
                Some(Row::Model(m)) => w.checked = &w.checked ^ &BTreeSet::from([m.id.clone()]),
                _ => {}
            }
            self.relayout(cx);
            return;
        }
        if self.view.button(cx, ids!(wz_back)).clicked(actions) {
            let w = self.wizard.as_mut().unwrap();
            w.step -= 1;
            w.status = None;
            if w.step == 1 {
                self.view.text_input(cx, ids!(wz_key)).set_text(cx, "");
            }
            self.relayout(cx);
            return;
        }
        if self.view.button(cx, ids!(wz_key_link)).clicked(actions) {
            if let Some(page) = self.wizard.as_ref().and_then(|w| w.family).and_then(|f| own_providers::key_page(f.id())) {
                open_url(page);
            }
        }
        let (key, base) = (self.view.text_input(cx, ids!(wz_key)).text(), self.view.text_input(cx, ids!(wz_base)).text());
        let key = Some(key.trim().to_string()).filter(|k| !k.is_empty());
        if self.view.button(cx, ids!(wz_test)).clicked(actions) {
            let w = self.wizard.as_mut().unwrap();
            match w.test_model().and_then(|m| w.provider(m, &base)) {
                Some(p) => {
                    w.busy = true;
                    w.status = None;
                    self.providers_job(ProvidersJob::Test, move || own_providers::test(&p, key.as_deref()));
                }
                None => w.status = Some(Err(i18n::t("No model is served there.", "这个接入点没有可用的模型。").into())),
            }
            self.relayout(cx);
            return;
        }
        if self.view.button(cx, ids!(wz_next)).clicked(actions) {
            let w = self.wizard.as_mut().unwrap();
            let probe = w.test_model().and_then(|m| w.provider(m, &base));
            let required = w.family.is_some_and(|f| f.family.key_required);
            w.status = match &probe {
                None if w.needs_base() && base.trim().is_empty() => Some(Err(i18n::t("Enter its base URL.", "请填写接入地址。").into())),
                None => Some(Err(i18n::t("No model is served there.", "这个接入点没有可用的模型。").into())),
                Some(p) if required && key.is_none() && !own_providers::has_key(&p.key_env) => Some(Err(i18n::t("Enter its API key.", "请填写 API key。").into())),
                Some(_) => None,
            };
            if w.status.is_none() {
                w.step = 3;
                w.checked = w.test_model().map(|m| m.id.clone()).into_iter().collect();
            }
            self.relayout(cx);
            return;
        }
        if self.view.button(cx, ids!(wz_finish)).clicked(actions) {
            let w = self.wizard.as_mut().unwrap();
            let chosen: Vec<Provider> = w.models().into_iter().filter(|m| w.checked.contains(&m.id)).filter_map(|m| w.provider(m, &base)).collect();
            if chosen.is_empty() {
                w.status = Some(Err(i18n::t("Pick at least one model.", "请至少选一个模型。").into()));
            } else {
                w.busy = true;
                w.status = None;
                self.providers_job(ProvidersJob::Add, move || {
                    let mut added = Vec::new();
                    for (i, p) in chosen.into_iter().enumerate() {
                        // The key once: the models of one endpoint share it.
                        added.push(own_providers::add(p, if i == 0 { key.as_deref() } else { None })?);
                    }
                    own_providers::set_source(Source::Own);
                    Ok(i18n::pick(format!("Added {}.", added.join(", ")), format!("已添加 {}。", added.join("、"))))
                });
            }
            self.relayout(cx);
        }
    }

    fn close_wizard(&mut self, cx: &mut Cx) {
        self.wizard = None;
        self.view.text_input(cx, ids!(wz_key)).set_text(cx, "");
        self.view.text_input(cx, ids!(wz_base)).set_text(cx, "");
        self.relayout(cx);
    }

    /// A job of the page is done.
    pub(crate) fn providers_done(&mut self, job: ProvidersJob, result: Result<String, String>) {
        match job {
            ProvidersJob::Test => {
                if let Some(w) = self.wizard.as_mut() {
                    w.busy = false;
                    w.status = Some(result);
                }
            }
            ProvidersJob::Add => match result {
                Ok(said) => {
                    self.wizard = None;
                    self.wizard_clear = true;
                    self.own_note = Some(Ok(said));
                }
                Err(err) => {
                    if let Some(w) = self.wizard.as_mut() {
                        w.busy = false;
                        w.status = Some(Err(err));
                    }
                }
            },
            ProvidersJob::Import => self.own_note = Some(result),
            ProvidersJob::Remove(label) => self.own_note = Some(result.map(|_| i18n::pick(format!("Removed {label}."), format!("已删除 {label}。")))),
            ProvidersJob::TestRow(label) => self.own_note = Some(result.map(|s| format!("{label}: {s}")).map_err(|e| format!("{label}: {e}"))),
        }
        self.providers = providers::read();
    }

    pub(crate) fn logins_probed(&mut self, logins: Vec<(String, String)>) {
        self.logins = Some(logins);
        self.logins_probing = false;
    }
}
