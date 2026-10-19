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
| `{"protocol": "solidcraft-geometry-provider/1", "op": "hello"}` | `{"ok": true, "provider": "occt", "version": "8.0.1", "operations": ["boolean"]}` |
| `{"protocol": …, "op": "boolean", "kind": "union" \| "cut" \| "intersect", "operands": [STEP, STEP], "tolerance_mm": 1e-6}` | `{"ok": true, "step": STEP}` |
| anything it cannot do | `{"ok": false, "error": "…"}` |

- Operands and results are complete STEP files (AP214/AP242 text) in millimetres. The first
  operand is the target and the second the tool. The result's solids are its pieces.
- `provider` is a short name (letters, digits, `-`, `_`, `.`); `version` is the provider's own.
  `operations` lists what it does; SolidCraft asks for nothing else.
- The reply is the first line of standard output that starts with `{`. Standard error is the
  program's log: the last lines go into SolidCraft's error when the program fails.
- A program that exits without a reply, writes more than 512 MiB, or takes longer than 10 s to
  say hello or 120 s for an operation is stopped and its request fails.

## What is asked today

Booleans of timeline features: the join, cut or intersection a feature makes with its target
bodies (an extrude, revolve, loft, sweep, primitive, hole, pattern or combine, and the like). The kernel's internal Booleans
(inside fillets, shells, clearance checks) and every other operation are not handed over yet.

A Boolean is handed over only when all of these hold:

- a provider is installed (above);
- the kernel failed with a geometry error (`KernelError::Failed` or `Internal`). Bad input
  (`Invalid`, a mesh body, a size too large) is not a geometry failure and is reported as before;
- both operands are B-rep solids.

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

None of these use the Boolean that just failed. A refused result is never published: the feature
fails as it would have, and its error says why the provider's result was refused.

## What a feature says

Every operation handed over is recorded as an event on the feature's result
(`FeatureResult::geometry_fallbacks`):

| Field | Meaning |
|---|---|
| `operation` | `boolean union`, `boolean cut` or `boolean intersect` |
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

## Not done yet

- A preference for the provider program in the desktop app (today: the environment only).
- A replay bundle for an operation that was handed over (its operands as STEP and the request,
  never the design), and a command to replay it locally.
- The OCCT provider program itself, in its own repository.
- Fillets and chamfers, shell and offset, sweeps and lofts, and import repair, one at a time.
- Saving a kept result with the design, so a computer without the provider can still show,
  measure and export it, and says which feature needs the provider to be rebuilt.
