# Handoff: host SolidCraft's `instance` branch, connected to an internal CAD-file portal

For the agent that takes this over. You are starting from the `instance` branch of the SolidCraft
fork and building on it a hosted, browser-based SolidCraft that opens and saves designs from an
internal portal of CAD files. Read this document completely before changing anything.

## 1. Where things are

| What | Where |
|---|---|
| Fork | <https://github.com/yashas-salankimatt/solidcraft> (`origin`) |
| Upstream | <https://github.com/tangency-labs/solidcraft> (`upstream`; never push there) |
| Your base branch | `instance` (commit `30408e40` when this was written) |
| Local checkout (the author's machine) | `~/Documents/solidcraft-instance`, a worktree of `~/Documents/solidcraft-hybrid` |
| Sibling forks (synced to upstream on 2026-10-20) | `solidcore`, `craft-libs` (fork of storytold/craft-libs), `OCCT` (not used by `instance`) |

`instance` is upstream `main` (`e9b1a077`, 2026-10-20) plus four commits:

| Commit | What |
|---|---|
| `90658241` | Touch: one shared gesture layer for touch screens (`crates/ui-egui/src/touch.rs`) |
| `91683e67` | Touch mode: SolidCraft laid out for fingers (`crates/ui-egui/src/touch_mode.rs`) |
| `f99790e1` | Shortcut presets: Fusion, SolidWorks and Onshape style keys and mouse (`crates/ui-egui/src/keymap.rs`) |
| `30408e40` | Touch: a second finger takes back a sketch tool's moving click (upstream #2448 fix-up) |

The first three are cherry-picks (`-x`) of fork PRs #5, #6 and #7. The fourth fixes a clash with
upstream #2448, found when the branch was built on the newest `main`.

**What is not on `instance`:** the OCCT geometry-provider work. That means no provider code, no
saved provider results, no replay bundles and no fillet/chamfer fallback. It lives on the fork's
`hybrid` branch with its own handoff doc (`docs/agent-handoff-occt-fallback-touch-shortcuts.md`
on the `docs/occt-fallback-touch-shortcuts` branch). Never merge `hybrid` into `instance`.
`git grep -i occt` on `instance` finds only what upstream `main` already has: a reference-checking
script and vendored test file names. A browser build could never run OCCT anyway, since the
provider is a separate native program.

## 2. Rules that still apply

`instance` is your deployment base, but it is still SolidCraft and follows the repo's rules.
Read these before your first change:

- `AGENTS.md` (the repo's agent rules; `CLAUDE.md` points there), `CONTRIBUTING.md`
- `docs/web-runtime.md`, `docs/book/src/guide/files.md`, `docs/design-container.md`,
  `docs/control-protocol.md`, `docs/mcp.md`, `docs/book/src/dev/architecture.md`

The ones most likely to bite on this project:

- **Never crash.** No `unwrap`/`expect`/`panic!` in non-test code and no `unsafe`; errors are
  `Result`s. Treat anything from the portal as hostile input, like design files are: sizes
  capped, no indexing with `[i]`.
- **Licences.** Never read, copy or link GPL/LGPL/AGPL code (OpenCASCADE, FreeCAD, …). Pick only
  MIT/Apache/BSD-style crates. Never copy a commercial CAD product's icons, artwork or text, and
  never drive commercial CAD to make test data.
- **Everything is a command.** Opening from or saving to the portal should be engine/UI commands
  (`CommandSpec`), so the toolbar, palette, scripts, control channel and the browser's
  `window.solidcraft` bridge all reach them. Programmatic calls never open dialogs.
- **Layering.** `cargo xtask layers` enforces it: geom → sketch/kernel/render → doc → io → engine
  → ui-egui → apps. Nothing below `ui-egui` depends on egui/wgpu/`web_sys`. Browser-only network
  code belongs in `apps/solidcraft-web` (or a small crate it alone uses), behind a trait or
  callback the UI calls.
- **Never break wasm.** `cargo xtask wasm` (part of `cargo xtask ci`) builds every library crate
  for `wasm32-unknown-unknown`.
- **Lists many PRs add to.** Message catalogs stay sorted (`cargo xtask lists --fix`), and `ja`
  and `zh` are complete catalogs, so a new `cmd.*`/`ui.*` string goes into `en`, `ja` and `zh`.
  Every command needs an English label (`cmd.<id with . → _>`) and an icon that resolves.
- **No secrets in the build.** The wasm and JS are public to every user of the site. Portal
  credentials stay server-side (§6).

### Branches and syncing

- `main` on the fork is a fast-forward-only mirror of upstream. Never commit to it.
- Work on feature branches off `instance`, and open PRs against `instance` on the fork. The fork's
  GitHub Actions do not run (upstream's workflows need self-hosted runners the fork lacks), so
  `cargo xtask ci` locally is the gate.
- To take in upstream: `git fetch upstream --prune && git switch main && git merge --ff-only
  upstream/main && git push origin main`, then `git switch instance && git merge main`. Then run
  the whole gate (§7) before pushing. Merge, don't rebase: `instance` is shared.
- Upstream changes can clash with the touch code even when git merges cleanly, as #2448 did. The
  compiler and the touch tests (`touch_ui_tests`, `touch_mode_ui_tests`) catch them.

## 3. Set-up

- Rust 1.95 or newer (`rust-version` in `Cargo.toml`). The author's machine defaults to 1.94, so
  each checkout has `rustup override set 1.97.0`.
- `rustup target add wasm32-unknown-unknown`, and `trunk` (0.21.x; `cargo install trunk` or
  Homebrew).
- Google Chrome, for the browser tests (§7).
- Python 3.11+ for `cargo xtask ci`'s icon and asset checks.
- A full `target/` is about 35 GB. A fresh worktree can clone an existing one (`cp -cR` on APFS) to
  reuse compiled dependencies.

## 4. Building and hosting the web app

```sh
cd apps/solidcraft-web && trunk build --release --public-url ./   # → dist/web
```

This is the same build upstream deploys (`.github/workflows/pages.yml`): no RUSTFLAGS, no
features, and Trunk runs wasm-opt (`data-wasm-opt="z"` in `index.html`). Trunk's post-build hook
runs `cargo xtask web-offline`, which writes `build.json` (each file's SHA-256) and fills in the
service worker `sw.js`. The output is static files:

| File | Size |
|---|---|
| `solidcraft-web-<hash>_bg.wasm` | about 47 MB (about 16 MB gzipped) |
| `solidcraft-web-<hash>.js` | 176 KB |
| `fonts/BIZUDPGothic-Regular.ttf` | 4.7 MB, fetched only for CJK text |
| `index.html`, `sw.js`, `build.json`, `manifest.webmanifest`, icons, loading css/svg | small |

**After the build:** if you change anything in `dist/web` (for example `index.html`), run
`cargo xtask web-offline dist/web` again. Otherwise the service worker's SHA-256 checks fail and
installs break.

### Serving it

- **Starting points.** `packaging/docker/` (Dockerfile, `nginx.conf`, `compose.yaml`,
  `smoke-test.sh`) and `packaging/web/package.sh`.
- **HTTPS (or localhost)** is required: OPFS storage and the service worker need it. Without
  OPFS, designs fall back to localStorage, which holds only a few MB.
- **Path.** Serve the app at a URL ending in `/` (for example `https://portal.internal/cad/app/`).
  All URLs are relative, so any path works.
- **Headers.** Copy `packaging/docker/nginx.conf`:
  - wasm as `application/wasm`, the manifest as `application/manifest+json`, gzip on.
  - Hashed `*.js`/`*.wasm`/`*.css` get `max-age=31536000, immutable`.
  - `index.html`, `sw.js` and `build.json` must be revalidated on every request (`no-cache`).
  - COOP/COEP are not needed: the wasm is single-threaded.
- **CSP.** The repo sets none. If you add one, it needs:
  - `script-src 'wasm-unsafe-eval'`, plus hashes or `'unsafe-inline'` for the inline scripts
    (Trunk's module loader and the service-worker registration);
  - `worker-src 'self'`, `connect-src 'self'` (plus the portal API's origin if it differs), and
    `blob:` for downloads.

  The proxy must not rewrite the JS or wasm, or the SRI hashes fail.
- **Behind SSO.** The service worker already copes with an auth proxy in front of the app
  (`sw.js`, `deployed()`). `build.json` answered with a redirect, 401 or 403 means "sign in": the
  page is sent to the network and follows the redirect. If your proxy answers `build.json` with a
  **200 HTML login page**, the worker treats it as unknown and starts the cached build instead.
  Make the proxy return 302/401/403 for unauthenticated requests. `cargo xtask web-offline-test`
  covers the redirect case.
- **Renderer: WebGPU only, in practice.** `?webgl` (WebGL2) **panics at start-up** on this build:
  `textureLoad` from a depth texture is not supported in GLSL (an upstream bug, re-checked
  2026-10-20 in Chrome on macOS). A browser without WebGPU probably hits the same failure, because
  wgpu falls back to GL. Target WebGPU browsers and check the actual devices (e.g. the iPads touch
  mode is for) before promising support. `?webgl` is remembered in localStorage until a visit with
  `?webgpu`.

### Outbound requests and links to change

The web build makes no outbound calls except to its own origin: no telemetry, no update checks
(`http_get` is desktop-only), no AI/LLM calls (agent chat says "desktop only" on web), no crash
upload, no CDN. What points elsewhere are links:

| Link | Where | Change for an internal deployment |
|---|---|---|
| Report an issue → `github.com/tangency-labs/solidcraft/issues/new?title=…` (opens a tab, copies the report to the clipboard, never uploads) | `crates/ui-egui/src/issue_report.rs` (`ISSUE_URL`), `crates/ui-egui/src/home.rs` (Home page links) | your issue tracker |
| Documentation → `https://docs.solidcraft.io/` | `crates/ui-egui/src/help.rs` (`DOCS_URL`), `home.rs` | keep, or an internal mirror (`cargo xtask book` builds the book) |
| What's new → `github.com/bherbruck/solidcraft/releases/latest` | `home.rs` | your release notes |
| og/twitter meta tags → solidcraft.ai | `apps/solidcraft-web/index.html` | remove or replace |

### Licences and naming

- **Licence.** SolidCraft is MIT OR Apache-2.0. Help › About already shows `LICENSE-MIT`,
  `LICENSE-APACHE`, `NOTICE`, `THIRD-PARTY-LICENSES.txt` and `ATTRIBUTION.md`, built into the wasm;
  keep that page.
- **What to serve alongside the app:** those five files, and `fonts/OFL.txt` next to the font.
  Mark files you modify, as Apache-2.0 asks.
- **Regenerating third-party licences.** Upstream regenerates `THIRD-PARTY-LICENSES.txt` only on
  `main`. On `instance`, run `cargo xtask licences` when your dependencies change.
- **Trademarks.** `NOTICE` has the trademark disclaimer that covers the Fusion, SolidWorks and
  Onshape style presets.
- **Naming.** The repo's rule is "SolidCraft everywhere". Renaming the product for an internal
  deployment is the owner's decision; ask before doing it. If you do, keep the compatibility
  shims listed in `docs/compatibility.md` (the web app still reads `solvecraft.*` keys).

### Pre-set defaults (touch mode, presets) for a deployment

There is no build-time or URL option for defaults. Touch mode defaults to Automatic, which
follows the `(pointer: coarse)` media query. The shortcut preset defaults to `solidcraft`.
Three ways to change them:

1. Change the defaults in code (`prefs.rs`, `keymap.rs`).
2. Have a same-origin script seed localStorage `solidcraft.prefs` before the wasm loads, only when
   the key is absent. For example
   `{"shortcuts":{},"shortcut_preset":"solidworks","preferences":{"touch_mode":"on","nav":"solidworks"}}`.
   `shortcut_preset` is read only when `shortcuts` is present (`crates/ui-egui/src/lib.rs`, prefs
   load). Re-run `cargo xtask web-offline` if you edit `index.html`.
3. After start, through the bridge: `solidcraft.request("ui.shortcut", {preset: "solidworks"})`,
   `solidcraft.request("ui.prefs", {touch_mode: "on"})`.

## 5. How files work in the browser today

Read `apps/solidcraft-web/src/{main.rs, files.rs, api.rs}` and `crates/io/src/vfs.rs` first.
There is **no cloud, remote or authenticated open/save path**, and no storage trait. The
`craft-cloud`/`craft-tier-store`/worker-broker architecture in `docs/web-runtime.md` is a design
only; none of it is wired into the app.

- **An in-memory file table (`vfs`).** On wasm, `solidcraft_io::vfs` is a per-tab map from path to
  bytes. The engine just reads and writes paths. The web app alone uses `vfs::insert`,
  `vfs::take_writes` and `vfs::list`.
- **The path prefix decides where a file lives** (`files.rs`):
  - `/opfs/` is a design kept in the browser (OPFS);
  - `/downloads/` is handed to the user as a download;
  - `/upload/` came from the computer.
- **Start.** `files::start` copies every OPFS file into vfs as `/opfs/<name>`. Without OPFS, it
  uses localStorage `solidcraft.file.*`.
- **Writes.** `files::flush`, called every frame, drains `vfs::take_writes()`. `/opfs/` writes go
  to OPFS (asynchronously, via `spawn_local`) and `/downloads/` writes become blob downloads.
  **Writes under any other prefix stay in memory and are lost.**
- **Pickers are stubbed** (`main.rs`, `Services { pick_open, pick_save }`):
  - `pick_open` only shows the "Files in this browser" window and always resolves to `None`.
  - `pick_save` returns `/opfs/<name>` for a design and `/downloads/<name>` for anything else,
    with no dialog.
  - So every action that waits on `pick_open_for` has no working UI on web: Insert DXF/SVG,
    Insert Design, Relink, and the parameter/library/preferences/shortcut imports. The shortcut
    presets' **Import…** button is one of these; dropping a shortcuts file on the window works.
- **"Files in this browser"** (`main.rs`, `egui::Window::new("Files in this browser")`) offers
  Open, Insert, Rename, Download and Delete, plus "Open from computer…". That uses
  `showOpenFilePicker`, or an `<input type=file>` as fallback. Dropped files are taken by
  `files::take_drops`, then `open_path`/`drop_path`.
- **Open.** `SolveApp::open_path` opens a new tab, unless the current one is blank, and runs
  `doc.open`. `drop_path` inserts STEP/mesh into a design that already has features. A design
  from the computer is copied to a free `/opfs/` name first.
- **Save.** Save runs `file.save` on the design's path (into vfs), then `flush` stores it. It
  runs synchronously on wasm.
- **Autosave/recovery.** Every 30 s while there are unsaved changes, the active tab is written to
  OPFS `recovery.solidcraft` + `recovery.json`. A reload offers "Recover unsaved work?". The
  browser's "leave site?" prompt guards unsaved tabs.
- **Recents** are kept in `solidcraft.prefs` (`recent`, at most 12). At start, `sync_recents`
  (`main.rs`) **drops paths that are not in vfs**, and `open_recent` refuses them.
- **Formats.**
  - `.solidcraft` is a ZIP container (`docs/design-container.md`); plain JSON is read too. Designs
    are capped at 256 MiB.
  - Opens/imports in the browser: STEP/STP, IGES, 3MF, STL, OBJ, glTF/GLB. DXF/SVG go into
    sketches (only through the API's `text` parameter today).
  - Not in the browser: FreeCAD `.FCStd` and Rhino `.3dm`.
  - Exports: STL, OBJ, STEP, IGES, 3MF, URDF and JSON, all delivered through `/downloads/`.
- **URL parameters.** `query()` checks `location.search` with plain substring tests:
  `webgl`, `webgpu`, `sample`. There is no `?open=`. Note that a portal id or file name containing
  "webgl" or "sample" in the query would trigger those flags, so parse real key/value pairs
  before adding parameters.

### The browser bridge (`window.solidcraft`, `apps/solidcraft-web/src/api.rs`, `docs/browser-api.md`)

- **Methods:**
  - `request(method, params)` reaches any control method (`docs/control-protocol.md`);
  - `execute(command, params)` runs `engine.execute`;
  - `commands()` runs `engine.commands`;
  - `inspect(params)` runs `document.inspect`.
- **Limits:** at most 256 pending requests, JSON parameters up to 16 MiB, and no request is taken
  while a file operation runs.
- **Opening by path:**
  - `request("ui.file", {path})` opens a vfs path in a new tab.
  - `execute("doc.open", {path})` replaces the active tab.
  - `ui.documents` lists the open tabs.
- **There is no way to hand the app bytes or a URL, and no command returns design or STEP bytes.**
  A same-origin page can drive everything else, so don't serve untrusted content on the app's
  origin.

## 6. Connecting the internal portal: suggested design

These are suggestions from reading the code, not something built or tried. Confirm the portal's
API and the owner's answers to §9 first.

**User stories to aim for:**
1. Browse the portal from the app's Open.
2. Open a design or a STEP/STL/… file from the portal in a new tab.
3. Ctrl+S saves back to the same portal file, refusing a save that would overwrite someone
   else's newer version.
4. Save As / Export into a portal folder.
5. A link in the portal's own web UI opens a file directly in the app (`…/app/?open=<id>`).

**Authentication.** Serve the app and the portal API from the **same origin** behind the portal's
SSO, for example `/cad/app/` and `/api/…`. The wasm then calls the API with `fetch(…,
{credentials: "same-origin"})` and the session cookie. No token ever goes into the wasm/JS, and
CORS doesn't arise. Make unauthenticated API calls answer 401, not an HTML login page, so the app
can say "signed out" instead of failing to parse a file.

**Storage: a `/portal/` prefix in vfs.**
- **Paths.** Mirror the portal's folders in the path: `/portal/<folder>/<name>`. Linked designs
  (`component.insert_design`) keep their links **relative to the design's folder**
  (`crates/engine/src/cmd/component_link.rs`), so mirrored folders keep links between portal
  files working. Ids alone would not.
- **Opening.** Fetch the bytes, `vfs::insert("/portal/…", bytes)`, then `open_path`/`ui.file`. The
  hardened readers already treat the bytes as untrusted input; keep the 256 MiB cap.
- **Saving.** Add a `/portal/` branch to `files::flush`: upload with `PUT` and `If-Match: <ETag
  or version>`. Keep the version you opened in a map in `Files`. Treat 412/409 as a conflict: keep
  the user's work, say so, and offer "save a copy" (or save to `/opfs/`), never a silent
  overwrite.
- **Errors.** Uploads are asynchronous, as OPFS writes already are. Surface failures with
  `files.say(…, true)`, as OPFS does. A failed upload must not clear the tab's unsaved mark, so
  the "leave site?" guard and autosave still protect the work.
- **Autosave.** It stays local in OPFS. Add the portal path and version to `recovery.json`, so
  "Recover" puts the design back on its portal path rather than an anonymous copy.

**Picking.**
- Replace `pick_open` in `main.rs` with a portal browser, an egui window like "Files in this
  browser", with "Portal" and "This browser" sections. After the fetch it resolves
  `Some("/portal/…")`. That also fixes the open-type actions that do nothing on web today
  (Insert Design, Relink, imports).
- `pick_save`: keep the current path for Save. Save As offers a portal folder.
- Recents: teach `sync_recents`/`open_recent` that `/portal/` paths are fetched on demand rather
  than dropped.

**Deep links.** Parse `location.search` into key/value pairs (`web_sys::UrlSearchParams`; add the
`UrlSearchParams` feature to `web-sys` in `apps/solidcraft-web/Cargo.toml`) and keep
the existing `webgl`/`webgpu`/`sample` meanings. On `open=<id or path>`, fetch and open it after
start-up.

**Where the code goes (layering).** Network code lives in `apps/solidcraft-web` (or a small crate
only it uses) with `web_sys`/`wasm-bindgen-futures`, which it already depends on. `ui-egui` and
`engine` stay network-free.
- The UI reaches portal features through `Services` callbacks, as with `pick_open`/`pick_save`
  (`crates/ui-egui/src/lib.rs`).
- Once a file is in vfs, the existing commands do the work with `/portal/…` paths: `doc.open`,
  `file.save`, `file.save_as`, `file.export`, `file.insert_step`, `component.insert_design`.
- For agents and scripts ("everything is a command"), add control methods that wrap the async
  fetch, for example `ui.portal_list {folder}` and `ui.portal_open {path}`, so
  `window.solidcraft.request` and the control channel can browse and open too. Document them in
  `docs/control-protocol.md`.
- Optionally add bridge methods `putFile(path, Uint8Array)`/`getFile(path)` in `api.rs`, if a
  host page should move bytes itself. Pass `Uint8Array`s, not JSON, because of the 16 MiB cap.

**Suggested order:** each step is a PR into `instance` with tests and before/after screenshots.
1. Host the current build behind SSO (§4): headers, links, licences. Run the checks in §7 against
   the hosted URL.
2. Read-only: the portal picker, opening into `/portal/`, `?open=` deep links, recents.
3. Save back with versions and conflicts; Save As and Export into the portal; recovery keeps the
   portal path.
4. Linked designs across portal folders; defaults for touch mode and presets if the owner wants
   them.

**Tests.**
- Unit-test path and version bookkeeping in Rust.
- For end-to-end, extend `cargo xtask web-api-test` (`xtask/src/web_offline_test.rs`). It already
  runs a local HTTP server (`Server`, `Mode`) and drives Chrome over CDP. Add a `Mode` that also
  serves a fake portal API, with listing, GET, PUT, ETags, a 412 conflict and a 401 sign-out.

## 7. Verifying

Run these before every push to `instance`:

1. **`cargo xtask ci`** runs fmt, clippy `-D warnings`, assets, icons, licences, lists, layers,
   the tests, the wasm check, and `web-offline-test` when Chrome is installed. It takes 40+
   minutes on the author's Mac and needs the machine mostly idle: several tests have time limits.
   Results on `instance` on macOS (2026-10-20), none caused by the branch:
   - `agent_chat::discover::tests::desktop_path_can_start_an_agent_and_its_runtime` (Mac only);
   - `viewport::tests::device_navigation_allows_face_clicks_and_preserves_modifier_selection`
     (fails on upstream `main` on macOS too);
   - `tests::robustness_goldens::seed_36` and `seed_71`: identical on pure upstream `main`
     `e9b1a077` on macOS;
   - `boolean_props::identity_generic`: a random-input property test that upstream's PR CI skips.
     It writes the failing seed into `crates/kernel/tests/boolean_props.proptest-regressions`;
     discard that file change, don't commit it;
   - `cmd::clearance::tests::a_pin_seated_line_to_line_in_its_bushing_is_measured`: a time limit
     that fails under load and passes alone;
   - `thread_tests::fillet_beside_a_modelled_thread_finishes`: another time limit that fails
     under load.

   Run single tests with `cargo test --profile ci -p <crate> --lib <name>`. The default dev
   profile panics in some UI tests on an epaint texture check.
2. **`trunk build --release`**, then `cargo xtask web-offline-test --site dist/web`.
3. **`cargo xtask web-api-test --site dist/web`** checks the bridge (box, undo/redo, limits). It
   was not run for this handoff, and it loads `?webgl`, which crashes on this build (§4). Expect
   to point it at WebGPU or fix that first.
4. **The browser check used for this handoff**, `docs/agent-handoff-instance-web-check.mjs`
   (Node 22+, Chrome):
   ```sh
   cd dist/web && python3 -m http.server 8793 --bind 127.0.0.1 &
   node docs/agent-handoff-instance-web-check.mjs http://127.0.0.1:8793/ /tmp/sc-web-out
   ```
   It passed 17/17 on `30408e40`:
   - **Desktop:** WebGPU; touch mode stays off with a mouse; each preset sets keys and mouse;
     Cmd/Ctrl+1 in SolidWorks style turns to the front view.
   - **Emulated tablet:** touch mode turns on; a tap selects a face and shows the selection bar;
     a group's flyout starts Create Sketch; a pinch zooms.

   Screenshots go to the output folder. Gotchas it already handles:
   - an unsaved design's `beforeunload` prompt blocks the next navigation, so accept
     `Page.javascriptDialogOpening`;
   - the next load then offers "Recover unsaved work?" over the view, so clear site storage
     between runs;
   - handles from `ui.handles` are in egui points: 1.2 CSS px per point in touch mode at
     deviceScaleFactor 1 (use DSF 1 in emulation);
   - with a face selected, Create Sketch finishes at once (no tool or dialog), so check for an
     open sketch.

   It is a stopgap. Port what you need into `web-api-test`, which is Rust, as the repo prefers.
5. **Look at the result.** For UI changes, take screenshots (`Page.captureScreenshot`, or the
   desktop app's `ui.screenshot`) and read them. Upstream PRs carry before/after screenshots for
   visible changes; do the same in fork PRs.

## 8. Known issues and limits

- **WebGL2 crashes at start-up** (§4), so the build needs WebGPU.
- **Touch mode has never been tried on a real tablet.** It was tested with egui unit tests
  (`touch_ui_tests`, `touch_mode_ui_tests`) and Chrome's touch emulation only. Try a real iPad
  and an Android tablet early.
- **Some actions do nothing on web** because `pick_open` resolves to `None` (§5).
- **URL flags are substring matches** (§5).
- **Writes outside `/opfs/` and `/downloads/` vanish** (§5). Any new prefix needs a `flush` branch.
- **Only the active tab is autosaved.**
- **Upstream sync can break touch code** that git merges cleanly (#2448 did): read the compiler
  errors and the touch tests after every merge of `main`.

## 9. Questions for the owner before building

1. **Portal API:** REST/GraphQL? Listing, download, upload, versions or ETags, locks/check-out,
   and folder structure. Is there an API spec or a sandbox?
2. **Hosting:** same origin as the portal or not; which SSO; where the static files are served
   (nginx, S3 + CDN, Kubernetes).
3. **Files in the portal:** native `.solidcraft` designs, STEP from other tools, or both? Should
   saving a STEP-born file write a `.solidcraft` next to it, or export STEP back?
4. **Devices and browsers:** which ones must work? This decides how much the WebGL2 crash
   matters.
5. **Name and branding** for the internal deployment, and where "Report an issue" and "What's
   new" should point.
6. **Defaults:** a shortcut preset or touch mode to turn on for everyone.
7. **Upstream:** should any generic piece (for example a host file bridge in `api.rs`) be offered
   to upstream SolidCraft? Never push or open anything upstream without the owner's OK.

## Appendix: what the `instance` commits touched

| Area | Files |
|---|---|
| Touch gestures (one layer for every camera canvas) | `crates/ui-egui/src/touch.rs`, `navigation.rs` (`navigation::handle` is the only place camera input becomes motion), `viewport.rs`, `scenario.rs` (touch steps), `apps/solidcraft-web/src/main.rs` (keeps fingers as touch events), `assets/styles/web/loading.css` |
| Touch mode (tool column, flyouts, selection bar, larger targets) | `crates/ui-egui/src/touch_mode.rs`, `prefs.rs` (`touch_mode`, `touch_column_right`), `lib.rs`, `apps/solidcraft-web/src/main.rs` (`touch_first` from `(pointer: coarse)`); user doc `docs/book/src/guide/ui-tour.md#touch-mode`; handles `touch:*` in `docs/control-protocol.md` |
| Shortcut presets | `crates/ui-egui/src/keymap.rs` (`PRESETS`: solidcraft, fusion, solidworks, onshape), `prefs.rs` (Onshape mouse preset), `toolbar.rs`, `toolbar_custom.rs` (export/import with preset), `context_menu.rs`, `control.rs` (`ui.shortcut {preset}`); `crates/engine/src/cmd/view_navigation.rs` (`view.front/back/left/right/top/bottom/iso/fit/home`); user doc `docs/book/src/guide/shortcuts.md#presets` |
| Strings | `assets/locales/{en,ja,zh}.json` (`ui.touch_*`, `ui.shortcut_*`, `cmd.view_*`) |
| Tests | `touch_ui_tests.rs`, `touch_mode_ui_tests.rs`, `keymap.rs` tests, `view_navigation.rs` test |
