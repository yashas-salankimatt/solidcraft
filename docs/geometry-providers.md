# Geometry providers

A geometry provider is another kernel that SolidCraft may ask when its own kernel cannot do a
geometry operation. SolidCraft's kernel always goes first. A provider is only asked after the
kernel fails, and its result is kept only after SolidCraft has checked it. The design keeps
SolidCraft's own result, file format, commands and names as the source of truth; a provider fills
a gap, it does not replace the kernel.

This fork plans an OCCT provider (see
[the handoff](agent-handoff-occt-fallback-touch-shortcuts.md)). SolidCraft never links or contains
provider code: a provider is a separate program, and the two exchange STEP text. SolidCraft's
own code stays MIT OR Apache-2.0, a provider that crashes cannot take the app down, and the browser
build is unchanged (it has no provider).

## Installing a provider

Name the provider program in `SOLIDCRAFT_GEOMETRY_PROVIDER` before starting the desktop app or
`solidcraft-cli` (`run`, `eval`, `mcp` and every other command):

```sh
SOLIDCRAFT_GEOMETRY_PROVIDER=/path/to/solidcraft-occt-provider solidcraft-cli mcp
```

At start-up SolidCraft asks the program who it is. The CLI says on standard error which provider
it uses (`geometry provider: occt 8.0.1 (/path/…)`), or why it uses none; the desktop app writes
the same to its log. A program that can't be started or doesn't answer is not used, and every
feature evaluates as it does without one. Unset, empty or `off` means no provider.

`geometry.providers` (any command surface: palette, scripts, control channel, MCP `execute`)
reports the installed provider: `{installed: true, provider, version, program, protocol,
operations}`, or `{installed: false, message}` saying why there is none. The browser build never
has one, and says so.

## The provider program

A provider is any program that speaks `solidcraft-geometry-provider/1`. SolidCraft starts it once
per request, writes one JSON object on one line to its standard input, and reads one JSON object
on one line from its standard output; then the program exits.

| Request | Reply |
|---|---|
| `{"protocol": "solidcraft-geometry-provider/1", "op": "hello"}` | `{"ok": true, "provider": "occt", "version": "8.0.1", "operations": ["boolean", "fillet", "chamfer"]}` |
| `{"protocol": …, "op": "boolean", "kind": "union" \| "cut" \| "intersect", "operands": [STEP, STEP], "tolerance_mm": 1e-6}` | `{"ok": true, "step": STEP}` |
| `{"protocol": …, "op": "fillet", "radius": 2, "edges": [[x, y, z], …], "operands": [STEP], "tolerance_mm": 1e-6}` | `{"ok": true, "step": STEP}` |
| `{"protocol": …, "op": "chamfer", "distance": 2, "edges": [[x, y, z], …], "operands": [STEP], "tolerance_mm": 1e-6}` | `{"ok": true, "step": STEP}` |
| anything it cannot do | `{"ok": false, "error": "…"}` |

- Operands and results are complete STEP files (AP214/AP242 text) in millimetres. A Boolean's
  first operand is the target and the second the tool; a fillet or chamfer has one. The result's
  solids are its pieces.
- A fillet rounds, and a chamfer cuts back equally along both faces, the operand's edge nearest
  each point of `edges` (a point on the edge, as the feature stores it), in millimetres.
- `provider` is a short name (letters, digits, `-`, `_`, `.`); `version` is the provider's own.
  `operations` lists what it does; SolidCraft asks for nothing else.
- The reply is the first line of standard output that starts with `{`. Standard error is the
  program's log: the last lines go into SolidCraft's error when the program fails.
- A program that exits without a reply, writes more than 512 MiB, or takes longer than 10 s to
  say hello or 120 s for an operation is stopped and its request fails.

## What is asked today

- Booleans of timeline features: the join, cut or intersection a feature makes with its target
  bodies (an extrude, revolve, loft, sweep, primitive, hole, pattern or combine, and the like).
- The Fillet feature's constant-radius round (type constant, any fit) and the Chamfer feature's
  equal-distance chamfer. A radius too large for the faces around the edges is the kernel's own
  answer, not a geometry failure, so a fillet's fit still reduces or tapers it; other fillet types,
  chamfers by two distances or a distance and an angle, and blends inside a pattern are not handed
  over yet.

The kernel's internal Booleans (inside fillets, shells, clearance checks) and every other
operation are not handed over yet. An operation is handed over only when all of these hold:

- a provider is installed (above) and lists the operation (`operations` in its hello);
- the kernel failed with a geometry error (`KernelError::Failed` or `Internal`). Bad input
  (`Invalid`, a mesh body, a size too large) is not a geometry failure and is reported as before;
- its operands are B-rep solids.

With no provider installed, every operation, result and message is exactly what it was.

## Checks before a result is kept

The provider's STEP text is read back with SolidCraft's STEP reader. The result is refused unless:

1. every piece is a closed solid, and the body is sound (`Body::validity`: closed shells, every
   face meshes, positive volume);
2. a join or cut passes the checks every Boolean retry must pass (`boolean_guard`): a join never
   loses volume and a cut never gains any, and away from the tool's box the result's surface is the
   target's. An intersection is no bigger than either operand and lies inside both boxes;
3. its surface lies where the operands' surfaces say it must. Points spread over each surface by
   area are classified against the other operand: every point of the result lies on an
   operand's surface, and every point of an operand's surface that the operation keeps (a join
   keeps each outside the other, a cut the target outside the tool and the tool inside the
   target, an intersection each inside the other) lies on the result. A cut that came back
   unchanged, a hole drilled in the wrong place or a join that lost its tool fails here even when
   its volume is plausible.

A fillet or chamfer is judged against the body it was given and the edges it was asked for
instead (`provider_blend`), without SolidCraft's blends:

1. the same: every piece closed, the body sound;
2. it lies within the body's box (a blend takes material off a convex edge and fills a concave
   one, both inside it);
3. its volume changes by no more than 4 × size² per millimetre of the blended edges (a round of a
   20° edge changes about 2.4 × r², of a right angle 0.21 × r²);
4. away from the blended edges (farther than 3 × the size from them, and from the edges
   continuing them within 2°, which a blend follows) its surface is the body's, both ways:
   nothing moved, dropped or added there. A round of the wrong edge, a much larger radius or a
   moved body fails here;
5. beside every edge asked for there is new surface: an edge the provider left as it was (as it
   leaves the seam line of a round face, which is no edge to round) fails here, so a partial
   result is never passed off as the blend.

None of these use the operation that just failed. A refused result is never published: the
feature fails as it would have, and its error says why the provider's result was refused.

## What a feature says

Every operation handed over is recorded as an event on the feature's result
(`FeatureResult::geometry_fallbacks`):

| Field | Meaning |
|---|---|
| `operation` | `boolean union`, `boolean cut`, `boolean intersect`, `fillet` or `chamfer` |
| `kernel_error` | why SolidCraft's kernel could not do it |
| `provider`, `provider_version` | which provider was asked |
| `outcome` | `accepted` (kept), `refused` (its result failed the checks) or `failed` (it gave no result) |
| `reason` | why the result was refused, or the provider's own error |
| `kernel_ms`, `provider_ms`, `check_ms` | time in the kernel before the provider was asked, in the provider (writing and reading STEP included), and checking its result |

`document.inspect` (MCP `inspect_design`, `solidcraft-cli eval`) lists them on the feature's
timeline entry, and `timeline.inspect` on the feature, as `geometry_fallbacks`. The key is left out
for a feature that handed nothing over, so other designs read as before.

A kept result adds a warning to the feature in plain words: which operation the provider made,
how long it took, and why the kernel could not. A failed feature's error says what the provider
did. The command that made the feature replies with the same warning or error.

### Names

The provider's result carries no history, so its faces and edges are named from geometry and
marked unverified, as for any operation without history ([persistent naming](book/src/dev/naming.md#where-there-is-no-history)).
`model.faces` reports `exact: false` for them. The feature stays parametric: its parameters and
inputs are edited as before and the provider is asked again on the next rebuild. Features that
refer to its faces find them by geometry and say so when a face moved.

### Cached results

The evaluation cache and the timeline's reuse of results are keyed by the provider's id and
version as well as the feature's inputs (a digest of 0, and so every key as before, when there is
none). A design evaluated with a provider is never shown with that result after the provider is
gone: it is evaluated again, and the feature fails as it does without one.

## Saved results

A design saves the results a provider made for its features, so a computer without the provider
(the browser, a machine without OCCT) still shows, measures and exports them:

- Saving (`file.save`, Save As, recovery autosaves) writes `provider_results`: the feature, the
  provider and its version, the operation, a digest of the design up to and including the feature
  (`intent`), what the operation's operands (two for a Boolean, one for a blend) were like
  (volume, area, centre of mass and box) and the result as STEP text, a deduplicated blob in a container. Nothing native to a provider
  is saved. A design no provider helped saves exactly as before.
- Opening offers each feature the results saved for it, but only while the design up to that
  feature is exactly as it was saved: an edit to the feature or to anything before it (a
  parameter, a sketch, an earlier feature) means its result must be made again. The design is
  compared rather than the operands, because the same design evaluates to operands that differ in
  their last digits on another computer (the browser's floating point is not the desktop's).
- When SolidCraft's kernel fails an operation and no provider here gives a result, the saved
  result for an operation of that kind whose operands it fits is used, after the same checks a
  provider's answer gets. One that fails the checks is never used.
- The feature then says so: `outcome: "saved"` in `geometry_fallbacks`, and a warning that it is
  the result the provider made and the design saved, and that rebuilding it once its inputs change
  needs that provider. Once its inputs have changed, the feature fails and its error names the
  provider it needs and says it is not installed here.
- The feature stays parametric: its parameters and inputs are edited as always, and the saved
  result is only a cache of what the provider made for them.
- Saved results come with a new design format (`crates/doc/src/format/pending/provider-results.rs`):
  a release that predates it refuses such a design and asks to be updated, as for any newer design.

## Replay bundles

A replay bundle holds the operations a feature hands to a provider, so they can be tried again
without the design: in SolidCraft, in a provider program, or by whoever works on the kernel.

- `geometry.fallback_bundle {feature}` bundles the operations a timeline feature hands over.
  `geometry.fallback_bundle {command, params}` bundles those a command would hand over, tried on a
  copy of the design: the design itself is left as it is. That is how to bundle a join that just
  failed and was not kept. Only commands that build geometry can be tried. Both work with no
  provider installed: then they bundle the operations SolidCraft's kernel could not do.
- `folder` says where to write it (default: a new folder in `geometry-fallbacks`, beside the crash
  reports). Under MCP the folder must be one the server may write to.
- `geometry.fallback_replay {folder}` replays each operation: SolidCraft's kernel on the very
  bodies it was given, then, where it fails, the installed provider, asked and checked as a
  feature's evaluation does. The reply has each operation's `kernel_error`, `kernel_ms`,
  `operands_from` and, when the provider was asked, its `fallback` event.

```text
bundle.json              format solidcraft-geometry-fallback/1, app, source, provider, requests, error
request-1.json           the request line, as a provider program reads it on standard input
request-1-target.step    the operands, to open on their own (a fillet's or chamfer's
request-1-tool.step      one is request-1-body.step)
request-1-target.scbrep  the same bodies in SolidCraft's own encoding: STEP refits intersection
request-1-tool.scbrep    curves, and the kernel may do on the refit bodies what it could not do
```

A provider program can be run on a bundle directly:
`solidcraft-occt-provider < request-1.json`.

A bundle holds no sketches, parameters, other bodies or file names. It does hold the bodies each
operation was given, so share it only where those bodies may go. SolidCraft writes it only
where it is asked to and never sends it anywhere.

## Not done yet

- A preference for the provider program in the desktop app (today: the environment only).
- The OCCT provider program itself, in its own repository.
- Other fillet types, chamfers by two distances or an angle, then shell and offset, sweeps and
  lofts, and import repair, one at a time.
