# A tour of the window

![The SolidCraft window](../generated/sample-plate.png)

- **Title bar.** On Windows and Linux the application bar is the window's title bar. On the left:
  the File menu (New, Open, Save, Save As, Export…), undo and redo, the Home tab (the start page)
  and one tab per open design (each with its own undo history, selection and camera; a dot marks
  unsaved changes). On the right: Report a bug, Help (?), Preferences (which hold the theme) and the
  window's buttons.
- **Toolbar.** The workspace switcher (DESIGN) sits on the left. The tabs (SOLID, SURFACE,
  SHEET METAL, PLASTIC, UTILITIES, and SKETCH while you sketch) hold panels of commands. Click a
  panel's name for all its commands. Drag a command from the dropdown onto a panel's label
  or empty space to append it; the insertion line shows its destination. Drop onto an existing
  button to insert before it. Buttons can also move between panels, and the layout is saved.
- **Browser** (left). The design as a tree: document settings and units, named views, the
  origin, bodies, sketches, construction planes, canvases, joints and components. The eye shows or
  hides an item, a component's radio button makes it the active component, and right-clicking any
  row gives that item's menu. Rows drag into groups and other components.
- **Viewport.** Click to select, Shift or Ctrl to add (Preferences › Selection), drag for a window or crossing box. While sketching, Ctrl/Cmd or Alt/Option suppress auto-snap by default; Shift prefers a midpoint. To
  reach something under something else (two lines drawn on top of each other, coincident points,
  an edge in front of a face, overlapping bodies), hold the left button still for half a second
  or right-click › **Select Other…** (on a touch screen: long-press, then **Select Other…**): a list of everything under
  the cursor opens there, the sketch being edited first. Hovering a row lights it up; clicking it
  selects it (Shift or Ctrl adds), and in a command's dialog it goes to the active selection box.
  Esc or a click elsewhere closes the list. Clicking again on the same spot, with nothing changed
  in between, selects the next thing in that list. The
  wheel zooms at the cursor, the middle button pans and right-drag orbits freely (the mouse
  preset in Preferences switches to SolidWorks, Inventor, Alias, Blender, Tinkercad, OpenSCAD or Touchpad style). On a
  trackpad, pinch zooms and two-finger scroll pans (Navigation › Two-finger scroll pans;
  off: scroll zooms like the wheel); mouse wheels zoom and the thumb wheel pans sideways.
  On a touch screen (a tablet, a touch laptop, the web app on an iPad), whatever the preset:
  - tap to select;
  - drag one finger from the model to orbit (to pan while a sketch is open). A drag that starts on
    a handle, a dimension or the open sketch's geometry works that instead, and a sketch tool
    draws with it;
  - rest a finger for a moment before dragging to select by a box (left to right: window; right
    to left: crossing);
  - drag two fingers to pan and pinch to zoom; twisting them rolls the view when
    Navigation › **Twist two fingers to roll** is on;
  - tap with two fingers for the context menu (with a tool running: done with it); long-press
    opens it too;
  - swipe three fingers left to undo and right to redo.

  A second finger always takes over from the first: a box or a sketch drag the first had started
  is let go. The page itself never scrolls or zooms under the fingers.
  Shift+middle orbits freely
  in the default and Inventor presets. In every preset, Ctrl+left drag (Cmd on macOS) that starts
  on empty space orbits; a Ctrl+click still adds to the selection. Shift+right-drag snaps to the nearest standard or
  diagonal view; Shift does not request snapping when the preset already requires it to orbit.
  With **Touchpad** selected in Preferences › Navigation, hold **Shift** and move the pointer to
  pan, or hold **Alt** (**Option** on macOS) and move it to orbit—no mouse button is needed.
  Vertical two-finger scrolling zooms in this preset, regardless of the saved two-finger pan
  setting; switching presets restores that setting. Release the modifier to stop navigating.
  Shift+click and Shift+left-drag still select. Combined Shift+Alt or Ctrl/Cmd modifiers do not
  start pointer-only navigation.
  The axis view shows the world's X/Y/Z directions in SolidCraft's colors. Click a signed
  axis tip to look along that axis; click the aligned tip again to look from the opposite side.
  Drag the axis view to orbit around the view centre, with Shift for snapping. Top and bottom
  keep the nearest quarter-turn heading. The house, the navigation bar's Home button and a middle-button double-click in the view go
  home, framing what is shown now (Navigation › **Middle double-click** can make it Fit, which
  keeps the orientation, or turn it off).
  Right-click for the marking menu: eight commands around the cursor, plus a
  menu for whatever is under it. Near the window's bottom or right edge a menu opens above or
  to the left of the cursor, so it stays in view.
  Shaded views omit certified smooth joins between spline spans on extruded
  walls. Real creases, cap outlines and authored face splits remain visible.
  Wireframe retains its edge presentation, and hiding a join does not remove its CAD
  identity or change the exported solid. Unsupported or uncertain wall geometry
  keeps its existing edge presentation.
- **Dialogs** dock on the right with a live preview. Most features also show drag handles and a
  value box in the viewport.
- **Timeline** (bottom). Every feature in order. Double-click to edit, drag the marker to roll
  back, and drag features to reorder them.
- **S box.** Press S anywhere for a command search at the cursor. Pin the commands you use most
  to its top.

Preferences (File › Preferences) hold the theme, the units of new designs, the mouse preset, the
visual style and autosave. Graphics › Colour vision gives the colours that carry a meaning (sketch
states, axes, bends, analyses, icons) values for a weak red, green or blue sense: Protan, Deutan or
Tritan. In a mode, the faces Draft and Accessibility Analysis mark as failing are striped too.
Command messages and errors show for a few seconds in the status chip at the bottom left of the view. Click the chip, or choose Help › **Messages…**, for every message of the session: newest first, errors in red, each line selectable, with **Copy all** for a bug report.
Help › Keyboard Shortcuts lists every key, and File › Keyboard
Shortcuts changes them.

### Touch mode

General › **Touch mode** lays SolidCraft out for fingers. **Automatic** (the default) turns it on
in a browser whose main pointer is a finger (a tablet or a phone); **On** and **Off** choose for
any screen.

- The toolbar gives way to a column of large buttons along the left edge (General › **Tools on
  the right** moves it to the right edge). Its first button names the tab (SOLID, SURFACE…) and
  lists the tabs and workspaces; each button below opens one panel's commands beside the column,
  in more than one column when the list is long. Tap a command to start it.
- With something selected, the column also shows the first actions of its context menu, and
  **More** for the whole menu. The panels keep their places.
- Undo and Redo sit at the column's foot, where the thumb rests; **Done** above them finishes a
  running tool.
- Everything is drawn 20 % larger (on top of General › Interface scale), and a finger may move a
  little further than a mouse and still tap.
- A tap adds to the selection, or takes out what is already selected; a tap on empty space
  clears it.

The touch gestures above work the same in both layouts.

### Colour themes

General › **Theme** lists Dark, Light and System (follows the operating system), the built-in
Gruvbox themes (dark and light, each in hard, normal and soft contrast) and any themes you have
imported. A choice applies at once.

A theme is a small JSON file that starts from the Dark or Light palette and changes any of its
colours by name:

```json
{
  "format": "solidcraft-theme/1",
  "name": "My Theme",
  "base": "dark",
  "colors": {
    "panel": "#282828",
    "text": "#ebdbb2",
    "accent": "#83a598",
    "viewport_top": "#32302f",
    "viewport_bottom": "#32302f",
    "triad": ["#fb4934", "#b8bb26", "#83a598"]
  }
}
```

Colours are `#rrggbb`, or `#rrggbbaa` for translucent ones. A few colours, such as the view
triad's X, Y and Z, take a list. Any colour the file leaves out keeps the base palette's value.

- **Import…** adds a theme file and turns it on. Importing a file with the same name again updates
  that theme. A file with an unknown colour name or a malformed colour is refused, and the message
  names every problem.
- **Export…** saves the theme on now as a complete file listing every colour. It is a good starting
  point for your own theme.
- **Remove** deletes the selected imported theme.

Imported themes are kept with your preferences, so they work in the browser app too. Colour
vision modes still apply on top of any theme.

Navigation › **Smooth wheel zoom** is on by default. Turn it off to apply each wheel or
trackpad event immediately, with no smoothing tail. It is independent of **Animate view
changes**, which controls axis views, Look At, Home and Fit. Re-enabling smoothing during
a wheel gesture takes effect after that gesture ends, avoiding leftover zoom being replayed.

Navigation › **Alias** uses Alias's camera keys: Shift+Alt+left drag orbits, Shift+Alt+middle
drag pans and Shift+Alt+right drag zooms (drag down to move away, up to move closer). Without
Shift+Alt, a left drag still selects with a box and drags sketch geometry.

Some Linux desktops take Alt+drag for themselves (moving or resizing windows), so the viewport
never sees it. To free it:

- **KDE Plasma:** System Settings › Window Management › Window Behavior › Window Actions, set
  **Modifier key** to Meta (or None).
- **GNOME:** run `gsettings set org.gnome.desktop.wm.preferences mouse-button-modifier '<Super>'`
  (or set Tweaks › Windows › Window Action Key to Super).

Shift-orbit snapping includes 45-degree edge views and isometric corner views, with a
lightly damped spring transition and a small boundary margin to prevent flicker.

On desktop, Ctrl/Cmd+Plus (or Equals) enlarges the interface, Ctrl/Cmd+Minus reduces it,
and Ctrl/Cmd+0 restores 100%. These shortcuts update the UI scale preference. The UI scale
slider in Preferences applies the new layout when released, so it stays still while dragging.
