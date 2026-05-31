# Plugin Diagrams

A draw.io-style interactive diagram editor for Pulsar Engine, built on the GPUI framework with WGPU-accelerated rendering. Shapes are entirely data-driven — each shape is defined by an SVG file, and adding a new shape to the library requires nothing more than dropping a new file into the shapes directory.

---

## Overview

Plugin Diagrams provides a full diagramming environment inside the Pulsar Engine editor. It renders a dot-grid canvas via a custom WGSL shader, draws diagram elements using GPUI path builders fed directly from parsed SVG path data, and manages undo/redo history through a deadlock-free command architecture that separates history ownership from document state.

The editor is structured as three panels arranged horizontally: a shape palette on the left, the interactive canvas in the centre, and a properties inspector on the right.

---

## Architecture

```
Plugin_Diagrams/
├── src/
│   ├── lib.rs
│   ├── plugin.rs
│   ├── panel.rs                    Main docking panel and toolbar
│   ├── shape_def/
│   │   └── mod.rs                  SVG loader, expression evaluator, path parser
│   ├── shapes/                     Built-in shape SVG definitions
│   │   ├── rectangle.svg
│   │   ├── ellipse.svg
│   │   ├── diamond.svg
│   │   ├── triangle.svg
│   │   ├── parallelogram.svg
│   │   ├── hexagon.svg
│   │   ├── cylinder.svg
│   │   ├── table.svg
│   │   ├── text.svg
│   │   └── connector.svg
│   ├── elements/
│   │   ├── element.rs              DiagramElement, ElementStyle, resize helpers
│   │   └── render.rs               Shape rendering via PathBuilder and paint_quad
│   ├── state/
│   │   ├── document.rs             In-memory element store, hit testing, selection
│   │   ├── history.rs              Command trait taking &mut DiagramDocument
│   │   ├── commands.rs             Add, remove, move, resize, param commands
│   │   └── tool_state.rs           Active tool enumeration
│   ├── canvas/
│   │   ├── viewport.rs             DiagramViewport: interaction, history, rendering
│   │   └── renderer/
│   │       ├── canvas_renderer.rs  WGPU grid background renderer
│   │       ├── types.rs
│   │       └── shaders/grid.wgsl   Dot-grid and canvas shadow shader
│   ├── panels/
│   │   └── properties.rs           Per-element properties inspector
│   └── ui/
│       ├── toolbar.rs
│       └── shape_palette.rs        Preview grid of all loaded shapes
└── examples/
    └── standalone.rs               Runs the editor without the engine
```

---

## Shape Definition System

Each shape is an SVG file. The file carries everything the editor needs: geometry, parameter metadata, and drag handle positions. No Rust code needs to change when a new shape is added.

**Path templates.** The `d` attribute of each path element may contain `{expr}` tokens. At render time these are substituted with evaluated values before the path string is parsed. The expression language supports the four arithmetic operators with correct precedence, named variables for `w` (element width), `h` (element height), and any declared parameter.

```xml
<!-- rectangle.svg -->
<svg data-name="Rectangle" data-category="General">
  <param name="r" label="Corner Radius" default="0" min="0" max="50" type="float"/>
  <path role="body"
        d="M {r} 0 L {w-r} 0 Q {w} 0 {w} {r}
           L {w} {h-r} Q {w} {h} {w-r} {h}
           L {r} {h} Q 0 {h} 0 {h-r}
           L 0 {r} Q 0 0 {r} 0 Z"/>
  <handle param="r" cx="{w-r}" cy="0" axis="x" invert="true"/>
</svg>
```

**Parameters** are declared with `<param>` elements. Each specifies a name, human-readable label, default value, min/max range, and whether it is a continuous float or a discrete integer. The properties panel reads these at runtime to generate the correct controls.

**Handles** are declared with `<handle>` elements. A handle has a canvas-space position expressed as template expressions, the name of the parameter it controls, the axis along which dragging acts, and whether the direction is inverted. Dragging the handle live-updates the parameter and immediately redraws the shape.

**Special renderers.** Shapes that cannot be expressed as static paths — the table grid and the connector line — carry a `data-renderer` attribute on the root `<svg>` element. The renderer dispatches to dedicated Rust functions for those cases while all other shapes go through the generic path evaluation pipeline.

---

## Running Standalone

```bash
cd Plugin_Diagrams
cargo run --example standalone
```

---

## Controls

| Action | Input |
|---|---|
| Pan | Middle-click drag |
| Zoom | Scroll wheel |
| Select element | Left-click |
| Multi-select | Shift + left-click |
| Rubber-band select | Drag on empty canvas |
| Move element | Select, then drag |
| Resize element | Drag one of the eight bounding-box handles |
| Adjust shape param | Drag the orange diamond handle |
| Delete selected | Delete or Backspace |
| Undo | Ctrl + Z (toolbar button) |
| Redo | Ctrl + Shift + Z (toolbar button) |

---

## Progress

### Foundation

- [x] WGPU dot-grid canvas background with pan/zoom shader
- [x] WgpuSurface integration via GPUI canvas overlay architecture
- [x] Pan with middle-click drag
- [x] Zoom to cursor with scroll wheel
- [x] Three-panel layout: palette, canvas, properties

### Shape System

- [x] SVG file format with `{expr}` template variables in path data
- [x] Expression evaluator supporting `+` `-` `*` `/` with correct precedence
- [x] Full SVG path parser: M L H V Q C A Z commands, absolute and relative
- [x] Built-in shapes: Rectangle, Ellipse, Diamond, Triangle, Parallelogram, Hexagon, Cylinder, Table, Text, Connector
- [x] Shape param handles (orange diamond handles, live-updating)
- [x] Special renderer for Table (dynamic grid from rows/cols params)
- [x] Special renderer for Connector (arrowhead, start/end arrow params)
- [ ] Runtime loading of custom shapes from a user directory
- [ ] Curved and orthogonal connector routing
- [ ] Connection port snapping for connectors

### Interaction

- [x] Drag to create new shapes (returns to Select on release)
- [x] Move selected elements
- [x] Resize with eight-handle bounding box
- [x] Rubber-band multi-selection
- [x] Delete selected elements
- [x] Hover highlight and connection port display
- [ ] Copy and paste
- [ ] Alignment and distribution guides
- [ ] Snap to grid

### History

- [x] Deadlock-free undo/redo (history owns document, commands take `&mut DiagramDocument`)
- [x] Add element command
- [x] Remove element command
- [x] Move/resize command
- [x] Shape param command
- [ ] Batch commands (e.g. multi-element move recorded as one undo step)

### Properties Panel

- [x] Fill and stroke colour display
- [x] Stroke width nudge controls
- [x] Opacity nudge controls
- [x] Position and size nudge controls
- [x] Shape-specific parameter controls driven from SVG param definitions
- [ ] Inline label text editing
- [ ] Colour picker popups

### Shape Palette

- [x] Uniform preview grid with live-rendered shape thumbnails
- [x] Active tool highlight
- [x] Connector button with arrow preview
- [ ] Drag-and-drop placement from palette

### Export and Persistence

- [ ] Save and load diagram (JSON serialization is wired in the element model)
- [ ] Export to SVG
- [ ] Export to PNG

---

## Dependencies

- **GPUI** — UI framework and rendering host (Far-Beyond-Pulsar fork)
- **wgpu** — GPU pipeline for the grid background shader
- **lyon** — Path tessellation (via the GPUI PathBuilder abstraction)
- **parking_lot** — RwLock for shared document state
- **uuid** — Element identifiers
- **serde / serde_json** — Element model serialization

---

## License

See the main Pulsar Engine license.
