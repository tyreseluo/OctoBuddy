# Splash cookbook for OctoSense apps (`bundle/main.splash`)

Verified on 2026-10-02 against `card-host` built from OctoSense-App-Hub `053fb27` (`octosense-ws/OctoSense-App-Hub`, runtime: its sibling makepad checkout `a5a3cf5`), macOS, headless. The snippets below were put into one test app (`tools/octo new … --id cbtest`), run with `tools/octo run <bundle> --port 8196 --hidden --detach` and driven over the remote bridge. The test switched pages on a bottom tab bar and added rows from two TextInputs (empty, invalid and valid input, and Return). It also deleted every row and saw the empty state come back, deleted with two taps, quit and restarted (the data reloaded), started on nine kinds of broken `data.json`, and read the bar widths from `/snap`. Small probe apps produced the error messages quoted here. `tools/octo check` printed `cbtest 0.1.0 — PASSED` with only the unsigned warning. **✓** = exercised in that run. *cited* = copied from a working system app or SCRIPT-API but not exercised here.

Citations: `ai:19` = OctoSense `apps/ai-providers/bundle/main.splash` line 19 (also `photos`, `news`, `mail`, `camera`, `maps`, `youtube`). `tmpl` = OctoScript-App-Design-Flow `templates/script-app/bundle/main.splash`. `API`, `CAP` and `AGENTS` = that repo's `docs/SCRIPT-API.md`, `docs/CAPABILITIES.md` and `AGENTS.md`.

## 1. Program shape
**Rule:** put declarations (`let`, `fn`, style `let`s) first, then exactly one root widget. Use `ui` only inside fns. Start with `start_timeout(0.05, || boot())`.
```splash
let items = []                          // state: top-level let, lives for the session
fn boot(){ load(); show("list"); ui.rows.render() }
start_timeout(0.05, || boot())          // ui is injected only after the body ran
let ink = #x1c1c1e                      // style lets may come after the fns
SolidView{width: Fill height: Fill flow: Down draw_bg.color: #xffffff …children… }   // the one root
```
Cites: API:20-48, photos:443-451, news:381-390. ✓
- ✓ A top-level `ui.x.set_text("…")` does nothing and logs nothing.
- ✓ A body that does not parse draws nothing. `octo run` then prints `the app's widgets did not appear in /snap in time` and stops card-host. Read `<app>/.local-state/card-host.log`. In `[E] splash:<n>:<line>:<col> - …`, `<line>` is the main.splash line **+ 3** (*cited:* + 4 with `net`, API:334-338).

## 2. State and re-render
**Rule:** change the top-level `let`, then call `ui.<id>.render()` on every `on_render` container that shows it. `set_text` takes a string (`"" + n`). `set_visible` takes a bool.
```splash
rows := ScrollYView{width: Fill height: Fill flow: Down spacing: 8 on_render: || { … }}
fn add(){ items.push(x); save(); ui.rows.render() }      // shape only; full add() in §5
ui.form_note.set_text("" + n); ui.toast.set_visible(text != "")
```
Cites: API:63-73, tmpl:11-18. ✓
- ✓ An `on_render` container stays **empty until the first `.render()`**, so render each one in `boot()`.
- ✓ `ui.<id>` finds a `:=` widget anywhere in the tree, including through unnamed wrappers.
- ✓ **Template override paths** need every level named. `Row{top.sender.text: m.sender}` works because `top :=` is named (mail:151-153,190). Through an unnamed wrapper you get `field inner not found in type-check and has no default`, and that whole `on_render` output is discarded.

## 3. Pages and a bottom tab bar
**Rule:** make pages sibling views in a `flow: Overlay` box and toggle them with `set_visible`. Draw the tab bar in an `on_render` that picks between two pre-bound styles.
```splash
let Tab = ButtonFlat{width: Fill height: 48
    draw_bg +: {color: #xffffff color_hover: #xf2f2f7 color_down: #xe5e5ea border_size: 0.0}
    draw_text +: {color: secondary color_hover: secondary color_down: secondary text_style +: {font_size: 14}}}
let TabOn = Tab{draw_text +: {color: accent color_hover: accent color_down: accent}}
fn show(name){
    page = name
    ui.list_page.set_visible(name == "list")
    ui.stats_page.set_visible(name == "stats")
    ui.tabs.render()
    if name == "stats" { ui.stats.render() }
}
// inside the root SolidView{… flow: Down}:
    View{width: Fill height: Fill flow: Overlay
        list_page := View{width: Fill height: Fill flow: Down padding: 16 spacing: 10 …}
        stats_page := View{visible: false width: Fill height: Fill flow: Down padding: 16 spacing: 10 …}
    }
    tabs := View{width: Fill height: Fit flow: Right on_render: || {
        if page == "list" { TabOn{text: "List"} } else { Tab{text: "List" on_click: || show("list")} }
        if page == "stats" { TabOn{text: "Stats"} } else { Tab{text: "Stats" on_click: || show("stats")} }
    }}
```
Cites: mail:11-17,161 (panes, Overlay), photos:598-606 and news:398-405,446-453 (active tab). ✓
- **One varying value:** `pick_one` is **not a builtin**. Photos defines it (photos:235): `fn pick_one(c, a, b){ if c { return a } b }`. Then `draw_bg.color: pick_one(it.cents == top, danger, accent)` ✓. At this revision an inline `if` also worked as a value (✓ `text: if on {"a"} else {"b"}`, `width: if …`, `draw_bg.color: if …`, `else if` chains), but no system app does this, so prefer `pick_one` or a `let`.

## 4. Lists
**Rule:** write `if a.len() == 0 { Empty }`, then a separate `for i in a.len() { Row }`. Never use `else for`. Every render must yield at least one child.
```splash
rows := ScrollYView{width: Fill height: Fill flow: Down spacing: 8 on_render: || {
    if items.len() == 0 {
        Label{text: "No items yet. Add one above." draw_text.color: secondary}
    }
    for i in items.len() {
        let it = items[i]
        View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
            Label{width: Fill text: it.name draw_text.color: ink}
            Label{text: money(it.cents) draw_text.color: ink}
            if confirm_del == it.id { ActDanger{text: "Tap again to delete" on_click: || remove(i)} }
            else { ActDanger{text: "Delete" on_click: || remove(i)} }
        }
    }
}}
```
Cites: API:286-288, tmpl:41-51, ai:242-255,319-325; `Act`/`ActDanger` are the button styles of ai:179-183. ✓
- ✓ Each row's closure keeps its own `i`: tapping row 2 deleted row 2. With `else for`, the old rows stayed on screen after the list emptied, and the empty label never appeared. mail:188, news:463, photos:546 and youtube:235 use `else for`; don't copy them. An `on_render` that yields **zero** children also leaves the previous rows on screen (✓; ai:243: "Always one child").
- ✓ An error inside `on_render` (a missing field, say) logs `on_render closure failed; discarding its output: [Error:NotFound]`, and the list does not change.
- *cited:* for a tappable rich row, use `GestureView{width: Fill height: Fit on_tap: |x, y| open(m) …}` (mail:190, API:83); `ButtonFlat` holds no children (API:307). Arrays have `push pop clear len remove(i) retain(fn)`, and no sort, map, filter, join or slice (API:227). To filter, use `retain(|r| r.id != id)` (news:342) or loop into a new array (ai:54-59).

## 5. Forms
**Rule:** read with `("" + ui.<id>.text()).trim()` and parse with `.trim().to_f64()`. Validate with `if !(v > 0)`, because NaN fails every comparison. Clear the fields with `set_text("")`.
```splash
let Field = TextInput{width: Fill height: 40
    draw_bg +: {color: #xf2f2f7 color_hover: #xf2f2f7 color_focus: #xf2f2f7 color_empty: #xf2f2f7
        border_color: #x00000000 border_color_hover: #x00000000 border_color_focus: #x00000000 border_color_empty: #x00000000 border_radius: 10.0}
    draw_text +: {color: ink color_hover: ink color_focus: ink color_empty: secondary color_empty_hover: secondary}}
name_in := Field{empty_text: "What"}
amount_in := Field{empty_text: "Amount, e.g. 12.30" on_return: |text| add()}
fn add(){
    let name = ("" + ui.name_in.text()).trim()
    let amount = ("" + ui.amount_in.text()).trim().to_f64()
    if name == "" { ui.form_note.set_text("Enter a name."); return }
    if !(amount > 0) { ui.form_note.set_text("Enter an amount above 0, like 12.30."); return }
    items.push({id: next_id name: name cents: round(amount * 100) day: day_key()})
    next_id = next_id + 1
    save()
    ui.name_in.set_text("")
    ui.amount_in.set_text("")
    ui.form_note.set_text("")
    ui.rows.render()
}
```
Cites: photos:339-340, mail:146-150, maps:271 and youtube:200 (`on_return`), API:224. ✓ (Return key included)
- ✓ `"abc"`, `"x1"`, `""` and untrimmed `" 12.5 "` give NaN; `"12.5"` gives 12.5. `round(2.05 * 100)` = 205, while the raw product is `204.99999999999997`. Always `round` before you store cents. *cited:* `on_change: |text| set_query(text)` passes the new text (news:439, API:81). A TextInput needs a numeric `height` (API:326). Password, PIN and OTP fields are refused (API:310).

## 6. Storage (`fs`, JSON, try/catch)
**Rule:** keep one versioned JSON file. Read it through `try` and check every field you use. Keep an unreadable file aside, and write through `try`. The manifest asks for `"capabilities": ["storage"]`.
```splash
let DATA = "data.json"
fn field(v, key, fallback){                     // news:174-178
    let value = try { v[key] } catch { nil }
    if value == nil { return fallback }
    value
}
fn clean(raw){                                  // keep only rows with every field the UI reads
    let out = []
    for it in raw {
        let name = field(it, "name", nil)
        let cents = field(it, "cents", nil)
        if name != nil && cents != nil {
            out.push({id: field(it, "id", 0) name: "" + name cents: cents day: field(it, "day", "")})
        }
    }
    out
}
fn load(){
    if !fs.exists(DATA) { return }
    let text = try { fs.read(DATA) } catch { "" }
    let v = text.parse_json()
    let raw = field(v, "items", [])
    if field(v, "version", 0) == 1 {
        items = try { clean(raw) } catch { [] }
        for it in items { next_id = max(next_id, it.id + 1) }
        if items.len() == try { raw.len() } catch { -1 } { return }
    }
    fs.write("data.bad.json", text)             // keep what could not be read (no rename in fs)
    save()
    toast("Some saved data was unreadable; a copy is in data.bad.json.")
}
fn save(){
    let err = try { fs.write(DATA, {version: 1 next_id: next_id items: items}.to_json()) } catch { "failed" }
    if err != nil { toast("Couldn't save. Your last change is not stored.") }
}
```
Cites: photos:81,163-172 (versioned store), news:174-178 (try), camera:30-36, API:121-147. ✓
- ✓ The exact form is `try { <one expression> } catch { <fallback expression> }`. It also catches errors raised in functions it calls. `try { a; b } catch {…}` did **not** give `b`, so keep the protected part to one expression. There is no `catch e` binding.
- ✓ `parse_json` never raises, and you cannot rely on it to return nil. `"{nope"` gave an object. Truncated `{"version":1,"items":[{"id":1,` gave version 1 with one item `{id: 1}`. Check each row, not just the version.
- ✓ Broken files tested: `{nope`, truncated JSON, `[1,2]`, `42`, an empty file, `"version":2`, `"items":"zz"`, `"items":7`, and a row without `cents`. Each started with the good rows (or none), wrote data.bad.json and showed the toast. Without the `try`, `for it in "zz"` raised `for loop source is not iterable (expected number, range, object, array, or nil)` and boot stopped. An fs failure outside `try` **stops the rest of the handler** and logs, for example, `[E] … path escapes the app's storage` or `file not found`. Inside `try` you get the fallback. `fs.write` returns nil on success.
- To migrate, bump `version` and convert the old shape in `load()` before `clean()` (photos:167 checks `v.version == 1`; *cited*). The jail is `<app>/.local-state/<id>/` in card-host. Limits are 1 MiB per file and 256 entries (API:123-143). Declare `storage` even though `fs` works without it; otherwise the store tells people the app "Stores nothing" (CAP:27).

## 7. Strings and dates
**Rule:** strings cannot be indexed. Use `split`, `search`, `strip_prefix`, `replace` or a regex. Dates come from `local_time()`, which is **UTC in card-host**.
```splash
fn two(n){ if n < 10 { return "0" + n } "" + n }     // camera:78
fn month_key(){ let t = local_time(); "" + t.year + "-" + two(t.month) }   // "2026-10"
fn day_key(){ let t = local_time(); month_key() + "-" + two(t.day) }       // "2026-10-02"
```
- ✓ `"abc"[0]` raises `cannot index 0 on string (not an object/array/pod)`.
- ✓ `"a,b,c".split(",")` gives 3 parts (parse a date: `let parts = date.split("-")`, then `months[parts[1].to_f64() - 1]`, photos:214-217). `"abc".split("")` gives `["" "a" "b" "c" ""]`, five elements with empty ends.
- ✓ `"hello".search("ll")` = 2, `.search("z")` = -1, `"hello".len()` = 5, and `"pre-x".strip_prefix("pre-")` = `x`. `"a-b-c".replace("-", "+")` = `a+b-c` (first match only). `.replace(regex("-", "g"), "+")` = `a+b+c`.
- *cited:* there is no `to_lower`, `to_upper`, `contains`, `starts_with` or `ends_with`. Use `search(p) >= 0`, `search(p) == 0` or `regex(p, "i").test(s)` (API:225-226, news:282-285). To shorten text on screen, use `max_lines: 1` (mail:153) instead of cutting the string.
- ✓ `local_time()` gives `{year month day hour minute second weekday}` (API:106); it returned hour 9 at 17:51 local (UTC+8). Weekday 0 is Sunday (2026-10-02 gave 5). `time_now()` returns Unix seconds as a float; use `floor(time_now())` for whole seconds (news:290-311).
- ✓ Whole numbers print without `.0` (`"" + 12.0` = `12`), but `"" + 1230 / 100` = `12.3` and `"" + (0.1 + 0.2)` = `0.30000000000000004`. Format numbers before you show them.

## 8. Money (integer cents)
**Rule:** store integer cents (`round(amount * 100)`) and format them with `floor`, `%` and `two()`.
```splash
fn money(cents){ "¥" + floor(cents / 100) + "." + two(cents % 100) }
// ✓ seen on screen: 1230 → ¥12.30, 205 → ¥2.05, 120000 → ¥1200.00, 0 → ¥0.00
```
Adapted from camera:78-81 and mail:117-120. Not tested with negative amounts; §5's validation rejects them. There are no thousands separators.

## 9. Bars and simple charts
**Rule:** compute the size into a `let` inside `on_render` (or in a fn) and pass it as `width:` or `height:`. Re-render when the data changes, and start the maximum at 1 so you never divide by zero.
```splash
fn top_cents(){ let t = 1; for it in items { t = max(t, it.cents) }; t }
stats := ScrollYView{width: Fill height: Fill flow: Down spacing: 10 on_render: || {
    let top = top_cents()
    for it in items {
        let w = max(4, floor(300 * it.cents / top))
        View{width: Fill height: Fit flow: Down spacing: 4
            Label{text: it.name + " " + money(it.cents) draw_text.color: secondary draw_text.text_style.font_size: 12}
            RoundedView{width: w height: 10 show_bg: true draw_bg.color: pick_one(it.cents == top, danger, accent) draw_bg.border_radius: 5.0}
        }
    }
}}
```
Cites: photos:538 (`let` in on_render), photos:584-587 (`pic.height: cell_height()`), ai:200-201 (RoundedView). ✓
- ✓ `/snap` showed bar widths 300 and 50 for 1230 and 205 cents, and the 4-point minimum for a tiny value. An inline `width: floor(300 * 0.5)` also gave 150. *cited:* filled shapes need `SolidView` or `RoundedView`. `View{show_bg: true …}` did not draw in card-host (API:289-292). Vertical bars, using `height: h` in a `flow: Right` row, were not run here.

## 10. Two-tap confirm and toast
**Rule:** the first tap stores the row id and re-renders a "Tap again" button; the second tap acts. For a toast, show a hidden named View and hide it with `start_timeout`. A counter stops an old timer from hiding a newer message.
```splash
let confirm_del = 0
fn remove(i){
    let it = items[i]
    if confirm_del != it.id { confirm_del = it.id; ui.rows.render(); return }
    confirm_del = 0
    items.remove(i)
    save()
    ui.rows.render()
    toast("Deleted " + it.name)
}
let banner_n = 0
fn toast(text){
    banner_n = banner_n + 1
    ui.toast_text.set_text(text)
    ui.toast.set_visible(text != "")
    let n = banner_n
    if text != "" { start_timeout(3, || hide_toast(n)) }
}
fn hide_toast(n){ if n == banner_n { ui.toast.set_visible(false) } }
// in the root, above the tab bar:
    toast := View{visible: false width: Fill height: Fit padding: Inset{left: 12 right: 12 bottom: 6}
        RoundedView{width: Fill height: Fit padding: 10 show_bg: true draw_bg.color: #x1c1c1e draw_bg.border_radius: 12.0
            toast_text := Label{width: Fill text: "" draw_text.color: #xffffff draw_text.text_style.font_size: 13}}}
```
Cites: ai:118-125,324-325 (two-tap), ai:19-27,339-346 (banner). ✓ The row showed "Tap again to delete", the second tap removed only that row, and the toast was gone after 3.5 s. *cited:* `start_interval(secs, fn)` returns an id for `stop_timer(id)` (API:100-113, camera:68-74).

## 11. Colors and gotchas
**Rule:** write every color as `#x…` and bind each style once as a `let`. Text is white by default, so set `draw_text.color` on every Label. A `ButtonFlat` with only `text:` is invisible on white (API:293-298, 325).
```splash
let accent = #x007aff                    // ai:173-177; with alpha: #x00000000, #xffffff22 (ai:343)
let danger = #xff3b30
let Act = ButtonFlat{height: 30 padding: Inset{left: 10 right: 10}
    draw_bg +: {border_radius: 8.0 color: #xf2f2f7 color_focus: #xf2f2f7 color_hover: #xe5e5ea color_down: #xd1d1d6 border_size: 0.0}
    draw_text +: {color: accent color_focus: accent color_hover: accent color_down: accent text_style +: {font_size: 13}}}
let ActDanger = Act{draw_text +: {color: danger color_focus: danger color_hover: danger color_down: danger}}   // ai:179-183
```
- **Hex:** a bare `#` with an `e` beside a digit can be read as an exponent (API:282). At this revision `#1e1e2e` and `#2ecc71` drew correctly without the `x` (✓ probe), but `#x` is always safe.
- **Loops:** `for i in n` runs 0..n-1. `for v in arr`, `for i v in arr` and `for k v in obj` also work. ✓ `range(3)` raises `variable range not found in scope. Did you mean: …`.
- ✓ **A missing field is an error, not nil:** `property b not found in prototype chain. Did you mean: a(1)`. Guard data you did not create with `field()` (§6).
- ✓ **Reserved words** cannot be names. `let ok = …` raises `'ok' is reserved and cannot be a variable name …`, and the whole body fails. The full list (API:321-323): `me scope self nil true false ok let var mut fn if elif else for in while loop match return break continue and or is do try use`.
- *cited:* a top-level `let`/`fn` named like a prelude name (`tick`, `floor`, a widget) opens a child scope instead of replacing it, and a fn named `tick` starts an implicit 1 Hz timer (API:115, 318-320). Pick distinct names. *cited:* each handler, timer or callback gets 200 000 instructions and 64 ms. Past that you see `script instruction limit exceeded` or `script time budget exceeded`, and the session's instruction budget is cumulative (API:269-275).
- To find errors: `grep -nE '\[E\]|splash:[0-9]+:|refused|on_render closure failed|callback error' <app>/.local-state/card-host.log`

## 12. Don't
**Rule:** ask for only what a screen uses. For this cookbook's app, the manifest needs just this (✓ `octo check` PASSED):
```json
"capabilities": ["storage"]
```
- Don't read other apps' sources or the docs unless this file lacks the pattern. If you must, read SCRIPT-API.md first, then one system app. Don't invent an API. If it is not here, in SCRIPT-API or in a system app, it does not exist for apps (AGENTS:37-41).
- Don't use the network (`net`), `images` or `web` unless the manifest declares them with exact hosts. Without `net`, `net` is not even defined: `variable net not found in scope` (API:151-153, CAP:28).
- Don't add AI or model features. `octos.*` and `model` answer `no service answers … on this device`, and `llm` is for system apps only (AGENTS:51-56, CAP:36-39). Don't put secrets in the app: no password, PIN or OTP field (they are refused) and no API key or token in the bundle (AGENTS:45-47).
- Don't request capabilities no screen uses, and don't forget `storage` when you write files (CAP:20-27). Don't write `else for`, zero-child renders, `s[i]`, `range()`, top-level `ui`, multi-statement `try` blocks, or unchecked `parse_json` fields.
- Don't fake screenshots: capture them with `tools/octo shot <port> <png>` and look at them. Keep notes, logs and `.local-state/` out of `bundle/`. Don't `pkill`. End your own card-host with `curl -s 127.0.0.1:<port>/quit`, and never touch ports or windows you did not start.

**Test loop (✓ used for this file):** with `OCTOSENSE_APP_HUB=<App Hub checkout>` set, run `tools/octo run <bundle> --port P --hidden --detach`, then poll `curl -s '127.0.0.1:P/snap?q='`. `boot()` runs 0.05 s after "ready", so the first snap can come too early. Click with `curl -X POST 127.0.0.1:P/click -d '{"x":X,"y":Y,"wait":1}'` and type into the focused input with `curl -X POST 127.0.0.1:P/k -d '{"t":"Coffee","wait":1}'`. Clicking a button takes focus, so click the input again before you type. Send keys with `curl '127.0.0.1:P/k?k=down&c=Backspace'` (or `c=ReturnKey`). A widget hidden with `set_visible(false)` is absent from `/snap`. To restart, `/quit` and run again. Finish with `tools/octo check <bundle>`, which needs `screenshots/01-main.png` (named in listing.json) to exist.
