# Keyboard shortcuts

Command shortcuts can be changed in File › Keyboard Shortcuts; these are the defaults.

| Key | Command | Key | Command |
|---|---|---|---|
| L | Line | E | Extrude |
| R | Two-Point Rectangle | Q | Press Pull |
| C | Center Diameter Circle | F | Fillet |
| D | Sketch Dimension | H | Hole |
| X | Construction | M | Move/Copy |
| O | Offset | J | Joint |
| T | Trim | I | Measure |
| P | Project | A | Appearance |
| Ctrl+Z | Undo | Ctrl+Y | Redo |
| Ctrl+N | New Design | Ctrl+O | Open |
| Ctrl+S | Save | Delete | Delete the selection |

### Presets

File › Keyboard Shortcuts › **Preset** starts from a set of keys and the mouse preset that goes
with it (Preferences › Navigation). The styles follow the default keys those applications
publish, for people used to them; only keys with a SolidCraft command are given.

| Preset | Mouse | Keys |
|---|---|---|
| SolidCraft (default) | Fusion | the table above |
| Fusion style | Fusion | the table above, plus Shift+J As-Built Joint and Ctrl+B Compute All |
| SolidWorks style | SolidWorks | Ctrl+1 to Ctrl+7: front, back, left, right, top, bottom and isometric views; F fits the view (Fillet has no key); Ctrl+B Compute All |
| Onshape style | Onshape: right drag orbits, middle or Ctrl+right drag pans | sketch: A 3-Point Arc, R Center Rectangle, G 2-Point Rectangle, Q Construction, M Trim, X Extend, U Project, Shift+F Fillet, Shift+S Point; constraints: I Coincident, H Horizontal/Vertical, E Equal, B Parallel, Shift+L Perpendicular, T Tangent, Shift+O Concentric, Shift+M MidPoint, Shift+J Fix, Shift+Q Symmetry; model: Shift+E Extrude, Shift+W Revolve, Shift+F Fillet, Shift+S Create Sketch; Shift+1 to Shift+7 the standard views, F fits the view |

A key a preset gives to both a sketch command and a model command runs the sketch's while a
sketch is open, the model's otherwise. The standard views, Fit View and Home View are commands
of their own, so any preset or you can give them a key.

Your changes go on top of the preset (the page shows it as changed): **Reset** takes one key
back to the preset, **Reset All** all of them, and Restore Defaults goes back to SolidCraft's
preset. Keys a command shares with another in the same place are listed at the top of the page,
as only the first that applies runs. Keys show with your system's names (Cmd and Option on
macOS); **Export…** writes them with Ctrl and Alt, so the file works on any system, with the
preset, your changes, the mouse preset, the S box favourites and the toolbar. **Import…**, or
dropping the file on the window (in the browser too), reads it back; keys SolidCraft keeps for
itself are left out and named.

Keys handled by the window itself:

| Key | Action |
|---|---|
| S | the S box: search every command at the cursor |
| V | show or hide the selected bodies, sketches and planes |
| F2 | rename the selected browser or timeline item |
| F6 | fit the view to the model |
| ← → ↑ ↓ | turn the view 15° (with Shift, a quarter turn) |
| Ctrl+← → ↑ ↓ | pan the view |
| Ctrl+8 | look straight at the selected face, plane or sketch |
| Space (held) | pan the view while the pointer moves |
| Ctrl+Plus, Ctrl+Minus, Ctrl+0 | make the interface larger, smaller, or 100 % |
| Esc | cancel the running command, or clear the selection |

While a value box has the keyboard (a dimension being added, a dialog field), keys type into
it and single-letter shortcuts don't fire. With Sketch Dimension (D), the box opens as soon as
you click a line, circle or arc (or the second point or line), so D, click, type the value,
Enter.

Mouse and touch:

| Action | Result |
|---|---|
| Click | select (Shift or Ctrl adds) |
| Click again on the same spot | select the next thing under the cursor (click cycling) |
| Hold the left button still (long-press on touch), or right-click › Select Other… | list everything under the cursor to pick from |
| Double-click a sketch curve or edge | select its chain or tangent loop |
| Double-click the middle button | go Home: the home view, framing everything shown (Preferences › Navigation › Middle double-click: Home, Fit or Off) |

Help › Keyboard Shortcuts in the app lists the current bindings, including any you changed.

With the **Touchpad** navigation preset, hold **Shift** while moving the pointer to pan, or
**Alt/Option** to orbit, without pressing a mouse button. Vertical scrolling zooms. These
viewport gestures are separate from rebindable command shortcuts; mouse-button selection
and editing keep their existing modifiers.

### Sketch modifiers

While drawing or dragging a sketch point:

| Keys | Action |
|---|---|
| Ctrl/Cmd or Alt/Option (default) | suppress auto-snap for that gesture |
| Shift | prefer a line midpoint when targeting |

Change the snap-suppress chord in Preferences › Sketch. Selection still uses Shift and
Ctrl/Cmd to add or toggle (Preferences › Selection can make Shift add-only).
