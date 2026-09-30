# Large text fixes

`i-slint-renderer-software/` is the crates.io source of version 1.17.1, from
Slint commit `cf62c975c311e7036d599ed8ed0b7e6a8386a934`, directory
`internal/renderers/software`. Its original license notices and `LICENSES/`
are preserved. FindOut uses the GPL-3.0-only license option.

Only `lib.rs` differs from the published source. Cargo selects this copy via
`[patch.crates-io]`, and the Slint dependency is pinned to the compatible version.
The crate's generated lockfile, original workspace manifest and Cargo cache
metadata are omitted.

The question input scrolls a single line horizontally. A 12,681-byte transcript
can extend past the renderer's signed 16-bit device coordinates. The original
renderer narrows the item offset and glyph position separately, before they
cancel on screen, causing a panic. Full-line selections also narrow an unclipped
rectangle. Release builds abort on those panics.

The changes are confined to `sharedparley::GlyphRenderer`:

- `draw_glyph_run` keeps offsets as floating-point values and translated glyph
  bounds as 32-bit integers, then clips before narrowing to raster coordinates.
- `fill_rectangle` clips solid selection/cursor fills in floating-point
  coordinates before constructing raster arguments.

`parley/` is the crates.io source of Parley 0.10.0, commit
`1df9544bf0bd675d304001c0d0b35df2d220cd14`, directory `parley`. Its Apache/MIT
license files and notices are preserved. Only `src/layout/data.rs` differs:
cluster text offsets and their construction use `u32` instead of `u16`.
The original offsets wrap after 65,535 UTF-8 bytes in a shaping run, breaking
caret placement and navigation for a valid 50,000-character Unicode question.
For example, 50,000 four-byte emoji fit the query limit but exceed that byte
range. The wider offset preserves the existing shaping and cluster behavior.

No input is truncated. The existing text layout, scrolling and 50,000-character
query limit are retained.

`long_text_paste_ui` reproduces the original crash and checks transcript paste,
50,000-character ASCII/Unicode editing, selection, undo and submission. Run it
at scale factors 1 and 2, as in `.github/workflows/release.yml`. The other desktop
GUI checks cover answer selection, clipping and scrolling. Remove these overrides
only after replacement releases pass the glyph, selection and Unicode cases;
1.18.1 still panics on a full-line selection.
