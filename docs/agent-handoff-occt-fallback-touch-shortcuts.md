# Implementation handoff: OCCT fallback, web touch navigation, and shortcut presets

**Status:** planning handoff, 2026-10-18  
**Primary product:** SolidCraft  
**Supporting projects:** SolidCore, craft-libs, and an OCCT fork

This is a product plan for the next agent. Keep the implementation understandable and user-visible. The goal is to make SolidCraft the main CAD application, use OCCT when the normal geometry path cannot complete an operation, keep `.solidcraft` files portable, and make the browser app comfortable on touch devices.

## Repositories and forks

| Project | Working fork | Upstream | Use |
|---|---|---|---|
| SolidCraft | [yashas-salankimatt/solidcraft](https://github.com/yashas-salankimatt/solidcraft) | [tangency-labs/solidcraft](https://github.com/tangency-labs/solidcraft) | Main product, file format, UI, MCP, web app |
| SolidCore | [yashas-salankimatt/solidcore](https://github.com/yashas-salankimatt/solidcore) | [tangency-labs/solidcore](https://github.com/tangency-labs/solidcore) | Future Rust kernel work; do not make SolidCraft wait for it |
| craft-libs | [yashas-salankimatt/craft-libs](https://github.com/yashas-salankimatt/craft-libs) | [storytold/craft-libs](https://github.com/storytold/craft-libs) | Browser storage, worker jobs, and shared runtime support |
| OCCT | [yashas-salankimatt/OCCT](https://github.com/yashas-salankimatt/OCCT) | [Open-Cascade-SAS/OCCT](https://github.com/Open-Cascade-SAS/OCCT) | Optional native/WASM geometry provider and any small integration patches |

The OCCT fork is for integration work only. Start with a pinned upstream OCCT release and keep local patches small enough to send upstream later.

## Keeping the forks current

Each clone should have two remotes:

```sh
git remote -v
# origin   = yashas-salankimatt/<repo>
# upstream = the original project
```

For SolidCraft, SolidCore, and craft-libs:

```sh
git fetch upstream --prune
git switch main
git merge --ff-only upstream/main
git push origin main
```

For OCCT, use its default `master` branch unless the selected release branch says otherwise:

```sh
git fetch upstream --prune
git switch master
git merge --ff-only upstream/master
git push origin master
```

Do this before starting a feature branch. Record the upstream commit in the branch description or handoff note. Never overwrite local work with a force pull.

## What the finished system should do

### 1. Use OCCT as a safe fallback

- SolidCraft’s normal Rust kernel remains the first choice for ordinary operations.
- When a known geometry failure occurs—such as a difficult boolean, fillet, shell, offset, sweep, or imported-shape repair—SolidCraft may try OCCT.
- The fallback must be visible in diagnostics. Never silently change kernels.
- Accept the OCCT result only after it passes shape validity, measurements, and downstream rebuild checks.
- If the result has no reliable face/edge history, keep the feature usable for display, measure, and export, but show that later parametric editing may be limited.
- Failures in sketches, timeline rules, selections, or file permissions are not geometry fallback cases. They should remain ordinary command errors.
- The web app must continue to work when no OCCT provider is available. It can report “requires the native geometry provider” or use a remote worker later.

The first diagnostics should answer, in plain language:

1. Which command failed?
2. Why did the normal kernel fail?
3. Was OCCT tried?
4. Did OCCT succeed, and how long did it take?
5. Is the resulting feature still editable?

### 2. Preserve `.solidcraft` compatibility

- Keep the existing `.solidcraft` feature timeline, parameters, components, and semantic face/edge references as the source of truth.
- Do not save native OCCT objects, pointers, or unstable topology indices in the file.
- An OCCT feature may save a portable geometry snapshot/cache and a small record of which provider created it. The design must still describe the user’s intent and parameters.
- Opening and saving a file must work on desktop, CLI, MCP, and web as far as the available provider allows.
- Older files must continue to open. Newer files must produce a clear “update required” message rather than being guessed at.
- If a machine lacks OCCT, it should open the saved display/measurement result when possible and explain which feature needs the native provider for editing or recompute.
- Saving must never silently replace a parametric feature with an uneditable shape.

Use the existing file and MCP documentation as the compatibility contract:

- [`docs/book/src/guide/files.md`](book/src/guide/files.md)
- [`docs/mcp.md`](mcp.md)
- [`docs/web-runtime.md`](web-runtime.md)

### 3. Keep MCP and the command surface unchanged

- Toolbar, command palette, scripts, CLI, control channel, and MCP must continue to use the same command IDs.
- MCP should report the provider used, fallback status, warnings, and timing in structured results.
- A headless MCP session should be able to use OCCT when the native provider is installed.
- Browser agents should use the same command surface. If the browser lacks OCCT, return a clear capability error instead of a hidden network call.
- Add a replay command or exported failure bundle so an agent can reproduce a fallback without access to the original private design.

### 4. Add touch-first browser navigation

Use one shared navigation/input layer for desktop and web. Do not add a separate gesture implementation to individual canvases.

The web viewport should support:

- tap to select;
- drag to orbit or pan according to the active navigation preset;
- two-finger pan;
- pinch to zoom;
- optional two-finger twist for roll;
- pointer cancellation and finger-count changes without leaving the viewport stuck;
- sketch gestures that do not accidentally orbit the model;
- no page scrolling while the user is manipulating the viewport.

The default should feel familiar to Fusion users, but label that mapping as provisional until reference captures are authorized. Every gesture must remain configurable through the shortcut/navigation settings.

Test the same interactions on trackpad, mouse, phone, and tablet. Test selection, sketching, timeline dragging, browser scrolling, and dialogs separately.

### 5. Add shortcut and navigation presets

Provide presets for:

- SolidCraft default;
- Fusion-style;
- SolidWorks-style;
- Onshape-style;
- custom user mapping.

Presets should cover keyboard shortcuts, mouse buttons, viewport gestures, sketch gestures, command-palette keys, timeline/browser actions, and common modifier keys. Users need to:

- switch presets without restarting;
- edit individual bindings;
- see and resolve conflicts;
- restore defaults;
- export/import a custom preset;
- use platform-appropriate modifier labels on macOS, Windows, and Linux.

Store shortcut preferences separately from the design file. A `.solidcraft` design should not change how the next user’s keyboard or touch screen behaves.

## Suggested implementation order

1. **Baseline and sync:** fast-forward each fork from upstream, run the current SolidCraft tests and browser build, and record the baseline commit.
2. **Provider boundary:** add the small internal boundary that lets one command ask for the normal kernel or OCCT without changing the document model.
3. **Diagnostics:** add structured fallback events, feature warnings, timings, and a local replay bundle. Add one CLI/MCP inspection path.
4. **First fallback:** support one high-value operation, preferably a difficult boolean. Validate the result before accepting it.
5. **Expand geometry coverage:** add fillet/chamfer, shell/offset, sweep/loft, and imported-shape repair one at a time. Each new operation gets a replay case.
6. **File compatibility:** save/open/reopen designs containing fallback features on desktop, CLI, MCP, and web. Verify degraded behavior when OCCT is absent.
7. **Touch navigation:** unify pointer and gesture handling, then add browser tests for tap, pan, orbit, pinch, sketching, and page-scroll prevention.
8. **Shortcut presets:** introduce the preset model, conflict checking, editing, import/export, and the Fusion/SolidWorks/Onshape presets.
9. **End-to-end verification:** exercise the same design through the desktop UI, CLI, MCP, and browser. Record which backend each feature used and confirm that saved files still open.

## Acceptance checklist for the next agent

- A normal design uses the Rust path without new latency or visible behavior changes.
- A reproducible hard geometry case records the failure, tries OCCT, records timing, and reports the outcome.
- A rejected OCCT result is never silently published.
- An accepted OCCT result survives undo, save, reopen, measure, and export.
- MCP can create, inspect, save, and export a design that used OCCT.
- A browser without OCCT still opens the file and explains any unavailable edit.
- Touch navigation works without page-scroll or stuck-pointer bugs.
- Presets switch immediately and custom bindings persist outside the design file.
- A replay bundle can be run locally without uploading private geometry.
- Each upstream sync is fast-forwarded, recorded, and pushed to the fork.

## Do not do yet

- Do not replace the Rust kernel globally.
- Do not make the web app depend on a native OCCT binary.
- Do not put raw OCCT data in `.solidcraft` files.
- Do not silently convert a parametric feature into a frozen shape.
- Do not upload model data automatically.
- Do not hand-edit recorded UI transcripts; re-record them with the project’s recorders.

The first implementation agent should begin with the provider boundary and diagnostics, not with a full OCCT port. The fallback data will tell us which operations deserve deeper SolidCore work and which should remain OCCT-backed.
