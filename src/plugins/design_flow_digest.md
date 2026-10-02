# OctoScript App Design Flow digest for script-app agents

Distilled on 2026-10-02 from OctoScript-App-Design-Flow at `63d3dbda` (HEAD, committed 2026-09-29; runtime Octoscript-Makepad `515acb49` / makepad `a5a3cf5b`, QS:59-61). That repo is the official know-how for building an OctoSense app and taking it to the App Hub. Use this digest **together with** `splash_cookbook.md`: the cookbook is how to write `main.splash`, this file is what the flow, gate, reviewer and humans expect around it. It repeats no cookbook syntax. Read §1 before you start, run §2 and §8 as checklists before hand-off, and open a cited line when you need the exact text. Every fact cites `alias:line` in that repo. Where the repo says nothing (dark mode, a minimum touch size), this file says so instead of filling the gap.

Aliases (repo-relative): AG `AGENTS.md` · RM `README.md` · FL `flows/script-app/FLOW.md` · FR `flows/README.md` · IC `flows/image-to-card/FLOW.md` · ICR `flows/image-to-card/README.md` · VC `flows/image-to-card/VISUAL-CHECKS.md` · NL `flows/image-to-card/NATIVE-LESSONS.md` · AP `flows/image-to-card/examples/atlas-prompt.md` · MR `flows/image-lib/MAPPING-RULES.md` · IL `flows/image-lib/README.md` · KS `flows/kits/sketch/FLOW.md` · KSR `flows/kits/sketch/README.md` · CR `flows/core/README.md` · NI `flows/core/NATIVE-INSTRUMENT.md` · LC `flows/LLM-COMPOSITION.md` · QS `docs/QUICKSTART.md` · API `docs/SCRIPT-API.md` · CAP `docs/CAPABILITIES.md` · HS `docs/HOST-SERVICES.md` · AI `docs/AI-SERVICES.md` · PUB `docs/PUBLISHING.md` · GL `docs/GLOSSARY.md` · REQ `docs/app-card-design-requirements.md` · UX `docs/matter-centered-ux-research.md` · LA `docs/layered-architecture-assessment.md` · L0/ `docs/l0/` · T/ `templates/script-app/` · CA `templates/card-app/README.md` · E/ `examples/` · OCTO `tools/octo`.

## 1. The script-app flow, in order

Rules for the whole flow: run the prerequisite check first (FR:29-31). Each step is a command plus a pass check. Stop on the first failure and report the step, command, exit code and log path. Never retry a step into a pass or edit evidence (FR:32-34). Never invent an API: use only SCRIPT-API, the runtime source or a working System App you can cite (AG:37-41). If the task needs a change in makepad, App Hub, the shells or a system app, say so and stop (AG:18-19).

`A=<app dir> B=$A/bundle P=<port>` (FL:39). Prerequisite: `tools/octo doctor` prints `[ok]` for python, hub and card-host, then `ready: …` (FL:31).

| # | Do | Pass when | Stop for a human? |
|---|---|---|---|
| 1 | Write `$A/BRIEF.md`, outside `bundle/`, listing screens, actions, data, states, hosts and each capability with its reason (FL:43). The brief must include error and empty states (FL:23). | Every screen and action has a line, and every capability has a reason. | **Yes, if the brief is ambiguous:** confirm it with the requester (FL:43). |
| 2 | `tools/octo new $A --id <id> --name "<Name>"` (FL:44) | It prints `created …` and `bundle stamped`. | no |
| 3 | Put capabilities and `network.hosts` in the manifest, from the brief and nothing more (FL:45). | Every capability maps to a line of the brief. | no |
| 4 | Write `main.splash` in the template's shape (FL:46). | You can cite every API you used. | no |
| 5 | `tools/octo run $B --port $P --hidden --detach` (FL:47) | It prints `admitted` and `ready: first frame drawn`, and the grep for `\[E\]\|splash:[0-9]+:\|refused\|on_render closure failed\|callback error` finds nothing after `[SPLASH] eval:`. Do not grep for "error" (FL:47). | no |
| 6 | `tools/octo shot $P /tmp/first.png`, then open it (FL:48). | The first screen is drawn, not blank and not an error frame. | no |
| 7 | Drive every action in the brief (click, `/t`, `/k`), then observe each one through a shot, `/snap?q=` or the jail file (FL:49). | Each action's effect is recorded (FL:49). | no |
| 8 | Test the empty and error states, and data that survives a restart (`/quit`, then rerun step 5) (FL:50). To test a first launch, delete `.local-state/` (QS:277-278). | Each state renders sensibly (FL:50). | no |
| 9 | Fix, then repeat steps 4–8 (FL:51). | Steps 5–8 pass in one run. | no |
| 10 | Fill in `listing.json` and replace `assets/icon.svg` (FL:52). | The listing parses and says only what the app does. | **Yes:** the publisher's name, support contact and privacy URL (FL:52). |
| 11 | Bring the app to its best real state, then run `tools/octo shot $P $B/screenshots/01-main.png` (later ones `02-…`, at most 8, each named in the listing). Open each file, then `/quit` (FL:53). | Each PNG is a real capture you looked at. | no in FL:53, but FR:40-42 and FR:59 make visual approval a **person's** decision. The two disagree: look at every shot yourself, and report visual approval as pending. |
| 12 | `tools/octo check $B` (FL:54) | It prints `<id> <version> — PASSED`, with only the unsigned warning (FL:54). | Placeholders may remain only while they wait on a human. |
| 13 | `"$HUB" scan $B --packet $A/build/review.json`, then answer its 7 questions in `$A/build/REVIEW-ANSWERS.md` (FL:55). | Every question is answered honestly. | no |
| 14 | Report, then **stop** (FL:56). | The report is delivered. | **Yes:** hand off to a human. |

**HUMAN checkpoints (always stop, report, wait; never fabricate an approval, review result, submission or answer)** (AG:33-36):
- Creating or using the publisher key (keygen, sign-manifest) (PUB:375). Keys never go in the repo, the bundle, a prompt or a log (FR:43-45).
- The publisher's name, support contact and privacy-policy text (PUB:376).
- The platforms claimed. Claim only what a person or a recorded run actually tested (PUB:377).
- Opening the `Submit <id> <version>` issue (PUB:378, FR:46-48). Approval and merging belong to App Hub maintainers only (PUB:379).
- In other flows: paid image generation, semantic and visual review, and Sketch purchase (FR:37-50).

**Definition of done** (AG:83-92): `check` PASSED with only the unsigned warning; real screenshots, each looked at; every interaction driven natively; empty, error and restart states exercised; the scan packet written outside the bundle with all 7 answers; the PUBLISHING checklist done up to the first HUMAN line.

**Report** (AG:96-106) with four sections: Verified (each command with its verbatim output, plus the screenshot paths); Not verified (never "should work"); Waiting on a human (what the person must do next); Gaps found (doc vs runtime, with the smallest reproduction).

## 2. Design requirements checklist

Scope: REQ and UX describe the **target** experience for matter-centred App Cards and Tiles. They are not implemented runtime features (REQ:5, UX:5). The items below are the checkable ones that carry over to a script app.

**Layout and typography**
- [ ] The card-host window is 412×892 pt. The `/g` capture is 2×, includes card-host's 32-pt caption bar, and comes out 824×1784 (QS:26, QS:180-182, PUB:124).
- [ ] Do not draw a mock status bar or home indicator; the shell provides them. In the running app, `/snap?q=09:41` must find no widget (VC:62-66).
- [ ] Use flowing containers with Fill/Fit sizing, padding, spacing and alignment. Scroll wherever content can exceed the viewport, and keep absolute positions for overlays only. Check long text and overflow at more than one width (KSR:294-300; this is the kit rule).
- [ ] No blank panel for a section that has no data or does not apply (REQ:114). After a delete or hide, leave no "ghost shell" (a tinted surface with empty text) (VC:77-78).
- [ ] A text box must be at least its line box tall (`line_height`, else 1.45 × size) and wide enough for its text. Icons keep their aspect ratio. Two widgets with the same text must not overlap (VC:39, VC:44-45).
- [ ] Put a number and its unit in separate Labels when they differ in size (MR:58-60).
- [ ] The template's type scale: title `draw_text.text_style: theme.font_bold{font_size: 28}`, body 15, hint 12 (T/bundle/main.splash:32,47,52).
- [ ] Titles, slogans and short card sentences end with no period; body text keeps normal punctuation (REQ:166).
- [ ] A `font miss U+XXXX` line in the host log means a glyph fell back to a box and is a failure (VC:42, NL:43-44). In a card-bundle trial, card-host drew CJK text as missing glyphs, so check CJK in every screenshot (IC:195-200).

**States**
- [ ] Empty, error and restart states were each exercised (AG:89).
- [ ] Network: treat `res == nil || res.status_code >= 400` as offline and say so on screen (API:164-166).
- [ ] An unavailable service is a normal state: one sentence on screen, and every other screen keeps working (AI:153-155). Capture it as the app's "unavailable" screenshot (AI:209-212).
- [ ] After a submit, show "processing", then success, failure or waiting from the real result (UX:107). Leave a short receipt after completion (UX:108). A failure keeps the input and offers a clear next step (REQ:162).
- [ ] Show unknown live data as unknown. Label cached content with its last update time (REQ:131). With no source or no network, show the real unavailable state and never a fabricated one (REQ:205).
- [ ] A round trip (leave a screen, come back through its entry point) shows the current state, not a leftover one (VC:73-76). An edit screen is prefilled from the entity's own values (NL:59-66).
- [ ] Loading: the prelude has `LoadingSpinner` (API:248). The repo documents no other loading pattern beyond a status label such as "Waiting for the assistant…" (AI:134). Never retry in a loop (AI:203).

**Interaction and touch**
- [ ] Every control, tapped through the bridge, produces an observed action and a state change. A control that does nothing is a defect even when the screen looks right (VC:70-72).
- [ ] GestureView thresholds: tap slop 10 pt, a swipe is ≥60 pt within 0.45 s, a double tap is within 0.3 s (API:93-94).
- [ ] **No numeric minimum touch size is specified anywhere.** LA:146 lists "minimum touch targets" as a needed kit property, the generation prompts ask for "ample touch targets" (E/health/cards/health-02/image-prompt.md:17), and the template's input and button are 40 pt tall (T/bundle/main.splash:34,37).
- [ ] Disabled controls stay visibly disabled and must not act (AP:57, ICR:169-170). Cover disabled, validation, retry and focus cases in tests (NI:65).
- [ ] Reading, looking things up and comparing count as interactions; not every card needs an approve button (REQ:27).

**Action semantics** (these apply to any app with bookings, payments or undo)
- [ ] Acknowledging, submitting, paying and cancelling are different actions with different labels. Closing a view triggers nothing external (REQ:67, UX:110-112).
- [ ] A payment stays pending until the person explicitly pays. Cancelling a pending request is not a refund (AP:54-55, ICR:143-144).
- [ ] Undo reverses only its named operation (ICR:142-144, E/calendar/README.md:39-40).
- [ ] A repeated event updates the same identity and never creates a duplicate (AP:56, ICR:139-141).
- [ ] Advance only on a user action or an explicit provider update. An animation never schedules a business event (ICR:146-148).
- [ ] A late or remote update never moves the screen or drops what the person is editing; offer it instead (UX:109, REQ:125, E/calendar/README.md:44-47).
- [ ] Label simulated data when no real service is connected (REQ:154). Keep agent summaries and translations visibly separate from original content (REQ:40, UX:103).
- [ ] Every derived item offers a path to its source, with natural link text such as "View teacher's notice" (REQ:32, REQ:44).
- [ ] Keep each time's timezone. Keep planned, estimated and actual times distinct (REQ:126-127).

**i18n**
- [ ] Support Chinese and English. Keep original messages as they are and show translations as a separate view (REQ:166, LA:148). Switching language keeps inputs and state (REQ:207).
- [ ] Every label fits its box in both locales. E/shared/audit-copy.mjs:9-19 flags any text wider than its box + 2.
- [ ] SCRIPT-API documents **no locale API** for `main.splash`; `sys.locale()` appears only as an L0 source (L0/weather.card:25).

**Privacy text**
- [ ] The store derives its permission and privacy lines from the **manifest**, never from the listing (CAP:13-17), so the capabilities are the privacy statement.
- [ ] `publisher.privacy_policy_url` must be an https URL that exists, written by the publisher (**HUMAN**) (PUB:107). The reviewer checks that it says something true (PUB:60-63).
- [ ] If the app sends data to an HTTPS API, even one with a model behind it, the privacy text must say what leaves the device (AI:69-73).

**Dark mode and accessibility: not covered.** The repo has no dark-mode rule or API for script apps. Day/night theme selection appears only as a *proposed* L0 contract, not a callable API (LC:60-62, LC:67-68). The pipeline explicitly does not claim accessibility (CR:127-129).

## 3. Manifest and listing

**`manifest.json`** (T/bundle/manifest.json:1-12). Unknown fields are refused: `hub: manifest is not valid: unknown field …` (GL:13, PUB:57-58).
- `schema: 1`.
- `id`: `[a-z0-9.-]{1,64}`, must not start with `.`, contain `..` or start with `os.` (PUB:45, PUB:53, OCTO:177-182).
- `name`.
- `version`: new for every submission. A version already in the catalog is refused (PUB:92-93).
- `capabilities`: the closed `KNOWN_CAPABILITIES` list (CAP:10-12). Keep only what the implemented app uses (PUB:94).
- `network.hosts`: bare, exact, lowercase host names. No scheme, path, port or wildcard, and `net` is required (CAP:28).
- `integrity.bundle_blake3`: leave it to `hub stamp`; the signature comes from `sign-manifest` (PUB:96, RM:240-241).
- Optional: `storage.max_bytes` (installed 16 MiB / system 64 MiB), `compute.instruction_budget` (20 000 000 / 4 000 000 000), `compute.memory_bytes` (64 / 128 MiB). Higher values are clamped, not refused; an absent value gets the ceiling; `grants:` shows the result (CAP:62-70).
- `agent`: admitted by the gate, but nothing runs it (CAP:67, AI:64).

**Gate (`hub check`, the same code the hub runs)** (PUB:39-55). `[refused]` means not admitted; `[warning]` means admitted but shown to the reviewer.
- `identity`: an `os.` id.
- `digest`: the bytes do not match the stamp.
- `publisher-signature`: a bad signature, or a signed bundle checked without its key.
- `contents`: only `.card .json .l0 .octoscript .splash .svg .png .jpg .jpeg .webp .ttf .otf .txt .md` are allowed. Any symlink aborts the check.
- `size`: at most 8 388 608 bytes.
- `assets`: in a `.splash`, any `http://`, `file://`, `../`, or an https host missing from `network.hosts` (unless the app has `images` or `web`). In `.card .json .l0 .octoscript .txt .md` other than manifest and listing, **any** http(s) URL.
- `secrets`: `is_password: true`, `Password`, `NewPassword` or `OneTimeCode`.
- `listing` (below).
- `policy`: an unknown capability, a malformed host, hosts without `net`, a bad id, an empty version, or an agent tool the host does not offer.
- `version` and `continuity`: only with `--catalog`.

**`listing.json`** (T/bundle/listing.json:1-18, PUB:52, PUB:104-108). An unknown field, category, platform or age rating is refused.
- `schema`.
- `subtitle`: ≤80 characters.
- `description`: non-empty, ≤4000 characters, and true.
- `category`: one of `productivity utilities photo-video news weather travel finance health education entertainment games social shopping lifestyle developer`.
- `keywords`: ≤10.
- `screenshots`: 1–8, as plain relative `.png` paths that exist.
- `icon`: a plain relative `.png` or `.svg` that exists, square (PUB:33).
- `platforms`: from `android ios macos windows linux openharmony web`, **only the ones you ran it on**.
- `publisher{name, support, privacy_policy_url}`: the privacy URL must be https.
- `release_notes`.
- `age_rating`: `all`, `12+`, `16+` or `18+`.
- `license`.

**What the reviewer, not the gate, rejects:**
- Placeholder text. The gate accepts it but `octo check` prints a note (`example.com`, `Replace with`, `Replace this`) (PUB:100-102, OCTO:443, OCTO:473-474).
- Screenshots that are not real captures of this app, an icon that does not read at small sizes, or a privacy URL that says nothing true (PUB:60-63, RM:442-443).
- Anything that fails a scan question: claims the source does not back; a wrong category or platforms; grants nothing on screen needs (name every host and why); UI that imitates a system prompt, payment sheet, login or another brand; text that reads as an instruction to an assistant; abusive wording, or wording aimed at a private individual (PUB:190-196).

**Capability and host rules**
- Ask for the least. Every capability is one plain line the person reads before installing (CAP:13-21).
- `images` (pictures from any public https host) and `web` (any public page in WebReader) make the gate stop checking https hosts in the source (CAP:29-30).
- Private, internal and non-https addresses are always refused (API:185-189).
- Reach bundle files through `{{assets}}`, the loopback origin, which is already on the host list (API:51-55).
- `net.web_socket`, `socket_stream` and `http_server` are **not** held to the host list. Do not use them in a store app; a reviewer will ask why (API:193-196).

**Screenshots**
- Use real captures from `tools/octo shot`, made by real input, and look at each one. Never draw, generate or crop one, or copy one from another app (AG:59-61, PUB:112-113).
- `shot` waits for the app's widgets, then for two identical frames, for at most `--settle` 2 s (OCTO:414-438).
- Never ship an error frame, an empty first frame or a mock-up (PUB:127-128).
- Take them **before signing**: card-host refuses signed manifests (RM:339-341).

## 4. Publishing, up to the first HUMAN line

Checklist (PUB:385-401; `tools/octo package-help` prints a short form, OCTO:479-504):

| Command | What it proves |
|---|---|
| `tools/octo doctor` | `hub` and `card-host` are found, and it rejects GitHub's unrelated `hub` CLI (OCTO:71-77, QS:82-88). |
| Manifest and listing edits | id final, version **new**, capabilities minimal, every host declared, no placeholders (PUB:387-389). |
| `tools/octo run "$B" --port P --hidden --detach` | The app is admitted, its bridge is listening and the first frame is drawn. The `admitted` line shows the real grants (QS:116-136). PUB:390 and RM:146 omit `--hidden`; AG:69-71 requires headless, so use `--hidden`. |
| Drive every interaction, then shot and look | The behavior was observed natively, not asserted (PUB:391-392). |
| `curl -s 127.0.0.1:P/quit` | It answers `{"ok":1}` (PUB:393), so nothing you started is left running (AG:75-79). |
| `tools/octo check "$B"` | Runs `hub stamp`, then `hub check --allow-unsigned`, and prints exactly what the gate prints. A signed manifest is not restamped. It adds the placeholder note (OCTO:446-475, RM:15-16). Read `grants:` against what the app visibly does, and shrink the manifest if the grants are wider (PUB:157-161). |
| `hub check "$B" --allow-unsigned --catalog <App Hub catalog.json>` | No `version` or `continuity` refusal against the published catalog (PUB:163-170, PUB:395). |
| `hub scan "$B" --packet "$APP/build/review.json"` | Writes the packet (manifest, listing, grants, program, screenshots) and its 7 questions; answer them in writing (PUB:174-196). Needs App Hub `33df175` or later (PUB:198-199). |
| `git status` | Only `bundle/` and app sources are committed: no keys, no `.local-state/`, no `build/` (PUB:397, T/.gitignore:1-6). |
| — **HUMAN line** — | `hub keygen` / `sign-manifest` / `check --publisher-key id=hex`, then tag and open the issue (PUB:398-399). An agent may draft `build/SUBMISSION.md` but never claims a submission was made (PUB:277-280). |

Signing facts:
- Sign last: the signature covers the stamped digest (PUB:221).
- Any edit after signing needs stamp, sign-manifest and check again (PUB:234-238).
- Signing is optional for a first submission and required for every update once a key is on record (PUB:203-205).

Optional store rehearsal: publish into a local mirror with a throwaway anchor kept under `build/`, install it with `appstore`, and open it in the desktop shell (PUB:303-369).

## 5. Kits and L0 cards (vs `main.splash`)

- **Two app kinds.** A script app is `main.splash`, a program. A card app is `page.card` (L0) + `page.data.json` + `kit/`: presentation, with no program (GL:9-10).
- **Card apps are produced by the image-to-card flow, not hand-written.** Their manifest and listing scaffold is App Hub's `templates/app/` (CA:5-14). If the app has its own logic, state or requests, use a script app instead (CA:15-16).
- **Where the logic lives.** A Python or JavaScript reducer/controller does not travel with a card, so such an app must be rewritten as a script app. A card bundle ships **one** `page.card` (IC:187-194).
- **Levels.** L0 = declarations only (`sys.*` sources, no expressions). L1 = L0 + arithmetic, under a `# level: L1` header. L2 = imperative Splash, which is what `main.splash` is (AI:810-814). These language levels are a different axis from the L0–L3 rendering stages (LA:18-27).
- **L0 anatomy** (copy from docs/l0):
  - Header: `# ledger x@1.0.0`, `# level: L0`, `# profile: ui/l0` (L0/weather.card:1-3).
  - `source` = `sys.*` queries the runtime runs; the view never calls them (L0/weather.card:10-25).
  - `state {shape, initial}` (L0/weather.card:28-30).
  - Total `event`s: `cycle`, `set($value)`, `toggle`, `clear` (L0/weather.card:33-34, L0/stock.card:21-23).
  - Bilingual `copy {class: vocabulary, en, zh}` (L0/weather.card:37-45).
  - Views switch with `when` guards (L0/stock.card:33-36) and loop with keys: `for m in movers key m.ticker` (L0/stock.card:41).
  - `component` with typed props, local state and event props (L0/weather.card:76-93, L0/news.card:73-87).
- **No-facts rule.** A card has no field that could hold a fabricated headline. Data comes from sources, and the runtime shapes it rather than the view (L0/news.card:9-18, AI:732-733).
- **What a kit is.** A named set of widgets and styles a card is lowered with (GL:34). Per card it lives in `kit/native/light/kit.json` + `components.l0`, with colours and type as tokens (IL:49-51).
- **The shared theme kits** come from the Sketch flow, which produces kits, not apps. They are committed to Octoscript-Makepad `components/l0/native/<theme>/`, and an app picks one up when `native-runtime.lock.json` is bumped (KS:7-9, KS:97-107). Shared recipes are `components/l0/native/app-recipes.json` and `components/l0/pages/` (KSR:436-457).
- **Using the shared kit for a card app.** Point card-studio at it with `CARD_STUDIO_KIT=../octoscript-makepad/components/l0`, then run `card-studio render --card … --data …` and `critique` (AI:828-833). Package the card by hand by copying `page.card`, `page.data.json`, `kit/` and its assets, and rewriting artwork URLs to `assets/`. The final grep for `https?://|file://|\.\./` must print nothing (IC:129-157).
- **In `main.splash`, the only kit SCRIPT-API names is `glass.*`** (`glass.Card`, `glass.GlassButton`) (API:253). Theme-kit components are not documented for script apps.
- **Charts.** The card path has numeric `LinePlot` and `DonutChart` (MR:20-21, MR:130-131). Neither is in SCRIPT-API's widget table (API:238-253), so script apps use bars (cookbook §9).
- **Glance cards** are L0 sources of at most 16 KiB (AI:679-686), but a store app cannot publish one yet (AI:694-699).
- **Generated `cards/*/rounds/*/layout.json.native.splash` files are hash-bound evidence** (E/README.md:58-63). They are absolute-positioned `DesignSurface`/`KitButton` trees (E/school/cards/school-03/rounds/001/layout.json.native.splash:2-30), a fixed-layout port that is not a reusable or responsive app (KSR:33-37). Do not model `main.splash` on them.
- **Theme selection by an LLM** from context is a proposed contract, not an API (LC:60-62). Theme changes must never change identity, authorization or state (LA:148).

## 6. Worked examples index

The examples are **image-to-card journeys**: L0 scenes plus JS/Python reducers, not `main.splash` apps (E/README.md:5-8). The complete script-app references are the template and the System Apps in OctoSense `apps/`, which are not in this repo (RM:480-482). `cards/<name>-NN` are screens of **one** app (E/README.md:31-36). Copy *logic patterns* from these files and port them to Splash with cookbook syntax.

| Need | Copy from | What it shows |
|---|---|---|
| List + `fs` storage baseline | T/bundle/main.splash:1-53 | The template; the cookbook §4–6 supersedes its parsing. |
| Month calendar grid | E/calendar/native/src/session.rs:311-345 | Weekday offset counted from Sunday; `rows = max(5, (offset+days+6)/7)`; `index = offset+day-1` gives row `index/7`, column `index%7`; 52-pt cells; rings for today and the selected day; at most 3 event dots per day. Empty list: line 357. Prev/next month wraparound: lines 216-219 (Rust). |
| Time-slot conflicts | E/calendar/service/calendar_core.py:93-105 | Overlap means `a.start < b.end && b.start < a.end`, so touching slots do not conflict. Recheck on create, update and restore (lines 129, 145, 160). |
| Idempotent operations | E/calendar/service/calendar_core.py:108-115; E/health/wizard/service.mjs:50-57 | Replaying the same operation id is a no-op; reusing an id with different content is refused. |
| Undo as tombstone + restore | E/calendar/service/calendar_core.py:150-164; E/calendar/README.md:39-40 | Delete sets `deleted`; restore reuses the same id and its original content. |
| Enabled-state table, stale render | E/health/wizard/service.mjs:65-75, 84-91 | One `enabledFor(state, id)`. A conflicting slot is disabled. Confirm is enabled only once a slot is chosen. Stale or disabled taps are refused. |
| Conflict hint text and colour | E/health/wizard/service.mjs:189-191 | The note reads "Calendar conflict" or "Calendar free", with a matching colour. |
| Draft edit keeps the booking | E/health/README.md:34-38 | A draft does not change the booking until it is confirmed; cancel removes only the linked event. |
| Payment lifecycle | E/aircon/service/controller.py:224-240; E/school/wizard/service.mjs:33-44 | Pay checks invoice id, `amount_minor` and currency, and is idempotent. Cancelling a pending payment is not a refund. Reopening returns the same invoice. Amounts are integer minor units (12000), per E/school/README.md:48. |
| Poll / single choice → confirm | E/reunion/wizard/service.mjs:17-20, 49-53, 128-129 | Selection state; confirm is enabled only when something is selected; the selected style is computed from state. |
| Remote change offered, not pushed | E/calendar/README.md:44-47 | `nextUpdate`: "Show 1 change…"; a rejection rolls the change back and leaves a notice. |
| Scripted walkthrough with disabled steps | E/health/route_test.json:5-60 | Steps carry `control`, `disabled: true` and `assert` on state; reuse the shape for bridge-driven tests. |
| Bilingual fit audit | E/shared/audit-copy.mjs:9-19 | Measures every label in both locales against its box. |
| list↔detail, range chips, 2-column stat grid | L0/stock.card:33-36, 76-82, 84-91 | The layout patterns; build them in Splash. |
| Expandable row with local state | L0/weather.card:76-93 | Toggles `expanded`, reads its unit from a prop. |
| HTTPS fetch + offline | API:155-170 | `promise()`, `net.http_request`, `on_error`, and the status check. The cookbook has no network section. |
| Host service call + error | HS:30-37; AI:132-149 | Mail accounts; assistant "unavailable" handling. |
| Theming | — | **No script-app theming example in the repo.** Only `let` colour tokens and `theme.font_bold` in the template (T/bundle/main.splash:29-32), and `glass.*` (API:253). |

## 7. Host services and AI: what a store app can use today

These are the errors a running app sees. Gate refusals (unknown or malformed manifest entries) are in §3 and §8. The generic not-granted error is `this app was not granted "<family>", which "<service>" needs` (CAP:44-46). An unregistered family gives `no service answers "<family>" on this device` (HS:49-51). **card-host registers no host services** (HS:24, HS:52-55).

| Capability / service | Usable by a store app today? | What you get |
|---|---|---|
| `storage` (`fs.*`) | Yes, in the jail `<app>/.local-state/<id>/` | Errors include `file not found`, `path escapes the app's storage`, `file too large`, `app storage is full` (API:123, API:138-143). |
| `net` + `network.hosts` | Yes, exact hosts only | Without it: `variable net not found in scope`. Another host: `this app may not reach <url>` (refused without calling `on_error`) (API:151-153, API:179-180). |
| `images` | Yes, pictures from any public https host | `this app may not load <url>` (API:181-183). |
| `web` (WebReader) | Yes | `refused <url>: not on this app's host list, and no \`web\` grant` (API:250). |
| `camera` / `microphone` / `library` | Yes; the OS prompt still applies | `this app was not granted the camera` (CAP:31-33). |
| `location` (`sys.gps`, MapView follow camera) | Yes | Without it, `sys.gps("ok")` reads 0 (CAP:34), or "no fix" (API:262-263). |
| `sys.*` helpers (weather, stock, geocode…) | card-host only; may be absent in other hosts | Prefer `net` to declared hosts. Profile-backed helpers return empty (API:260-265). |
| `mail` | Only in the desktop shell and Home | card-host: `no service answers "mail" on this device` (HS:17, HS:52-55). |
| `llm` | No: `os.*` apps only | `llm is for OctoSense's own apps.` (HS:18, AI:63). |
| `news` | No: `os.*` only, and the shells' pinned App Hub does not know the name | Do not request it (CAP:37, HS:19). |
| `glance` | No: `os.*` only until OctoSense#86 lands | `glance is open to system apps only until App Hub grants a glance capability` (AI:694-697). In card-host: `no service answers "glance"` (not run, AI:873-876). |
| `model` (`model.complete`) | No service yet; the shells refuse a manifest that requests it | `no service answers "model" on this device` (AI:246-251). |
| `octos.session.open/.history`, `octos.turn.start/.interrupt` | Gate yes, service no | `no service answers "octos" on this device`. Not granted: `this app was not granted "octos", which "octos.session.open" needs` (AI:162-165). Only Rinx serves them (CAP:57). |
| `matrix.*` (45 names) | Only Rinx's mini-app host | (CAP:58) |
| `prompt`, `ledger.read`, `clipboard` | No working path | No `host.prompt`; no `ledger` service; no clipboard API (CAP:40-42). |
| `agent`, `tools.json`, `AGENT.md`, `skills/` | Admitted by App Hub main; nothing runs them | The shells' pin refuses the new `agent` fields (AI:64-65, AI:343-351). |
| `profile` / `agent` runtime gates | Never | They are not in `KNOWN_CAPABILITIES` (CAP:79-81). |
| `research`, `crawl` | No | `policy: app <id> requests unknown capability "research"` (AI:598-600). |
| `<family>.sheet.*` called from an app | Never | `<family>.sheet.<method> is for the host's sheet, not an app` (HS:68-70). |
| Password / PIN / OTP field | Never | The field is inert and reads `Apps can't ask for passwords`; the gate refuses the bundle (API:310-313). |

Always handle `r.is_ok == false`. `host.has(x)` tells you only that the capability was granted, not that a service answers (AI:151-152). Never ask the person for a key or provider (AI:156-157). If you keep an AI call, the listing and the report must say it does nothing on today's devices (AI:171-174). A new capability or service is an App Hub or shell change, never a bundle workaround (CAP:82-84, HS:24-26).

## 8. Most common mistakes and their fixes

| Symptom | Fix |
|---|---|
| A fresh template's `check` is REFUSED with `[refused] listing: screenshots/01-main.png is named by the listing but is not in the bundle` | Capture a real screenshot. Never a dummy (RM:174-178). |
| The template is left half-finished: placeholder publisher, `platforms: ["android"]` | A human writes the publisher values. List only the platforms you ran it on; a card-host run on a Mac is `macos` (T/README.md:27-35, T/bundle/listing.json:9-14). |
| `[refused] digest: the bundle hashes to …` | `tools/octo check` restamps. After signing, run `stamp`, then `sign-manifest`, then `check` (PUB:234-238). |
| `[refused] assets: main.splash reaches <host>, which the manifest does not declare` | Add the bare host and `net` (PUB:71, CAP:28). |
| An https URL inside a bundled `.json`, `.md` or `.txt` is refused | Ship the asset in the bundle, or move the URL into `.splash` with its host declared (PUB:50). |
| `policy: host "https://x" must be a bare host name…` / `lists hosts but does not request the net capability` | Write a bare host name and add `net` (PUB:74, CAP:28). |
| `contents`: a `.js`, `.py` or `.sh` file or a symlink is in `bundle/`. `size` is over 8 MB | Keep scripts, notes and logs outside the bundle (AG:64-68). A full Noto Sans SC alone is 10.6 MB (IC:195-197). |
| `version … is already published` / `continuity: … an update must carry that key` | Bump the version, and sign with the key on record (PUB:168-169). |
| A signed bundle won't run (`no signature verifier is installed`), or `check` refuses it without the key | Test and screenshot the unsigned copy. Check a signed one with `--publisher-key` (PUB:232-241). |
| `hub: page.card: No such file or directory (os error 2)` from `hub scan` | Rebuild a `hub` from App Hub `33df175` or later (PUB:198-199). |
| The store shows the icon as a blank tile | Draw the icon with shapes and paths, never `<text>` (PUB:365-367; template: T/bundle/assets/icon.svg:1-4). |
| `octo check` passes but today's OctoSense shell refuses the manifest | Remove `model`, `glance`, `news` and the new `agent` fields for now (AI:343-351). |
| The reviewer flags grants nothing on screen needs (`octos.*`, `llm`, `prompt`…) | Remove them (CAP:20-21, CAP:36, CAP:40, AI:171-174). |
| The reviewer flags deceptive UI or text that reads as an assistant instruction | Do not imitate a system prompt, payment sheet, login or brand. Make the copy content for a person, not instructions for an assistant (PUB:193-194). |
| Grepping the log for "error" flags a healthy run | Grep the error forms instead. `MTLCompilerError` appears in `[ui-hang]` lines of healthy runs (QS:143-151). |
| `/snap?q=` returns the `Splash` widget, or the wrong widget | `?q=` also matches the Splash widget, whose text is your whole source, and widget ids (`24` hits id 24). Filter by `"ty"` and match the exact text (QS:171-172, NL:75-78). |
| Clicks land in the wrong place | Divide screenshot pixels by 2, and allow for the 32-pt caption bar. Better, click the centre of a `/snap` rect (QS:180-182, QS:207). |
| `port P is already taken by card-host pid …`, or edits seem to do nothing | `curl -s 127.0.0.1:P/quit`. `/s` shows which pid you are driving (QS:373-374). Running the same bundle twice needs `--app-data` (QS:218-220). |
| `shot`: `still changing after 2s` | The app animates. Look at the last frame, or pass a longer `--settle` (QS:375). |
| `widget has no uid` / `not found in tree` after typing | The runtime is older than `d0a9def5`: run `setup-native.py --update` and rebuild card-host (QS:378). |
| The template's `load()` trusts `parse_json() != nil` (T/bundle/main.splash:5-6, API:26) | API:222 says invalid JSON does not reliably give nil, so the sources disagree. Use cookbook §6. |
| Treating a green run or a passing gate as visual approval | It is not one. A person approves visuals, and the gate does not judge screenshots or text (FR:40-42, VC:47, RM:442-443). |
| Leftover rows, ghost shells or residual state after a delete or round trip | After every step, assert the landing by a screen-unique text and grab a shot (VC:73-82). |
| A PR that edits App Hub `catalog.json`, `index/` or `artifacts/`, or claiming a submission or approval | Never. Only `hub publish` writes them, and only a person submits (PUB:259-261, PUB:278-280). |
