# Printable templates — functional specification

This document specifies **what is drawn** for every printable template of the reference program (BoardCAD 3.0.1):
the plain design templates (outline, spin, profile, cross-sections), the one-page spec sheet, and the three wooden-board
template families (sandwich, hollow wooden board, chambered wooden board). Paper handling, page tiling and the
PDF/SVG/DXF writers are out of scope; the implementation already has them. Everything below is expressed in board
millimetres at 1:1 unless explicitly stated otherwise (the spec sheet is the only scaled output).

Where the reference program is defective the defect is described and the **intended** behaviour is specified; the
implementation must follow the intended behaviour. All defects are collected again in section 9.

---

## Contents

1. Conventions and the board interface
2. Shared constructions (rail line, core surfaces, validity, flattening, developed lengths, closed profiles, holes,
   grid, line styles, overlays)
3. Plain design templates
   3.1 Outline template (half / full, flat / over the curve)
   3.2 Spin template
   3.3 Profile (rocker) template
   3.4 Design cross-section templates
4. Spec sheet (one page, scaled)
5. Sandwich construction templates
   5.1 Core profile template · 5.2 Rail template · 5.3 Deck skin · 5.4 Bottom skin · 5.5 Core cross-section
6. Hollow wooden board templates
   6.1 Parameters · 6.2 Stations · 6.3 Stringer · 6.4 Ribs · 6.5 Rail strip · 6.6 Deck and bottom skins ·
   6.7 Tail piece · 6.8 Nose piece
7. Chambered wooden board (plank) templates
8. Labels printed on templates
9. Known defects of the original and intended behaviour
10. Points that could not be fully determined

---

## 1. Conventions and the board interface

### 1.1 Units and constants

- All template geometry is in millimetres. The board model stores centimetres; convert on the way in.
- `IN` = 25.4 mm, `FT` = 12 `IN` = 304.8 mm.
- Original defaults were given in centimetres; they are restated here in mm.

### 1.2 Board coordinate frame

- `x`: along the board, 0 at the tail, `L` (board length) at the nose.
- `y`: lateral, 0 on the stringer (centreline), positive towards one rail. All half-templates use `y ≥ 0`.
- `z`: vertical, up. Heights include rocker (the bottom centreline height at `x` is the rocker at `x`).

### 1.3 Template frames

Each template is drawn in its own 2D frame `(u, v)` in mm, with `v` up. The frame is stated per template. The original
drew on paper with the vertical axis pointing **down**, so its printed profiles, rails and sections appeared upside down
(deck towards the bottom of the sheet). This is harmless (templates can be flipped) and need not be reproduced; the
implementation draws with `v` up. An option to mirror a template is allowed but not required.

### 1.4 Neutral board interface

The templates use only these board queries:

| Symbol | Meaning |
|---|---|
| `L` | board length |
| `o(x)` | outline half-width at `x` (the outline curve evaluated as a function of `x`) |
| `T_o(x)` | unit tangent of the outline curve at `x`, oriented from tail to nose (from the curve's parametric derivative, so it is defined where the outline is vertical) |
| `D(x, y)` | deck height at `(x, y)`; `D(x, 0)` is the deck centreline |
| `B(x, y)` | bottom height at `(x, y)`; `B(x, 0)` is the rocker |
| `n_D(x, y)`, `n_B(x, y)` | surface normals of deck and bottom (see 2.2 for how they are used) |
| `S(x, s)` | point `(y, z)` on the half cross-section at `x`, `s ∈ [0, 1]` running from one centreline end of the section, round the rail, to the other centreline end |
| `X(x)` | full interpolated cross-section at `x` (both halves), in local section coordinates (no rocker) |
| `C_i`, `x_i` | the designer's cross-sections and their positions, `i = 0 … n−1`; `C_0` (tail) and `C_{n−1}` (nose) are the zero-size end sections. Each `C_i` is a half-section curve in local coordinates, running from the bottom centre round the rail to the deck centre |
| `A_B(x)`, `A_D(x)` | arc length of the bottom centreline curve (resp. deck centreline curve) from the tail to `x` |
| `apex(x)` | height (with rocker) of the rail apex at `x`: the point of the cross-section where, coming from the deck centre outwards, the outward surface normal reaches 88° from vertical |
| `R_max` | the largest rocker value (maximum of `B(x, 0)`) |
| `W_max`, `x_Wmax` | maximum width (`2·max o`) and its position |
| `W_c`, `T_c` | width and thickness at `x = L/2` ("centre width", "thickness") |
| fins | side fin rear point `(xr, yr)`, side fin front point `(xf, yf)`, centre fin rear `x`, centre fin front `x`, centre fin depth, side fin depth, splay; fin type text |
| guide points | the designer's guide points of the outline / deck / bottom / each cross-section |
| metadata | file name, designer, model, surfer, comments, volume |

`D(x, y)` and `B(x, y)` are **undefined** for `y > o(x)` (outside the outline) and for `x` outside `[0, L]`; any
sample that would need them there is invalid (2.3).

---

## 2. Shared constructions

### 2.1 Rail line at distance `d`

The rail line is the curve parallel to the outline, `d` millimetres **inside** it, used for every rail strip, skin and
the hollow-board pieces.

For an outline station `x₀ ∈ [0, L]`, let `P(x₀) = (x₀, o(x₀))` and `T = T_o(x₀) = (t_x, t_y)`. The inward unit normal
is `N = (t_y, −t_x)` (the tangent turned 90° clockwise; at the widest point it is `(0, −1)`). The rail-line point is

    R_d(x₀) = P(x₀) + d · N          (components R_x, R_y)

Rail lines are always parametrised by the **outline station `x₀`**, never by `R_x`.

> Defect: the original applied the normal with the opposite sign and so moved the point `d` **outside** the outline.
> The intended direction is inward, consistent with the rib construction (6.4), which steps inwards by
> `d / cos β`. See 9, D4.

**Station-plane point.** Where a feature must meet a plane `x = X` (ribs, 6.4/6.5), use the rail-line point whose
`R_x` equals `X`, found by solving `R_x(x₀) = X` for `x₀` (monotone away from the extreme tips; bisection on
`[X − d, X + d]` is sufficient). Its lateral coordinate is `y_R(X)`. To first order this equals the original's formula
`y_R(X) ≈ o(X) − d / cos β(X)`, where `β` is the angle of the outline to the board axis (`cos β = t_x`); either may be
used, but the same value must be used for a rib and for its matching slot.

**Developed length along the rail line.** For a sequence of rail-line points `R_d(x₀_k)` the strip coordinate is

    u_0 = 0,   u_k = u_{k−1} + |R_d(x₀_k) − R_d(x₀_{k−1})|     (plan distance in x–y)

i.e. the rail strip is developed along its plan curve. All features placed on one rail strip (outline, holes, slots)
must use one shared sampling of the rail line and one shared `u` origin (the first valid sample). The original
recomputed the sampling separately for each feature, so their `u` origins could differ by up to one sample (≈ 5 mm);
see D17.

### 2.2 Core (skin-compensated) surfaces

For a skin of thickness `t ≥ 0`, the core under the deck skin and above the bottom skin is

    D_t(x, y) = D(x, y) − t / cos α_D(x, y)
    B_t(x, y) = B(x, y) + t / cos α_B(x, y)

where `α_D` (resp. `α_B`) is the angle between the vertical and the deck (resp. bottom) surface normal at `(x, y)`,
taken in 3D (both the lengthwise and the crosswise slope count). Equivalently
`1 / cos α = sqrt(1 + (∂z/∂x)² + (∂z/∂y)²)` of that surface. This is the vertical offset that leaves a perpendicular
skin thickness `t` on a locally plane surface. The original estimated the slopes by finite differences with a 0.1 mm
offset (one-sided, stepping backwards in `x` and `y`, or forwards where that would leave the board).

With `t = 0`, `D_t = D` and `B_t = B`.

> Defect: the original used the **bottom** normal to compensate the deck and the **deck** normal to compensate the
> bottom. Intended: each surface uses its own normal (D5).

### 2.3 Validity of a sample

A sample at plan point `(x, y)` is **valid** when `y ≥ 0`, the point lies inside the outline, and the relevant top
height is not below the relevant bottom height (`D_t ≥ B_t` for core templates; `D > B` for raw profiles; the stated
reduced heights for holes). Unless a template says otherwise, a curve is drawn over the **first maximal run of
consecutive valid samples**: leading invalid samples are skipped, and the first invalid sample after a valid one ends
the run.

### 2.4 Flattening ("Flatten" option)

When flattening is on, every height `z` computed at a lengthwise position `x` is replaced by

    z' = z − (D(x, 0) − T_c)

i.e. the raw deck centreline is made a horizontal line at height `T_c` (the thickness at `L/2`), and every other height
at that `x` moves by the same amount. For rail templates `x` is the rail-line point's `R_x`. Purpose: build on a flat
deck-down jig.

### 2.5 Closed profile at a lateral offset

Used by the profile, stringer and plank templates. Inputs: lateral offset `y₀`, skin `t` (0 = raw surfaces), flatten
flag, start `x_a` and end `x_b`.

1. `n = floor((x_b − x_a) / 3 mm)`, `Δ = (x_b − x_a) / n`, samples `x_j = x_a + j·Δ`, `j = 0 … n`.
2. At each sample compute `top = D_t(x_j, y₀)`, `bot = B_t(x_j, y₀)` (raw `D`, `B` when `t = 0`), then flatten
   (2.4) if requested. A sample is valid when `bot < top`.
3. Top edge: polyline `(x_j, top_j)` over the first valid run (2.3), tail to nose.
4. Bottom edge: polyline `(x_j, bot_j)` over the same samples, nose to tail.
5. Close the polygon (this adds the two end edges, which are vertical when the run reaches `x_a`/`x_b`).

Frame: `u = x`, `v = z`. Cut line.

(Original quirk, harmless: the bottom edge collected every valid sample of the whole range, not only the first run.
For a profile with a single valid run the result is identical.)

### 2.6 Lightening holes in a profile or section ("webbing")

A hole of nominal span `s` starting at coordinate `a` (lengthwise `x` for stringers, lateral `y` for ribs, strip `u`
for rail strips), with web width `w`, is the region between the reduced top `top − w` and reduced bottom `bot + w`:

1. Sample `k = 0 … 19` at `a + k·s/20` (the original stops one twentieth short of `a + s`; keep this — it is harmless
   and reproduces the original shapes; chambers in section 7 use all 21 samples).
2. A sample is valid when `top − w ≥ bot + w`.
3. Take the first valid run (2.3); polygon = reduced top forwards, reduced bottom backwards, closed.
4. No valid sample → no hole.

Cut line.

### 2.7 Measuring grid

Optional background grid (guide layer), drawn from the template origin into the quadrant occupied by the geometry
(`u ≥ 0`, `v ≥ 0` in the template frame).

- Metric (user unit centimetres, millimetres or metres): one line every 10 mm. For a distance `c` to cover (in cm),
  the number of intervals is `N = floor(c) + 1`, then raised to the next multiple of 10 (if already a multiple of 10,
  add 10). Lines at `0 … N` cm in each direction, each line running the full extent of the other direction. Emphasis:
  every 100th line 1.5 pt and darkest, every 50th 1.3 pt, every 10th 1.1 pt, others 1.0 pt; base colour mid grey
  (128,128,128), darkened by a factor 0.7 per emphasis level (≈ 89, 62, 43).
- Imperial (inches, decimal inches): one line every inch; `N = floor(c / IN) + 1`, raised to the next multiple of 6
  (adding 6 when already a multiple). Every 12th line 1.5 pt (two levels darker), every 6th 1.2 pt (one level darker),
  others 1.0 pt.
- The coverage `c` per template is stated where the grid is used; the implementation may instead cover the bounding
  box of the template geometry (recommended, see D13).

> Defect: the original drew no grid at all when the user unit was metres. Intended: metres use the metric grid (D13).

### 2.8 Line styles

| Style | Used for | Nominal stroke |
|---|---|---|
| **cut** | every outline of a piece, holes, slots, tabs, notches | 2 pt (0.71 mm) |
| **guide** | grids, reference outlines drawn behind a piece, alignment marks, joint reference bands, fins and guide points, spec-sheet station lines | 1 pt (0.35 mm), or as stated |

Colours used by the original are noted per item; the implementation may map them to its own palette as long as cut and
guide stay distinguishable.

### 2.9 Overlays: guide points and fins

Two application-wide options (both default **off**) add overlays to the plain outline and profile/section templates:

- Guide points: each designer guide point drawn as a cross of half-size 4 pt (1.41 mm) at its position, guide style
  (original stroke width = the application's guide-point thickness setting, default 1.2 pt).
- Fins (outline templates only): side fin as the segment from its rear point `(xr, yr)` to its front point `(xf, yf)`,
  the same mirrored to `(xr, −yr)`–`(xf, −yf)`, and the centre fin as the segment from `(front x, 0)` to `(rear x, 0)`.
  Guide style.

---

## 3. Plain design templates

### 3.1 Outline template

Purpose: a 1:1 plan-shape template to trace the outline onto a blank.

Parameters: `grid` (default on), `over the curve` (default off), `mode` half / full (default **half**; see below),
guide-point and fin overlays (2.9).

**Flat outline.** Frame `u = x`, `v = y`.

- Cut: the outline curve `v = o(u)`, `u ∈ [0, L]`, drawn as the exact designer curve (the original rendered the Bézier
  segments directly; a polyline with sub-millimetre chord error is acceptable).
- Full mode: also its mirror `v = −o(u)`.
- Grid (2.7) covering `L` by half the centre width `W_c/2` (recommended: by `W_max/2`).
- Overlays per 2.9.

Half vs. full: the original drew both halves, but its page layout only covered the `v ≥ 0` side, so the printed result
was a half outline with the stringer line on the sheet edge. Half mode reproduces that; full mode draws both halves.

> Defect: the original drew the grid regardless of the grid option for the flat outline (D13). Honour the option.

**Over the curve.** The outline is laid out against the length measured along the rocker, for laying the template on
the curved bottom of a blank:

1. Samples `x = 0, 3, 6, … ≤ L` mm.
2. Points `(A_B(x), o(x))`.
3. The polyline starts at `(0, 0)` and then joins the samples in order (so the first edge is the tail edge from the
   centreline to the tail corner — harmless, keep it).
4. Full mode: also the mirror `(A_B(x), −o(x))`.
5. Grid if enabled, covering `L` by `W_max/2` (recommended: `A_B(L)` by `W_max/2`).
6. Overlays: the original drew fins and guide points at their flat `x`; intended: map each overlay `x` through `A_B`
   (D19).

Outline stroke colour in the original: the application's board colour (flat) or black (over the curve). Both are cut
lines.

### 3.2 Spin template

Purpose: a half-length template carrying the tail half of the outline on one edge and the nose half on the other, so
that one sheet traces the whole half-outline by turning it over about the midpoint.

Parameters: `grid` (default on), `over the curve` (default off).

**Flat.** Frame `u`, `v`; `M = L/2`, `h = o(M)`.

- Curve 1 (tail half): `(u, o(u))` for `u ∈ [0, M]`.
- Curve 2 (nose half, turned over): `(u, h − o(u + M))` for `u ∈ [0, M]`.

Both curves start on the left (`u = 0`) and meet at `(M, h)`. Both are cut lines. Grid covering `L` by `W_max/2`
(honour the option; the original ignored it, D13).

(The original drew both curves over their full length, shifted as above, and let the page boundary clip them; the
visible part extended slightly beyond `u = M`. Clipping to `[0, M]` is the intended result.)

**Over the curve.** As above with `u` measured along the rocker: curve 1 `(A_B(x), o(x))` for `x ∈ [0, M]`;
curve 2 `(A_B(x) − A_B(M), h − o(x))` for `x ∈ [M, L]` (sampled every 3 mm as in 3.1 "over the curve"; curve 1 keeps
its initial point on the centreline). They meet at `(A_B(M), h)`.

> Defect: the original's over-the-curve variant took `h` as the half-width at the position `x = A_B(M)` (an arc length
> used as a position) instead of `o(M)` (D12). It also drew the fin/guide overlays at unmapped positions; the spin
> template has no overlays.

### 3.3 Profile (rocker) template

Purpose: 1:1 side view of the board for the stringer/rocker.

Frame `u = x`, `v = z`.

- Cut: deck centreline curve `D(x, 0)` and bottom centreline curve `B(x, 0)` for `x ∈ [0, L]`, as exact curves. The
  original drew just the two curves (the end gaps at tail and nose are left open).
- Grid always on in the original, covering `L` by `R_max/2` (recommended: the profile's bounding box).
- Guide-point overlay (deck and bottom guide points) per 2.9.

### 3.4 Design cross-section templates

Purpose: one 1:1 template per designer cross-section.

- One template ("piece") for each interior section `C_i`, `i = 1 … n−2` (the zero-size end sections are skipped).
- Frame: `u = y` (lateral, ≥ 0), `v = z − z_min`, where `z_min` is the lowest point of the section curve (so the section
  sits on `v = 0`).
- Cut: the half-section curve `C_i` from the bottom centre round the rail to the deck centre, exact. The original drew
  only the half curve (open along the centreline); a full-width option may mirror it about `u = 0`.
- Grid (always on in the original) covering the section's full width (twice its maximum lateral extent) by its
  centre thickness.
- Guide-point overlay of that section per 2.9.
- Label: see section 8 (`Crossection<i> at <x_i>`).

---

## 4. Spec sheet (one page, scaled)

Purpose: a single-page summary of the design. It is **not** 1:1: all drawings share one scale

    s = (page printable width − 10 pt) / L

and are placed 5 pt from the left edge of the printable area. Text uses a 10 pt regular font except the title (30 pt).
`TS` = 12 `IN` (304.8 mm): the tail station is at `x = TS`, the centre station at `L/2`, the nose station at `L − TS`.

Top to bottom (vertical gaps in points are those of the original; "line" = the 10 pt font's line height):

1. Title "BoardCAD" (30 pt). (A re-implementation may print its own product name.)
2. "Spec sheet".
3. Two-column block, left column at the margin, right column at half the printable width, rows separated by
   line + 2 pt:
   - "Designer: <designer>" | "Date: <today as dd.MM.yyyy>"
   - "Surfer: <surfer>" | "Model: <model>"
   - "Measurements: <L> x <W_c> x <T_c>" (each with unit) | "Volume: <volume in the user's volume unit>"
   - "File: <file name>"
4. **Plan view.** Vertical space of `(W_c/2)·s` above the centreline and the same below (recommended: use `W_max`; the
   original's use of the centre width lets a wider outline overlap the text, harmless otherwise). Drawn:
   - the full outline (both halves), black, 1.5 pt;
   - the fins (2.9), black, 1.5 pt;
   - guide lines (grey, 1 pt): the stringer from tail to nose, and a full-width line across the outline at each of the
     three stations.
   - Width labels (grey) one line + 5 pt below the plan view, each centred on its station: width at `TS`, `W_c`, width at
     `L − TS` (numbers only, user unit).
5. **Side view.** Baseline (`z = 0`) placed `R_max·s + 10 pt` below the width labels; deck and bottom centreline curves
   drawn with `z` up, black, 1.5 pt. Grey 1 pt guide lines:
   - thickness lines at the tail and nose stations from `B(x,0)` to `B(x,0) + thickness(x)`;
   - the centre thickness line from `z = 0` to `z = T_c` (the original ignored the centre rocker here; intended: from
     `B(L/2, 0)` to `D(L/2, 0)`, D15);
   - rocker lines from the baseline up to `B(x, 0)` at `x = 0`, `TS`, `L − TS` and `L`.
   - Thickness labels (light grey) one line + 5 pt below the baseline, centred on the three stations.
   - Rocker labels (grey) a further line + 5 pt below: tail rocker left-aligned at `x = 0`, rocker at `TS` and at
     `L − TS` centred, nose rocker right-aligned at `x = L`.
6. **Cross-sections.** Each is the full interpolated section `X(x)` (both halves, `z` up, local coordinates without
   rocker), black, 1 pt, same scale `s`:
   - centre section `X(L/2)`, centred horizontally on the page, its base `T_c·s + 5 pt` below the rocker labels;
     label "Center" centred one line below;
   - one line + 10 pt lower: tail section `X(TS)` centred at one third and nose section `X(L − TS)` at two thirds of
     the printable width; labels "Tail" and "Nose" centred one line below each.
7. **Fins block** (only if a side fin or centre fin front position is non-zero, or a fin type is set), lines separated
   by line + 5 pt:
   - "Fins"
   - "Fin type: <type>" (if set)
   - "Center fin: Front:<front x>Rear:<rear x>Depth: <depth>" (if the centre fin front x is non-zero)
   - "Side fins: Front:<xf>Rear:<xr>Depth: <side depth>Toe-in<yf − yr>Splay:<splay>" (if the side fin front x is
     non-zero)
   - then an extra gap of line + 10 pt.
   Intended corrections (D15): show the block only when there is fin data (the original's emptiness test on the fin
   type was always true, so the block always appeared); print splay as an angle in degrees, not as a length; add the
   missing ": " after "Toe-in" and the missing spaces after "Front:", "Rear:", "Splay:".
8. "Comments:" followed by the comment text, word-wrapped to the printable width (split on newlines, then on spaces),
   line pitch line + 2 pt.

The original also contained two unused alternative layouts (a "detailed" one with width labels at `1 IN`, `1 FT`,
`2 FT`, the widest point, `L − 2 FT`, `L − 1 FT`, `L`, and a draft "new" layout). Only the layout above was ever
printed or exported; the alternatives are not required.

The "spec sheet to image file" command renders exactly this page into a bitmap (default 1200 × 1600 px); it is the same
content.

---

## 5. Sandwich construction templates

A sandwich board is a foam core with wooden deck and bottom skins and a wooden rail band. Defaults below are those of
the original dialogs (not remembered between uses).

| Parameter | Default | Used by |
|---|---|---|
| skin thickness `t` | 3 mm | core profile, rail |
| distance to rail `d` | 12.7 mm (½ in) | rail, deck skin, bottom skin |
| tail offset `e_t` | 20 mm | rail |
| nose offset `e_n` | 60 mm | rail |
| flatten | off | core profile, rail |

### 5.1 Core profile template

The core's stringer profile: closed profile (2.5) at `y₀ = 0` with skin `t`, flatten as chosen, from `x_a = 0` to
`x_b = L`. Cut. Grid on, covering `L` by `R_max` (guide). Title "… Profile".

### 5.2 Rail template

A developed vertical strip following the rail line, giving the core height at distance `d` inside the outline (used to
cut the rail band / to check the core edge).

1. Range of outline stations `[e_t, L − e_n]`; `n = floor((L − e_t − e_n) / 5 mm)`, step `Δ = (L − e_t − e_n)/n`,
   stations `x₀_j = e_t + j·Δ`, `j = 0 … n`.
2. At each station: rail-line point `R = R_d(x₀_j)` (2.1); heights `top = D_t(R)`, `bot = B_t(R)`; flatten (2.4) using
   `x = R_x` if requested. Valid when `R_y ≥ 0` and `top ≥ bot`.
3. First valid run (2.3). Developed coordinate `u` along the rail line (2.1), 0 at the first valid sample.
4. Closed polygon: top edge `(u, top)` forwards, bottom edge `(u, bot)` backwards, closed. Cut.
5. No grid.

(Original quirk, harmless: at each end the closing edge ran diagonally from the last top point to the bottom point one
sample further in, and on the forward-complete case the far end was evaluated one step past `L − e_n`. Intended: the
same samples on both edges.)

Title: intended "… Rail template" (the original printed "… Profile", D14).

### 5.3 Deck skin template

The developed half-width of the deck skin out to the rail line, as a function of developed length.

1. Outline stations `x₀ = 0, 10, 20, … < L` mm.
2. Rail-line point `R = R_d(x₀)` (no skin compensation). Valid while `R_y ≥ 0` (first run).
3. Lengthwise coordinate: `u = A_D(R_x) − A_D(R_x of the first valid sample)` (arc length along the deck centreline).
4. Crosswise coordinate: developed width of the deck surface along the station line from the stringer to the rail line:
   with `y_k = k·R_y/10`, `z_k = D(R_x, y_k)`, `k = 0 … 10`,
   `v = Σ_{k=1..10} sqrt((R_y/10)² + (z_k − z_{k−1})²)`.
5. Cut: the polyline `(u, v)` through all valid stations. The stringer edge `v = 0` is the reference edge (the
   original did not draw it; drawing it as a guide line is recommended). Half skin only (mirror for the other half).

> Defect: the original summed only the segments up to `0.9·R_y` and added one flat segment of `R_y/10`, i.e. it
> omitted the last, steepest tenth of the developed width (D7). Intended: the full sum above.

No grid. Title "… Deck skin template".

### 5.4 Bottom skin template

As 5.3 with the bottom surface: `u` from `A_B`, `z_k = B(R_x, y_k)`. Title "… Bottom skin template".

> Defect: the original's "bottom skin" command printed the deck skin template (D1). Intended: this template.

### 5.5 Core cross-section template

Present in the original but not reachable from its menus (position fixed at 30 mm); specified for completeness. At a
station `x`, with `d`, `t`:

1. Half-width to the rail line `h = o(x) − d / cos β(x)` (2.1).
2. Top edge: 151 samples `y_k = k·h/150`, `(y_k, D_t(x, y_k))`.
3. Bottom edge: back from `h` to 0, `(y_k, B_t(x, y_k))`. Close. Cut.
4. If `d > 0`: a vertical line at `y = h` from `D(x, h)` to `B(x, h)` (raw heights). Guide.
5. Reference: the raw half section `S(x, s)` for 301 samples `s = k/300`, closed along the centreline. Guide.

Frame `u = y`, `v = z` (with rocker).

---

## 6. Hollow wooden board templates

A hollow wooden board has a vertical centre **stringer**, vertical **ribs** across the board at whole-foot stations,
a vertical **rail strip** bent along the rail line, horizontal **tail and nose pieces** (plates) closing the ends,
and deck/bottom **skins**. Frame parts are cut from sheet stock of thickness `f`; all parts carry lightening holes
leaving webs of width `w`.

### 6.1 Parameters

Defaults (remembered between uses in the original):

| Parameter | Symbol | Default |
|---|---|---|
| distance from rail | `d` | 30 mm |
| skin thickness | `t` | 4 mm |
| frame thickness | `f` | 5 mm |
| webbing | `w` | 15 mm |
| tail offset | `e_t` | 35 mm |
| nose offset | `e_n` | 35 mm |

Derived: tail piece length `TPL = 9 IN − e_t`, occupying outline stations `[e_t, 9 IN]`; nose piece length
`NPL = 9 IN − e_n`, occupying `[L − 9 IN, L − e_n]`. Core surfaces `D_t`, `B_t` (2.2) are used throughout.

> Defect: the original's stringer command ignored the tail/nose offsets entered in its dialog and used stale values
> (initially 40 mm each), and it needed `d` without asking for it (initially 35 mm). Intended: the stringer uses the
> same six parameters as the other parts (D6).

### 6.2 Stations (shared by stringer, ribs and rail strip)

    N = floor((L − 9 IN) / FT),     stations X_k = k·FT,  k = 1 … N

Every station lies between the tail piece (ending at 9 IN) and the nose piece (starting at `L − 9 IN`). The rib
templates, the rail-strip rib slots and the whole-foot stringer holes must all use this list.

> Defect (known): the original's ribs used this count, but the rail strip used `floor(L/FT) − 1` stations, one fewer
> whenever `L mod FT ≥ 9 IN` (e.g. 6′10″: six ribs, five slots). It also placed each rail slot one sample (≈ 5 mm)
> short of the station. Intended: one station list, slots exactly at the station plane (D3).

### 6.3 Stringer template

Frame `u = x`, `v = z`.

1. **Outline (cut):** closed profile (2.5) at `y₀ = 0`, skin `t`, no flattening, from `x_a = e_t` to `x_b = L − e_n`.
   Grid on (guide), covering `L` by `R_max`.
2. **Lightening holes (cut):** `K = floor(L/FT)`; for `i = 0 … 2K − 1` a hole (2.6) of span `s = FT/2 − w` starting at
   `a_i = (i+1)·FT/2 − s/2` (so holes are centred on every half-foot mark `FT/2, FT, …, K·FT`), with `top = D_t(x, 0)`,
   `bot = B_t(x, 0)`. The holes centred on the rib stations `X_k` are where the ribs pass through the stringer (6.4
   step 1 gives the rib exactly the hole's height there); the half-foot holes are pure lightening. (Holes may also
   appear inside the tail/nose piece regions where the plate slot runs; the original allowed this. See section 10.)
3. **Tail piece joint (6.7 defines the plate line):** let `A = (x_A, z_A)` be the tail end and `F = (x_F, z_F)` the
   front end of the tail plate line, `α_t` its angle. Corner offsets for a band of thickness `f` about a segment are
   `±(f/2)·(−sin α, cos α)`.
   - Joint reference band (guide; red in the original): from `A` to `F`.
   - Stringer cut-out (cut; green in the original): from `x = x_F − 2 IN` to `x = x_F − TPL` along the plate line,
     i.e. between `(x_F − 2 IN, z_F − 2 IN·tan α_t)` and `(x_F − TPL, z_F − TPL·tan α_t)`.
     (The original used `TPL·sin α_t` for the second height, D10; use `tan`.)
   The stringer keeps its full height over the 2 IN next to the plate's front edge (that part enters the plate's slot,
   6.7) and is slotted from there to the tail end, so the plate slides on from behind.
4. **Nose piece joint:** with the nose plate line from its rear end `P = (x_P, z_P)` to its nose end `Q`, angle `α_n`:
   reference band from `P` to `Q` (guide); cut-out from `(x_P + 2 IN, z_P + 2 IN·tan α_n)` to
   `(x_P + NPL, z_P + NPL·tan α_n)` (cut).

Title: intended "… Stringer" (the original printed an untranslated placeholder, D14).

### 6.4 Rib templates

One template ("piece") per station `X_k` (6.2); in the original each rib was a separate print job. Frame `u = y`
(lateral), `v = z` (with rocker). Half rib by default; a full rib is the half plus its mirror about `u = 0`.

Per station `x = X_k`:

- `y_R` = lateral coordinate of the station-plane rail-line point (2.1) — the outer face of the rail strip.
- `y_B = y_R − f` — end of the rib body (inner face of the rail strip).
- Tab/slot heights (shared with the rail-strip slot, 6.5): at the station-plane rail-line point,
  `T = D_t − B_t`, `h = max(T/4, 5 mm)`, centre `z_m = (D_t + B_t)/2`; the tab spans `z_m ± h/2`.

Outline (cut), as one path:

1. Stringer pass-through: from `(0, D_t(x, f/2) − w)` to `(f/2, D_t(x, f/2) − w)`.
2. Deck edge: 151 samples `y_j = f/2 + j·(y_B − f/2)/150`, points `(y_j, D_t(x, y_j))`.
3. Tab: `(y_B, z_m + h/2)` → `(y_R, z_m + h/2)` → `(y_R, z_m − h/2)` → `(y_B, z_m − h/2)` → `(y_B, B_t(x, y_B))`.
4. Bottom edge: the same `y_j` back to `f/2`, points `(y_j, B_t(x, y_j))`.
5. `(f/2, B_t(x, f/2) + w)` → `(0, B_t(x, f/2) + w)`.

The path ends on the centreline (the mirror axis). For a half template the original left the centreline side open;
for a full rib the two halves join there. Between `y = −f/2` and `f/2` the rib is reduced to the stringer hole's height
so it passes through the stringer hole at that station.

> Defect: the original computed the tab height and position from the core thickness at `y_B`, and the matching rail
> slot from the thickness at the rail line, so tab and slot did not match exactly (D11). Intended: both from the
> station-plane rail-line point as above.

Lightening holes (cut): two holes (2.6) laterally, `top = D_t(x, y)`, `bot = B_t(x, y)`, span
`s = (y_R − 1.5·f − 3·w)/2`, starts `a_0 = f/2 + w`, `a_1 = f/2 + 2w + s`. (The far hole ends `w` short of the
body end `y_B`.) Skip a hole if `s ≤ 0`.

Reference (guide): the raw half cross-section at `x`: `S(x, s)` for 301 samples `s = k/300`, closed along the
centreline.

Label: "… Crossection" and "Position: <X_k with unit>".

### 6.5 Rail strip template

Frame `u` = developed length along the rail line at distance `d` (2.1), `v = z`. One shared sampling of the rail line
from outline station `e_t` to `L − e_n` (≈ 5 mm steps, as in 5.2) defines `u` for every feature below; positions
given as outline stations `x₀` are converted to `u` through it (interpolate between samples).

1. **Outline (cut):** the rail template of 5.2 with skin `t`, `d`, `e_t`, `e_n`, no flattening.
2. **Lightening holes (cut).** Divide the outline-station axis into sections, starting at `x₀ = e_t`:
   - section 0: from `e_t` to `FT` — solid;
   - then, for every foot from `FT` up to `K·FT` (`K = floor(L/FT)`), five sections: `w` solid, hole of
     `FT/2 − 1.5w`, `w` solid, hole of `FT/2 − 1.5w`, `w` solid;
   - then one final `w` solid section.
   This leaves a `2w` web centred on every whole-foot mark and a `w` web on every half-foot mark. Each hole section is
   sampled at ≈ 5 mm (`floor(span/5 mm)` equal steps, excluding its end); at each sample the rail-line heights are
   reduced, `top − w` and `bot + w`; the hole is the first valid run, closed polygon in `(u, v)`. Once a hole is
   pinched (a sample inside a hole becomes invalid after the hole has started), no further holes towards the nose are
   drawn. Rail-line samples that are themselves invalid (2.3) end the strip.
3. **Rib slots (cut):** at every station `X_k` (6.2): `u_k` = developed length at the station-plane rail-line point;
   rectangle `u ∈ [u_k − f/2, u_k + f/2]`, `v ∈ [z_m − h/2, z_m + h/2]` with `z_m`, `h` from 6.4. Only at stations
   where that rail-line point is valid (the original drew a degenerate slot at `u = 0` otherwise, D18).
4. **Tail piece slots (cut):** the tail plate's tabs (6.7) pass through the strip. For each tab section `[x₀_a, x₀_b]`
   of the tail piece, a band of thickness `f` about the segment from `(u(x₀_a), z_p(x₀_a))` to `(u(x₀_b), z_p(x₀_b))`,
   corner offsets `±(f/2)·(−sin α_t, cos α_t)`, where `z_p` is the tail plate line (6.7).
5. **Nose piece slots (cut):** likewise for the nose piece's tab sections with `α_n` and the nose plate line.

Title "… Rail".

### 6.6 Deck and bottom skins

Exactly 5.3 and 5.4 with `d` from 6.1. The tail and nose offsets are accepted by the original dialog but have no effect
(the skins run the full length); keep it so. The bottom-skin command printed the correct template here.

### 6.7 Tail piece

A horizontal plate of thickness `f` lying at mid-height inside the tail between the rail strips, tabbed through the
rail strip and interlocked with the stringer.

> Defect (known): the original's tail-piece and nose-piece commands printed nothing (their output routine was never
> connected; its menu labels for the two were also swapped). Intended: the template below (D2). The plate shape was
> reconstructed from the original's drawing routines, which existed but were never reached; deviations from those
> routines are flagged.

**Plate line.**

- Tail point: scanning outline stations from `e_t` towards the nose in steps of ≈ 5 mm (`floor(L/5 mm)` steps over
  `L`), the first station whose rail-line point `R_d` is valid (2.3). `x_A = R_x`, `z_A = (D_t(R) + B_t(R))/2`.
- Front point: the rail-line point for outline station `9 IN`; `x_F = R_x`, `z_F` = mid-height as above.
- `α_t = atan2(z_F − z_A, x_F − x_A)`. Plate line `z_p(x) = z_A + (x − x_A)·tan α_t` (for slots on the rail strip the
  original evaluated it at outline stations; either is acceptable).

**Plan frame.** `u = (R_x − x₁) / cos α_t` (true length along the inclined plate), `v = y`, where `x₁` is the `R_x`
of the first valid sample of the plate outline below (so the outline starts at `u = 0`).

**Sections** along outline stations from `e_t` to `9 IN`:

- if `TPL > 5 IN`: lengths `1 IN, 1 IN, TPL − 4 IN, 1 IN, 1 IN`;
- otherwise five equal sections of `TPL/5`.

  (Defect: the original's short-piece rule used `(k+1)/5·TPL` for section `k`, totalling three plate lengths, D8.)

Sections 0, 2, 4 are **body**, sections 1 and 3 are **tabs**. The body edge follows the rail line at `d + f` (inner
face of the rail strip); a tab edge follows the rail line at `d` (outer face), so each tab is exactly one strip
thickness long and passes through a strip slot (6.5 step 4).

> Decision (flagged, D21): the original's unreached routine used `d` for the body and `d − f` for the tabs, i.e. one
> frame thickness further out than the convention the printed ribs use (rib body to `d + f`, tab to `d`). The
> skins also end at `d`, so the rail strip must lie inside the rail line. Use the rib convention.

**Outline (cut), half plate:**

1. Sample each section at ≈ 5 mm (`floor(span/5 mm)` equal steps, excluding the section end).
2. Start at the first valid sample (validity of its rail-line point, 2.3): `(0, 0)` then `(0, y)` of that sample
   (this is the plate's tail edge).
3. Each valid sample adds `(u, y)` of its rail-line point at the section's distance.
4. At the end of sections 0–3, add a jog point: the last sample's outline station evaluated at the **next** section's
   distance (a short step between the body and tab lines).
5. After section 4, at its last point `(u_e, y_e)`: `(u_e, f/2)` → `(u_e − 2 IN / cos α_t, f/2)` →
   `(u_e − 2 IN / cos α_t, 0)`. This cuts a slot of half-width `f/2` and length 2 IN (in `x`) out of the plate's front
   edge, which receives the stringer's intact 2 IN (6.3 step 3). (The original used `2 IN` without the `cos` factor
   here but with it on the nose piece, D20.)
6. If a sample becomes invalid after the start, end the outline there by dropping to the centreline at the current
   `u`.

The path starts and ends on the centreline; the **full plate** is the path plus its mirror `v → −v` (recommended
default, since the plate is one piece).

**Rail-strip tail slots** (6.5 step 4) are the tab sections `[e_t + 1 IN, e_t + 2 IN]` and `[9 IN − 2 IN, 9 IN − 1 IN]`
(for the long-piece rule; for the short rule, sections 1 and 3 of the five equal sections).

**Lightening holes (cut; reconstructed from an unused routine):** two holes. Lengthwise (in `x`, converted to `u`):
leave `2w` after the tail end, then hole 1 of `s = (TPL − 5w)/2`, a `w` web, hole 2 of `s`, leaving `2w` before the
front. Laterally each hole spans from `v = w` to the rail line at distance `d + f + w` (inset `w` from the body edge),
sampled at ≈ 5 mm along its length, closed. Mirror for the full plate. Omit if `s ≤ 0`.

### 6.8 Nose piece

Mirror image of the tail piece at the nose:

- Rear point: rail-line point at outline station `L − 9 IN`; nose point: scanning outline stations back from
  `L − e_n` in ≈ 5 mm steps (`floor((L − e_t − e_n)/5 mm)` steps over that span) to the first valid rail-line point.
  `α_n = atan2(z_Q − z_P, x_Q − x_P)`, plate line through `P`.
- Plan frame `u = (R_x − x₁)/cos α_n`, `v = y`, with `x₁` the `R_x` of the first valid outline sample (rear edge).
- Sections from `L − 9 IN` towards the nose: `1 IN, 1 IN, NPL − 4 IN, 1 IN, 1 IN` (or five equal sections if
  `NPL ≤ 5 IN`); body/tab pattern and distances as for the tail piece.
- Outline: starts with the stringer slot at the **rear** edge: `(2 IN/cos α_n, 0)` → `(2 IN/cos α_n, f/2)` →
  `(0, f/2)` → `(0, y)` of the first valid sample; then the sections with jogs as in 6.7; at the end drop to
  `(u_e, 0)`. If a sample becomes invalid after the start, drop to the centreline at the current `u` and stop.
- Full plate by mirroring. Lightening holes as 6.7, measured from the rear end `L − 9 IN` (the original's unused
  routine started them at `L − 9 IN − e_n + 2w`, D9).
- Rail-strip nose slots (6.5 step 5) are the tab sections `[L − 9 IN + 1 IN, L − 9 IN + 2 IN]` and
  `[L − e_n − 2 IN, L − e_n − 1 IN]` (for the long-piece rule).

> Defect: the original's rail-strip nose slots measured the first section from the tail offset instead of from 0, so
> all nose slots were displaced towards the nose by `e_t` (D9). Intended: positions as above.

---

## 7. Chambered wooden board (plank) templates

The board is glued up from vertical planks parallel to the stringer, each hollowed (chambered) before glue-up. One
template per plank: the plank's side profile, its chambers, and alignment marks.

| Parameter | Default |
|---|---|
| draw grid | on |
| start offset from centre `y_s` | 0 |
| end offset from centre `y_e` | `W_max/2` (clamped to this if larger) |
| plank thickness `P` | 25.4 mm |
| deck/bottom thickness `t_s` (minimum wall left above/below chambers) | 8 mm |
| draw chambering | on |
| draw alignment marks | on |
| print multiple | off |

(Remembered between uses in the original.)

**Plank series.** Plank inner faces at `y₀ = y_s, y_s + P, y_s + 2P, …` while `y₀ < y_e`, at least one plank; with
"print multiple" off only the first. Each plank is one template ("piece").

> Defect: the confirmation message announced `floor((y_e − y_s)/P)` templates while the original printed
> `max(1, ceil((y_e − y_s)/P))`; and the templates did not show which plank they belong to (D16). Intended: announce the number
> actually produced and label each template with its plank offset `y₀`.

Per plank (frame `u = x`, `v = z`):

1. **Plank outline (cut):** closed profile (2.5) at `y₀`, raw surfaces (`t = 0`), no flattening, `x ∈ [0, L]`.
   This is the inner (larger) face of the plank. Grid (guide) if enabled, covering `L` by `R_max`.
2. If chambering is on:
   - **Outer face (guide, 1 pt, light grey 200,200,200):** closed profile at `y₀ + P`, raw, `x ∈ [0, L]`.
   - **Chambers (cut):** evaluated on the outer (smaller) face `y₁ = y₀ + P` with **vertical** offsets (no normal
     compensation): `top = D(x, y₁) − t_s`, `bot = B(x, y₁) + t_s`, valid when `bot ≤ top`. `K = floor(L/FT)`; for
     `i = 0 … 2K − 1` a chamber starting at `x = (i+1)·FT/2 + 10 mm` with span `FT/2 − 20 mm`, sampled at 21 points
     (20 equal steps, both ends included); first valid run; closed polygon (top forwards, bottom backwards). This
     leaves 20 mm bulkheads centred on every half-foot mark and no chamber in the first half foot. Chambers whose
     samples are all invalid are omitted.
3. If alignment marks are on: for `k = 1 … K − 1`, `x = k·FT`:
   - `top = D_t(x, y₀)`, `bot = B_t(x, y₀)` with `t = t_s` (normal-compensated, 2.2);
   - skip if `top < bot` or `top − bot < 10 mm`;
   - `z_a = apex(x)`; skip if `top − z_a < 5 mm` or `z_a − bot < 5 mm`;
   - draw a cross: `(x − 5 mm, z_a)`–`(x + 5 mm, z_a)` and `(x, z_a − 5 mm)`–`(x, z_a + 5 mm)`. Guide, 1 pt, red
     (255,20,20).
   The marks put the same reference height (the rail apex line) on every plank for aligning them at glue-up.

Title "… Profile" (recommended addition: "Plank at <y₀>").

---

## 8. Labels printed on templates

Every template carries a short header in the corner of its first sheet (10 pt; 6 pt on the sandwich templates). The
page-position lines ("Row: r/R", "Column: c/C") belong to the tiler. Intended header texts (user's length format):

| Template | Header line(s) |
|---|---|
| Outline | "Board file: <name> Outline" |
| Spin | "Board file: <name> Spin template" |
| Profile | "Board file: <name> Profile" |
| Design cross-section | "Board file: <name>", "Crossection <i> at <x_i>" |
| Sandwich core profile | "Board file: <name> Profile" |
| Sandwich rail | "Board file: <name> Rail template" |
| Deck skin (both families) | "Board file: <name> Deck skin template" |
| Bottom skin (both families) | "Board file: <name> Bottom skin template" |
| Sandwich core cross-section | "Board file: <name> Crossection", "Position: <x>" |
| HWS stringer | "Board file: <name> Stringer" |
| HWS rib | "Board file: <name> Crossection", "Position: <X_k>" |
| HWS rail strip | "Board file: <name> Rail" |
| HWS tail / nose piece | "Board file: <name> Tail piece" / "… Nose piece" |
| Plank | "Board file: <name> Profile", "Plank at <y₀>" |

Original texts and their defects (D14): the original concatenated "Board file" directly with the file name (no
separator, e.g. "Board fileMyBoard.brd Outline"); printed "Outline" on the spin template and "Profile" on the sandwich
rail template; printed an untranslated placeholder for the bottom-skin and stringer names; wrote "Crossection" and the
index with no space ("Crossection3 at 91.4"); and on the profile and plank templates labelled the column counter
"Board file" instead of "Column". Positions were printed with the unit only on the HWS rib and sandwich section labels.

---

## 9. Known defects of the original and intended behaviour

| # | Defect in the original | Intended behaviour |
|---|---|---|
| D1 | Sandwich "bottom skin" printed the deck skin template. | Print the bottom skin (5.4). |
| D2 | HWS tail piece and nose piece printed nothing (output never connected); the two menu entries were also cross-labelled. | Print the plates of 6.7/6.8 under the correct names. |
| D3 | Rail-strip rib slots used `floor(L/FT) − 1` stations while ribs used `floor((L − 9 IN)/FT)`; slots also sat ≈ 5 mm short of the station. | One station list (6.2); slot at the station-plane rail-line point. |
| D4 | The rail-line offset was applied outwards (outside the outline), so rail strips sampled outside the board (empty with any positive skin thickness) and skins came out roughly `d` wider than the outline instead of `d` narrower. Found by reading the geometry; should be confirmed against a reference run. | Offset inwards (2.1). |
| D5 | Skin compensation used the bottom normal for the deck and vice versa. | Each surface uses its own normal (2.2). |
| D6 | HWS stringer ignored its dialog's tail/nose offsets and used an unasked, stale distance from rail. | Same parameter set as the other HWS parts (6.1). |
| D7 | Skin developed width summed `R_y/10` flat plus the slope segments only up to `0.9·R_y`. | Full developed width (5.3 step 4). |
| D8 | Short tail/nose piece (≤ 5 IN) sections totalled three plate lengths. | Five equal sections. |
| D9 | Nose-piece rail slots measured from `e_t` instead of 0; nose-piece holes started `e_n` too early. | Positions per 6.8. |
| D10 | Stringer tail cut-out end height used `sin α` instead of `tan α`. | `tan α` (6.3). |
| D11 | Rib tab and rail slot sized at different points, so they did not match. | Both from the station-plane rail-line point (6.4). |
| D12 | Over-the-curve spin template looked up the half-width at an arc length used as a position. | Use `o(L/2)` (3.2). |
| D13 | Grid option ignored on the flat outline and spin templates; no grid in metre units; grid coverage too small for profile (half the rocker) and outline (centre width). | Honour the option; metric grid for metres; cover the geometry. |
| D14 | Header texts: missing separator, wrong template names, untranslated placeholders, "Column" shown as "Board file". | Section 8. |
| D15 | Spec sheet: fins block always shown; splay printed as a length; missing separators in the fin line; centre thickness line ignored rocker. | 4 step 5 and step 7. |
| D16 | Plank count announced wrongly; planks not labelled. | Section 7. |
| D17 | Each rail-strip feature resampled the rail line independently (origins could differ by one sample). | One shared sampling (2.1, 6.5). |
| D18 | A rib slot was drawn at `u = 0` for stations with no valid rail line. | Skip such stations. |
| D19 | Over-the-curve outline drew fins/guide points at their flat positions. | Map through `A_B` (3.1). |
| D20 | Tail-piece stringer slot length 2 IN in plate length, nose piece 2 IN/cos α. | `2 IN / cos α` on both (6.7). |
| D21 | Unreached tail/nose plate routine placed the rail strip one frame thickness outside the convention used by the ribs. | Rib convention: body to `d + f`, tabs to `d` (6.7). |

Odd but harmless behaviours that are kept or need not be reproduced: upside-down printing (1.3); the tail-edge segment
of the over-the-curve outline (3.1); the closed-profile bottom edge taking all valid samples (2.5); end diagonals of the
rail template (5.2); hole sampling stopping at 19/20 of the span (2.6); half-section templates left open at the
centreline (3.4, 6.4).

---

## 10. Points that could not be fully determined

- **D4 (rail-line direction)** was established by reading the curve-tangent conventions, not by running the original.
  If a reference run shows rail strips that are not empty, re-check; the intended (inward) behaviour stands either way
  for rail strips, ribs and hollow-board skins. For the *sandwich* skins an overhang (skin extending past the outline to
  wrap the rail) is a conceivable alternative reading of "distance to rail", and the original's output happened to be
  of that kind; this spec chooses inward for all templates so that skin and rail band meet on the same line.
- **Tail and nose pieces** (6.7, 6.8) were never printed by the original, so their exact intended look is reconstructed
  from its unreachable drawing routines plus consistency with the printed parts (D21). The lightening holes of the
  plates come from a routine that was not even used in the on-screen preview; treat them as recommended.
- The stringer's half-foot lightening holes nearest the tail and nose can overlap the plate cut-outs (6.3 step 2), and
  rail-strip holes can extend into the nose-piece region; the original did not prevent either. Clipping holes to
  `[9 IN, L − 9 IN]` would be a reasonable refinement but is not part of the reference behaviour.
- The stringer joint bands use rail-line `x` coordinates for the plate end points although they are drawn on the
  stringer (`y = 0`); the difference is a few millimetres at most and is kept.
- Colours and stroke widths are given as in the original; their mapping to the implementation's cut/guide layers is a
  presentation choice.
