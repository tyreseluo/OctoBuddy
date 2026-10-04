//! Submitting an app OctoBuddy built to OctoSense's App Hub, from the host
//! (App Hub's PUBLISHING, "The full sequence" and "Submitting"): the person
//! fills in what the store shows and who publishes it; OctoBuddy takes the
//! app's screenshots from a real run, runs the gate (`hub check`) and the
//! review scan (`hub scan`), signs it when asked (a publisher key made once
//! and kept under OctoBuddy's data, never in the project), commits and tags
//! it in the app's public repository, and opens the `Submit <id> <version>`
//! issue App Hub's maintainers accept. Nothing is submitted until the person
//! has seen every step (`plan`) and pressed Submit.
//!
//! What the form holds lives where App Hub reads it (`bundle/manifest.json`,
//! `bundle/listing.json`); OctoBuddy's own choices for the app (its
//! publisher id, whether to sign, the last submission) in
//! `<project>/.octobuddy/app-hub.json`. Inside OctoSense an app goes to
//! the local App Hub instead (`app_publish.rs`).
use super::octosense_app::{self, Tools};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where submissions go: App Hub's repository (its issues).
pub fn hub_repo() -> String {
    std::env::var("OCTOBUDDY_APP_HUB_REPO").ok().filter(|r| !r.trim().is_empty()).unwrap_or_else(|| "OctoSense-org/OctoSense-App-Hub".into())
}

/// The store's categories (PUBLISHING, "The listing").
pub const CATEGORIES: [&str; 15] = ["productivity", "utilities", "photo-video", "news", "weather", "travel", "finance", "health",
    "education", "entertainment", "games", "social", "shopping", "lifestyle", "developer"];
/// The platforms a listing may claim: only those it was run on.
pub const PLATFORMS: [&str; 7] = ["macos", "ios", "android", "windows", "linux", "openharmony", "web"];
pub const AGE_RATINGS: [&str; 4] = ["all", "12+", "16+", "18+"];

/// What the form shows and the person fills in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    pub id: String,
    pub name: String,
    pub version: String,
    pub subtitle: String,
    pub description: String,
    pub category: String,
    pub keywords: Vec<String>,
    pub platforms: Vec<String>,
    pub age_rating: String,
    pub license: String,
    pub release_notes: String,
    pub publisher_name: String,
    pub support: String,
    pub privacy_url: String,
    /// Its publisher id (`hub sign-manifest --key-id`), for a signed submission.
    pub publisher_id: String,
    /// Sign it (optional for a first submission; required for updates once a key is on record).
    pub sign: bool,
    pub screenshots: Vec<String>,
}

/// The last submission, as OctoBuddy keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Submission {
    pub version: String,
    pub tag: String,
    pub commit: String,
    pub repo: String,
    pub issue: String,
}

fn bundle(project: &Path) -> PathBuf {
    project.join("bundle")
}

fn read_json(path: &Path) -> Value {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
}

fn own_path(project: &Path) -> PathBuf {
    project.join(".octobuddy/app-hub.json")
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn list(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// The form, as the project's files have it now.
pub fn read(project: &Path) -> Listing {
    let m = read_json(&bundle(project).join("manifest.json"));
    let l = read_json(&bundle(project).join("listing.json"));
    let own = read_json(&own_path(project));
    Listing {
        id: s(&m["id"]),
        name: s(&m["name"]),
        version: s(&m["version"]),
        subtitle: s(&l["subtitle"]),
        description: s(&l["description"]),
        category: s(&l["category"]),
        keywords: list(&l["keywords"]),
        platforms: list(&l["platforms"]),
        age_rating: s(&l["age_rating"]),
        license: s(&l["license"]),
        release_notes: s(&l["release_notes"]),
        publisher_name: s(&l["publisher"]["name"]),
        support: s(&l["publisher"]["support"]),
        privacy_url: s(&l["publisher"]["privacy_policy_url"]),
        publisher_id: s(&own["publisher_id"]),
        sign: own["sign"].as_bool().unwrap_or(false),
        screenshots: list(&l["screenshots"]),
    }
}

/// The last submission made from here, if any.
pub fn last_submission(project: &Path) -> Option<Submission> {
    let v = &read_json(&own_path(project))["submitted"];
    (!v.is_null()).then(|| Submission { version: s(&v["version"]), tag: s(&v["tag"]), commit: s(&v["commit"]), repo: s(&v["repo"]), issue: s(&v["issue"]) })
}

/// A value the template leaves for a person to replace.
fn placeholder(v: &str) -> bool {
    let v = v.to_ascii_lowercase();
    v.is_empty() || v.contains("example.com") || v.contains("replace with") || v.contains("replace this")
}

/// What stops the form from being submitted, one line each (none: it can be).
pub fn problems(l: &Listing) -> Vec<String> {
    let mut out = Vec::new();
    let mut need = |ok: bool, en: &'static str, zh: &'static str| if !ok { out.push(crate::i18n::t(en, zh).to_string()) };
    need(!l.name.trim().is_empty(), "The app needs a name.", "应用需要名称。");
    let semver = l.version.split('.').count() == 3 && l.version.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    need(semver, "The version is three numbers (1.0.0).", "版本号是三段数字（如 1.0.0）。");
    need(!l.subtitle.trim().is_empty() && l.subtitle.chars().count() <= 80, "The subtitle: one line, at most 80 characters.", "副标题：一行，最多 80 个字符。");
    need(!l.description.trim().is_empty() && l.description.chars().count() <= 4000, "The description: what the app does, at most 4000 characters.", "描述：写清应用做什么，最多 4000 个字符。");
    need(CATEGORIES.contains(&l.category.as_str()), "Pick a category.", "请选择分类。");
    need(l.keywords.len() <= 10, "At most 10 keywords.", "关键词最多 10 个。");
    need(!l.platforms.is_empty() && l.platforms.iter().all(|p| PLATFORMS.contains(&p.as_str())), "Tick the platforms it was run on.", "请勾选实际运行过的平台。");
    need(AGE_RATINGS.contains(&l.age_rating.as_str()), "Pick an age rating.", "请选择年龄分级。");
    need(!placeholder(&l.publisher_name), "The publisher's name (yours, as the store shows it).", "请填写发布者名称（商店里显示的名字）。");
    need(!placeholder(&l.support), "A support contact: an email or a page.", "请填写支持联系方式：邮箱或网页。");
    need(l.privacy_url.starts_with("https://") && !placeholder(&l.privacy_url), "A privacy policy page (https) that says what the app does with data.", "请填写隐私政策页面（https），写明应用如何处理数据。");
    let id_ok = !l.publisher_id.is_empty() && l.publisher_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    need(!l.sign || id_ok, "To sign, a publisher id: lowercase letters, digits and -.", "要签名，请填写发布者 ID：小写字母、数字和 -。");
    out
}

/// Writes the form into the project's files: the manifest's name and
/// version, the listing, OctoBuddy's own choices. The screenshots the
/// listing names are what `prepare` takes.
pub fn write(project: &Path, l: &Listing) -> Result<(), String> {
    let mpath = bundle(project).join("manifest.json");
    let mut m = read_json(&mpath);
    if !m.is_object() {
        return Err("bundle/manifest.json is missing or not JSON".into());
    }
    m["name"] = json!(l.name.trim());
    m["version"] = json!(l.version.trim());
    let lpath = bundle(project).join("listing.json");
    let mut lv = read_json(&lpath);
    if !lv.is_object() {
        lv = json!({"schema": 1});
    }
    lv["subtitle"] = json!(l.subtitle.trim());
    lv["description"] = json!(l.description.trim());
    lv["category"] = json!(l.category);
    lv["keywords"] = json!(l.keywords);
    lv["platforms"] = json!(l.platforms);
    lv["age_rating"] = json!(l.age_rating);
    lv["license"] = json!(l.license.trim());
    lv["release_notes"] = json!(l.release_notes.trim());
    lv["publisher"] = json!({"name": l.publisher_name.trim(), "support": l.support.trim(), "privacy_policy_url": l.privacy_url.trim()});
    if l.screenshots.is_empty() {
        lv["screenshots"] = json!(["screenshots/01-main.png"]);
    }
    let pretty = |v: &Value| serde_json::to_string_pretty(v).map(|t| t + "\n").map_err(|e| e.to_string());
    std::fs::write(&mpath, pretty(&m)?).map_err(|e| e.to_string())?;
    std::fs::write(&lpath, pretty(&lv)?).map_err(|e| e.to_string())?;
    let opath = own_path(project);
    let mut own = read_json(&opath);
    if !own.is_object() {
        own = json!({});
    }
    own["publisher_id"] = json!(l.publisher_id.trim());
    own["sign"] = json!(l.sign);
    std::fs::create_dir_all(opath.parent().unwrap_or(project)).map_err(|e| e.to_string())?;
    std::fs::write(&opath, pretty(&own)?).map_err(|e| e.to_string())
}

/// The publisher key for `id`, kept under OctoBuddy's data (made on the first signed submission).
pub fn key_path(id: &str) -> PathBuf {
    crate::model::data_dir().join("publisher").join(format!("{id}.key"))
}

/// What App Hub's `hub` command is, and the script that does the steps.
fn hub_and_script(tools: &Tools) -> Result<(PathBuf, PathBuf), String> {
    let hub = tools.app_hub.clone().map(|d| d.join("target/release/hub")).filter(|h| h.is_file())
        .ok_or("App Hub's `hub` command was not found: build OctoSense-App-Hub (cargo build --release) beside OctoScript App Design Flow, or set OCTOSENSE_APP_HUB")?;
    let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', r"'\''"));
    let env = tools.app_hub.as_deref().map(|h| format!("OCTOSENSE_APP_HUB={}; export OCTOSENSE_APP_HUB\n", quote(h))).unwrap_or_default();
    let script = SCRIPT.replace("{env}", &env).replace("{octo}", &quote(&tools.flow.join("tools/octo")));
    let path = octosense_app::bin_dir().join("octobuddy-app-hub");
    std::fs::create_dir_all(octosense_app::bin_dir()).map_err(|e| e.to_string())?;
    std::fs::write(&path, script).map_err(|e| e.to_string())?;
    Ok((hub, path))
}

fn run_script(args: &[&str], envs: &[(&str, String)]) -> Result<String, String> {
    let tools = octosense_app::tools()?;
    let (hub, script) = hub_and_script(&tools)?;
    let mut cmd = Command::new("sh");
    cmd.arg(&script).args(args).env("HUB", &hub).env("PATH", crate::workspace::search_path());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| format!("could not run {}: {e}", script.display()))?;
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    if out.status.success() { Ok(log) } else { Err(log) }
}

/// Gets it ready to submit: its main.splash from its parts, its screenshots
/// from a real headless run, its digest, the gate and the review packet
/// (`build/review.json`, whose questions the answers reply to). Blocks.
pub fn prepare(project: &Path) -> Result<String, String> {
    octosense_app::assemble(&project.to_string_lossy())?;
    run_script(&["prepare", &project.to_string_lossy()], &[])
}

/// The review scan's questions (from `prepare`'s packet).
pub fn questions(project: &Path) -> Vec<String> {
    list(&read_json(&project.join("build/review.json"))["questions"])
}

/// The answers to them, as the person (or the outer loop's draft) wrote them.
pub fn answers(project: &Path) -> String {
    std::fs::read_to_string(project.join("build/REVIEW-ANSWERS.md")).unwrap_or_default()
}

pub fn save_answers(project: &Path, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(project.join("build")).map_err(|e| e.to_string())?;
    std::fs::write(project.join("build/REVIEW-ANSWERS.md"), text).map_err(|e| e.to_string())
}

/// What the person's Submit does, step by step, and the issue it opens
/// (none of it done: the dialog shows this first). Blocks.
pub fn plan(project: &Path) -> Result<String, String> {
    submit_or_plan(project, true)
}

/// Submits it (after `plan` was shown and the person pressed Submit):
/// signs it if asked, commits and tags it, pushes, opens the issue; the
/// issue's link on the last line. Blocks.
pub fn submit(project: &Path) -> Result<String, String> {
    let log = submit_or_plan(project, false)?;
    // The script's last lines: the commit, the repository, the issue.
    let field = |k: &str| log.lines().rev().find_map(|l| l.strip_prefix(k)).map(|v| v.trim().to_string()).unwrap_or_default();
    let l = read(project);
    let sub = Submission { version: l.version.clone(), tag: format!("v{}", l.version), commit: field("commit: "), repo: field("repo: "), issue: field("issue: ") };
    let opath = own_path(project);
    let mut own = read_json(&opath);
    if !own.is_object() {
        own = json!({});
    }
    own["submitted"] = json!({"version": sub.version, "tag": sub.tag, "commit": sub.commit, "repo": sub.repo, "issue": sub.issue});
    let _ = std::fs::write(&opath, serde_json::to_string_pretty(&own).unwrap_or_default() + "\n");
    Ok(log)
}

fn submit_or_plan(project: &Path, dry: bool) -> Result<String, String> {
    let l = read(project);
    let problems = problems(&l);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    let key = if l.sign { key_path(&l.publisher_id).to_string_lossy().into_owned() } else { String::new() };
    let answered = project.join("build/REVIEW-ANSWERS.md");
    if !answered.is_file() || answers(project).trim().is_empty() {
        return Err(crate::i18n::t("Answer the review questions first (Check, then the answers).", "请先回答审查问题（先「检查」，再填回答）。").into());
    }
    run_script(&[if dry { "plan" } else { "submit" }, &project.to_string_lossy()], &[
        ("APP_ID", l.id.clone()),
        ("APP_NAME", l.name.clone()),
        ("APP_VERSION", l.version.clone()),
        ("PUBLISHER_ID", if l.sign { l.publisher_id.clone() } else { String::new() }),
        ("PUBLISHER_KEY", key),
        ("HUB_REPO", hub_repo()),
    ])
}

/// The steps, in a script so each is the command App Hub's PUBLISHING names.
const SCRIPT: &str = r###"#!/bin/sh
# OctoBuddy's App Hub submission (OctoBuddy rewrites this file).
#   prepare <project>: screenshots, stamp, gate, review packet
#   plan <project>:    what submit does, nothing done
#   submit <project>:  sign (if asked), gate, commit, tag, push, the issue
set -u
step="$1"; project="$2"
{env}octo={octo}
b="$project/bundle"
build="$project/build"
mkdir -p "$build"
# An earlier signature goes first: the bytes change, and the preview and
# the headless run take an unsigned bundle (it is signed again on submit).
unsign() {
  python3 - "$b/manifest.json" <<'PY'
import json, sys
p = sys.argv[1]
m = json.load(open(p))
if isinstance(m.get("integrity"), dict) and "signature" in m["integrity"]:
    del m["integrity"]["signature"]
    open(p, "w").write(json.dumps(m, indent=2, ensure_ascii=False) + "\n")
PY
}
case "$step" in
prepare)
  unsign
  # The screenshots its listing names, from a real headless run.
  shots=$(python3 -c 'import json,sys; print("\n".join(json.load(open(sys.argv[1])).get("screenshots") or ["screenshots/01-main.png"]))' "$b/listing.json")
  run="$build/run"
  rm -rf "$run"; cp -R "$b" "$run"
  port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
  if python3 "$octo" run "$run" --port "$port" --hidden --detach --timeout 20 > "$build/run.txt" 2>&1; then
    for shot in $shots; do
      mkdir -p "$b/$(dirname "$shot")"
      python3 "$octo" shot "$port" "$b/$shot" > /dev/null 2>&1 && echo "screenshot $shot taken"
    done
    curl -s -m 5 "http://127.0.0.1:$port/quit" > /dev/null 2>&1
  else
    echo "the app did not start headless, so its screenshots could not be taken:"
    tail -5 "$build/run.txt"
    rm -rf "$run"
    exit 1
  fi
  rm -rf "$run"
  echo "== App Hub's gate"
  python3 "$octo" check "$b" > "$build/check.txt" 2>&1; status=$?
  grep -vE '^octo: (hub stamp|/)' "$build/check.txt"
  [ $status -eq 0 ] || exit 1
  echo "== review packet"
  "$HUB" scan "$b" --packet "$build/review.json" || exit 1
  ;;
plan|submit)
  dry=0; [ "$step" = plan ] && dry=1
  say() { if [ $dry -eq 1 ]; then echo "- $*"; fi; }
  tag="v$APP_VERSION"
  cd "$project" || exit 2
  git rev-parse --git-dir > /dev/null 2>&1 || { echo "the project is not a git repository"; exit 1; }
  if git rev-parse -q --verify "refs/tags/$tag" > /dev/null; then echo "the tag $tag exists already: raise the version"; exit 1; fi
  remote=$(git remote get-url origin 2>/dev/null || echo "")
  gh auth status > /dev/null 2>&1 || { echo "GitHub's gh is not signed in: run gh auth login"; exit 1; }
  if [ -n "$PUBLISHER_ID" ]; then
    if [ -f "$PUBLISHER_KEY" ]; then say "sign it with your publisher key \"$PUBLISHER_ID\" ($PUBLISHER_KEY)"
    else say "make your publisher key \"$PUBLISHER_ID\" once ($PUBLISHER_KEY, kept on this Mac, never in the project) and sign it"; fi
  else
    say "submit it unsigned (allowed for a first submission)"
  fi
  if [ -z "$remote" ]; then
    owner=$(gh api user --jq .login 2>/dev/null)
    say "create the public repository github.com/$owner/$APP_ID and push this project to it"
  else
    say "push to $remote"
  fi
  say "commit bundle/ (\"chore(release): $APP_NAME ${APP_VERSION}，提交 App Hub\") and tag it $tag"
  say "open the issue \"Submit $APP_ID $APP_VERSION\" in github.com/$HUB_REPO"
  if [ $dry -eq 1 ]; then
    echo
    echo "The issue will say:"
  fi
  if [ $dry -eq 0 ]; then
    unsign
    "$HUB" stamp "$b" > /dev/null || exit 1
    if [ -n "$PUBLISHER_ID" ]; then
      if [ ! -f "$PUBLISHER_KEY" ]; then
        mkdir -p "$(dirname "$PUBLISHER_KEY")"; chmod 700 "$(dirname "$PUBLISHER_KEY")"
        (umask 077; "$HUB" keygen "$PUBLISHER_KEY" > /dev/null) || exit 1
      fi
      pub=$("$HUB" pubkey "$PUBLISHER_KEY") || exit 1
      "$HUB" sign-manifest "$b" --key "$PUBLISHER_KEY" --key-id "$PUBLISHER_ID" || exit 1
      "$HUB" check "$b" --publisher-key "$PUBLISHER_ID=$pub" > "$build/check-final.txt" 2>&1 || { cat "$build/check-final.txt"; exit 1; }
    else
      "$HUB" check "$b" --allow-unsigned > "$build/check-final.txt" 2>&1 || { cat "$build/check-final.txt"; exit 1; }
    fi
    git add bundle && { git diff --cached --quiet || git commit -q -m "chore(release): $APP_NAME ${APP_VERSION}，提交 App Hub"; } || exit 1
    git tag -a "$tag" -m "$APP_NAME $APP_VERSION" || exit 1
    if [ -z "$remote" ]; then
      gh repo create "$APP_ID" --public --source . --remote origin --push > /dev/null || exit 1
      remote=$(git remote get-url origin)
    else
      git push -q origin HEAD || exit 1
    fi
    git push -q origin "$tag" || exit 1
  else
    check_out=$(cat "$build/check.txt" 2>/dev/null | grep -vE '^octo: (hub stamp|/)')
  fi
  commit=$(git rev-parse HEAD)
  url=$(echo "$remote" | sed -E 's#^git@github.com:#https://github.com/#; s#\.git$##')
  [ -n "$url" ] || url="https://github.com/$(gh api user --jq .login 2>/dev/null)/$APP_ID"
  if [ -n "$PUBLISHER_ID" ]; then
    key_line="$PUBLISHER_ID, public key $( [ -f "$PUBLISHER_KEY" ] && "$HUB" pubkey "$PUBLISHER_KEY" || echo '(made on submit)')"
  else
    key_line="unsigned"
  fi
  body="$build/submit-issue.md"
  {
    echo "Submitting **$APP_NAME** ($APP_ID) $APP_VERSION."
    echo
    echo "- Repository: $url"
    echo "- Tag: $tag"
    echo "- Commit: $( [ $dry -eq 1 ] && echo '(the commit this makes)' || echo "$commit")"
    echo "- Bundle: bundle/"
    echo "- Publisher: $key_line"
    echo
    echo "## hub check"
    echo
    echo '```'
    if [ $dry -eq 0 ]; then cat "$build/check-final.txt"; else echo "$check_out"; fi
    echo '```'
    echo
    echo "## Answers to the hub scan questions"
    echo
    cat "$build/REVIEW-ANSWERS.md"
    echo
    echo "(Prepared and submitted with OctoBuddy.)"
  } > "$body"
  if [ $dry -eq 1 ]; then
    cat "$body"
    exit 0
  fi
  issue=$(gh issue create -R "$HUB_REPO" --title "Submit $APP_ID $APP_VERSION" --body-file "$body") || exit 1
  echo "commit: $commit"
  echo "repo: $url"
  echo "issue: $issue"
  ;;
*) echo "unknown step $step"; exit 2 ;;
esac
"###;

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("octobuddy-app-hub-{}-{}", std::process::id(), crate::model::new_id("t")));
        std::fs::create_dir_all(dir.join("bundle")).unwrap();
        std::fs::write(dir.join("bundle/manifest.json"), r#"{"schema": 1, "id": "pocket-ledger", "name": "小账本", "version": "0.1.0", "capabilities": ["storage"]}"#).unwrap();
        std::fs::write(dir.join("bundle/listing.json"), r#"{"schema": 1, "subtitle": "s", "description": "d", "category": "finance", "keywords": [], "screenshots": ["screenshots/01-main.png"], "icon": "assets/icon.svg", "platforms": ["android"], "publisher": {"name": "Replace with your publisher name", "support": "https://example.com/support", "privacy_policy_url": "https://example.com/privacy"}, "release_notes": "", "age_rating": "all", "license": "Apache-2.0"}"#).unwrap();
        dir
    }

    /// The template's placeholders cannot be submitted; the person's values
    /// go where App Hub reads them, and come back the same.
    #[test]
    fn a_listing_is_checked_written_and_read_back() {
        let dir = project();
        let mut l = read(&dir);
        assert_eq!((l.id.as_str(), l.name.as_str(), l.category.as_str()), ("pocket-ledger", "小账本", "finance"));
        let p = problems(&l);
        assert!(p.len() >= 3, "the publisher's placeholders: {p:?}");
        l.version = "0.2.0".into();
        l.platforms = vec!["macos".into()];
        l.publisher_name = "Tyrese".into();
        l.support = "mailto:help@pocket-ledger.app".into();
        l.privacy_url = "https://pocket-ledger.app/privacy".into();
        l.keywords = vec!["记账".into()];
        assert!(problems(&l).is_empty(), "{:?}", problems(&l));
        l.sign = true;
        l.publisher_id = "Bad Id".into();
        assert_eq!(problems(&l).len(), 1, "a publisher id to sign with");
        l.publisher_id = "tyrese".into();
        write(&dir, &l).unwrap();
        let back = read(&dir);
        assert_eq!(back, Listing { screenshots: vec!["screenshots/01-main.png".into()], ..l });
        let listing = std::fs::read_to_string(dir.join("bundle/listing.json")).unwrap();
        assert!(listing.contains("\"icon\": \"assets/icon.svg\""), "what the form does not hold stays");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Live (`OCTOBUDDY_APP_HUB_PROJECT=<an app project> cargo test -- --ignored
    /// prepares_and_plans_a_copy --nocapture`): on a copy of the project,
    /// the form written, its screenshots taken and checked, the review
    /// packet made, then what Submit would do, nothing pushed or opened.
    #[test]
    #[ignore]
    fn prepares_and_plans_a_copy() {
        let from = PathBuf::from(std::env::var("OCTOBUDDY_APP_HUB_PROJECT").expect("OCTOBUDDY_APP_HUB_PROJECT"));
        let dir = std::env::temp_dir().join(format!("octobuddy-app-hub-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(Command::new("cp").arg("-R").arg(&from).arg(&dir).status().unwrap().success());
        // Its own copy: no remote, so the plan says a repository would be made.
        let _ = Command::new("git").args(["remote", "remove", "origin"]).current_dir(&dir).status();
        let mut l = read(&dir);
        l.version = "9.9.9".into();
        l.platforms = vec!["macos".into()];
        l.publisher_name = "OctoBuddy test".into();
        l.support = "mailto:help@pocket-ledger.app".into();
        l.privacy_url = "https://pocket-ledger.app/privacy".into();
        write(&dir, &l).unwrap();
        let prepared = prepare(&dir);
        eprintln!("== prepare\n{}", prepared.clone().unwrap_or_else(|e| e));
        assert!(prepared.is_ok());
        assert_eq!(questions(&dir).len(), 7, "the review packet's questions");
        assert!(dir.join("bundle/screenshots/01-main.png").is_file());
        save_answers(&dir, "## 1. …\n\nA test answer.\n").unwrap();
        let plan = plan(&dir);
        eprintln!("== plan\n{}", plan.clone().unwrap_or_else(|e| e));
        let plan = plan.unwrap();
        assert!(plan.contains("Submit pocket-ledger 9.9.9") && plan.contains("unsigned") && plan.contains("PASSED"), "{plan}");
        let tag = Command::new("git").args(["tag", "-l", "v9.9.9"]).current_dir(&dir).output().unwrap();
        assert!(tag.stdout.is_empty(), "the plan makes nothing");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_steps_script_is_valid_shell() {
        let path = std::env::temp_dir().join(format!("octobuddy-app-hub-script-{}.sh", std::process::id()));
        std::fs::write(&path, SCRIPT.replace("{env}", "").replace("{octo}", "/x/octo")).unwrap();
        let out = Command::new("sh").arg("-n").arg(&path).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let _ = std::fs::remove_file(path);
    }
}
