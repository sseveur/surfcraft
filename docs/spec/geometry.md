# Geometry and Board Model — Behavioural Specification

Status: normative for the clean-room reimplementation.
Scope: the Bezier board representation of the original surfboard CAD program (version 3.0.1): curve primitives, the board data model, the two 3-D surface models, every user-visible metric, the board operations, the weight calculator and the length/weight/volume unit handling.
Out of scope (covered by other specifications): file formats (field codes, text layout), the NURBS/mesh board representations, CAM/tool-path generation, printing layout, rendering.

Conventions in this document

* "MUST" marks behaviour that results depend on. Where the original is odd but harmless the odd behaviour is specified and labelled **(compat)**. Where the original is wrong, the correct behaviour is specified and labelled **(fix)**; the original behaviour is described in §13 so that golden-value comparisons can be interpreted.
* All numerical constants, sample counts, tolerances and iteration limits given here are those of the original and MUST be used unchanged unless a section says otherwise. Results are expected to match the original to within the stated tolerances.
* Comparisons involving NaN are false (IEEE semantics); several algorithms below rely on this (noted where it matters).
* `TLO = 1e-10` and `THI = 1 − 1e-10` are used as "almost-endpoint" curve parameters throughout. They exist so that tangent directions are well defined even when a handle has zero length.
* `DEG = π/180`.

---

## Table of contents

1. Units and coordinate systems
2. Control points ("knots")
3. The cubic segment
4. The spline (chain of segments)
5. Generic numerical utilities
6. Board data model
7. Cross-section model
8. Surface models
9. Metrics shown to the user
10. Board operations
11. Weight calculator
12. Units, display formats and input parsing
13. Known defects of the original
14. Appendix: constant tables

---

## 1. Units and coordinate systems

### 1.1 Internal units

| Quantity | Internal unit |
|---|---|
| length | centimetre (cm) |
| area | cm² |
| volume | cm³ (displayed in litres = cm³/1000) |
| mass | kilogram |
| density | kg per litre |
| glass "unit weight" | kg per m² |
| angles | radians internally; degrees in user-facing angle parameters as stated |

Constants: 1 inch = 2.54 cm; 1 foot = 12 inch = 30.48 cm. Further constants in §14.

### 1.2 Board (3-D) frame

* `x`: along the stringer, from the tail (x = 0) to the nose (x = L, the board length). Always ≥ 0.
* `y`: lateral, from the stringer (y = 0) towards one rail. Only the half-board y ≥ 0 is modelled; the other half is the mirror image y → −y.
* `z`: vertical, up. The bottom curve after normalisation has its lowest point at z = 0 (§6.6).

A 3-D surface point is reported as (x, lateral, vertical).

### 1.3 Planar curves of the board

All three are splines in a 2-D plane whose first coordinate is the board x:

* **Outline**: second coordinate = half-width at x (≥ 0). The full width at x is twice the outline value. The outline runs from the tail centre (0, 0) to the nose tip (L, 0); intermediate knots carry the plan shape. The board length is defined from the outline (§6.4).
* **Bottom** (rocker): second coordinate = z of the bottom surface on the stringer.
* **Deck**: second coordinate = z of the deck surface on the stringer.

Thickness on the stringer at x = deck(x) − bottom(x).

### 1.4 Cross-section frame

A cross-section is a spline in a 2-D frame attached to a station x:

* first coordinate `u` = lateral distance from the stringer (≥ 0 for the modelled half),
* second coordinate `v` = height above the bottom-on-stringer at that station (the bottom centre is normally at (0, 0)).

The section spline runs **from the bottom centre (first knot, at u = 0) out round the rail and back to the deck centre (last knot, at u = 0, v = centre thickness)**. The handle of the first knot that points "backwards" and the handle of the last knot that points "forwards" are not part of any segment (they are mirror helpers for display and are otherwise ignored).

To place a section point in the board frame: (x, u, v + bottom(x)).

### 1.5 Angle conventions (used everywhere)

**Tangent angle of a curve.** For a parametric curve (X(t), Y(t)) the tangent angle is

    θ(t) = atan2( X'(t), Y'(t) )        (note the argument order: x-derivative first)

i.e. the angle of the direction of travel **measured from the +second-axis ("up") towards the +first-axis**. θ = 0 → travelling straight "up" (+Y); θ = +90° → travelling in +X; θ = −90° → travelling in −X; ±180° → travelling "down". Range (−180°, 180°] (standard atan2 range).

**Normal angle** = tangent angle + 90°, same reference (from "up" towards +first-axis). The unit vector for an angle α in this convention is (sin α, cos α).

On a cross-section (travelled bottom centre → rail → deck centre, counter-clockwise for a convex section):

| location | tangent angle | normal angle | outward normal direction |
|---|---|---|---|
| bottom centre (travelling outward, horizontal) | +90° | 180° | straight down |
| rail apex (travelling straight up, outermost point) | 0° | 90° | horizontal, outward |
| deck centre (travelling inward, horizontal) | −90° | 0° | straight up |

The tangent angle decreases monotonically along a convex section; the searches in §4.7 assume this.

**Rail feature angles** (normal angles on a cross-section):

* rail **apex**: normal angle 90° (tangent vertical, outermost point). (The cross-section view's apex line uses 88° instead; the profile view's apex line and all metrics use 90°.)
* **tuck-under point**: normal angle 175° (5° short of pointing straight down; on the bottom side of the rail).

**Knot handle angle.** For a handle (vector from the knot point to the handle point) the "handle angle" is the *unsigned* angle between the +second-axis unit vector (0, 1) and the handle vector, computed as arccos of the normalised dot product, in [0°, 180°]. If the arccos is NaN (zero-length handle or rounding outside [−1, 1]) the angle is 0. Being unsigned, it does not distinguish handles to the left and right of the vertical.

**Angle between handles** of a knot: the same unsigned arccos formula applied to the two handle vectors (NaN → 0).

---

## 2. Control points ("knots")

### 2.1 Data held by a knot

* `P` — the point on the curve.
* `Hp` — the "previous" handle point (absolute coordinates), controls the segment arriving at P.
* `Hn` — the "next" handle point (absolute coordinates), controls the segment leaving P.
* `continuous` — boolean flag: when set, interactive editing keeps the two handles collinear and opposite (G1).
* `aux` — an opaque boolean preserved from/to files; no geometric meaning.
* `axis factors` = (mx, my) — per-axis movement multipliers for point moves (1 = free, 0 = frozen). Default (1, 1).
* `locks_prev`, `locks_next` — sets of handle constraints (§2.4). Default empty.
* `partner` — optional link to another knot whose point must stay coincident with this knot's point (used to tie deck and bottom end points, §2.9).

Copying a knot's *values* onto another knot ("assign") copies P, Hp, Hn, continuous, aux, partner link and both lock sets, but **not** the axis factors. A full duplicate ("clone") copies everything including the axis factors.

Two knots are "identical" when all six coordinates are bit-equal and `continuous` and `aux` are equal.

### 2.2 Derived quantities

* handle lengths |Hp − P|, |Hn − P|;
* handle angles (§1.5) of Hp − P and Hn − P;
* angle between handles (§1.5).

### 2.3 Moving the point

"Move point to (x, y)":

1. d = ((x − Px)·mx, (y − Py)·my).
2. P, Hp and Hn are all translated by d (handles follow the point; handle locks are not re-applied).
3. If a partner is linked: partner.P := this.P (after the move) and partner.Hp, partner.Hn are translated by the same d.

So axis factors of (0, 0) makes the point immovable and (0, 1) allows vertical moves only. Multiple-selection moves and end-point drags always go through this operation.

"Set point raw" (used only by importers and loaders, before constraints are assigned) sets P := (x·mx, y·my) without moving the handles. **(compat)** — axis factors are (1, 1) at that time, so this is a plain assignment in practice.

### 2.4 Setting a handle; locks

"Set handle (prev or next) to (x, y)" assigns the handle and then applies that handle's lock set, each lock clamping one coordinate relative to the knot point:

| lock | effect after assignment |
|---|---|
| X-not-less | if H.x < P.x then H.x := P.x |
| X-not-more | if H.x > P.x then H.x := P.x |
| Y-not-less | if H.y < P.y then H.y := P.y |
| Y-not-more | if H.y > P.y then H.y := P.y |

(The original's lock names read the other way round; the table gives the *effect*, which is what matters.) Locks are evaluated in the order X-not-less, X-not-more, Y-not-less, Y-not-more.

Purpose: on the outline, deck and bottom the previous handle may not point forward (x) and the next handle may not point backward, which keeps every segment's x(t) non-decreasing at its ends and the curves usable as functions of x.

### 2.5 Scaling and rotating handles

* "Scale knot by (sx, sy)": every one of P, Hp, Hn is multiplied component-wise: (x·sx, y·sy). This is a scaling about the frame origin, not about P.
* "Scale handle by k": H := P + k·(H − P), then the handle's locks are applied.
* "Rotate handle to handle-angle a": let r = a − (current handle angle); rotate the handle vector counter-clockwise by r and assign through "set handle" (locks apply). The rotation is always counter-clockwise; because the measured angle is unsigned, the requested angle is reached only for handles lying to the left of the vertical. **(compat)** The only geometric user is the rocker-preserving scale (§10.1.2), which calls it as "rotate counter-clockwise by δ".

### 2.6 Continuity

**Flag semantics.** The flag affects editing only (§2.8); evaluation never enforces it.

**Load-time check** (run after every load/import, with "set if continuous" = false and "clear if not continuous" = true): for each knot of every curve and section,

    cont := | |π − angle(Hp − P)| − angle(Hn − P) | < 0.02 rad

using the unsigned handle angles of §1.5. If cont is false the flag is cleared; if true the flag is left as loaded. **(compat)** Because the angles are unsigned, a "V" whose two handles are mirror images about the vertical also counts as continuous; such a flag survives loading.

### 2.7 Knot similarity score (used by section matching, §8.2.2)

For knots A and B (B already translated as described in §8.2.2). If A and B are identical (§2.1) the score is 0. Otherwise accumulate:

1. +1.0 if their continuous flags differ.
2. dpos = |P_A − P_B| / 0.5; add 4.0 · dpos².
3. For each handle side (prev, then next): angle difference δ (degrees) = unsigned angle between the A handle vector and the B handle vector (arccos formula, NaN → 0); add 15.0 · (δ/5)².
4. For each handle side: Δℓ = | |H_A − P_A| − |H_B − P_B| |; add 0.3 · Δℓ².

Return the total **truncated towards zero to an integer** (the integer is then used as a number).

### 2.8 Interactive knot editing (behavioural)

Editing works on a snapshot taken when an edit gesture starts; every update re-applies "snapshot + offset", so movements do not accumulate rounding.

* **Hit test.** For each active curve, among all *complete* segments, candidates are tested per segment in this order: start knot's next handle, end knot's previous handle, start knot point, end knot point; the nearest wins with strict "<" (earlier candidate wins ties). The first knot's previous handle and the last knot's next handle are never candidates. A hit requires distance < 4 screen pixels. The grabbed element is the point, the prev handle or the next handle. **(fix)** When two curves are active (deck and bottom shown together) the grabbed element must be taken from the curve that produced the match (see §13).
* **Selection.** Click selects (clearing unless Shift/Ctrl; Ctrl toggles). Box-select adds every knot of the active curves that has *any* of its three points strictly inside the box; Ctrl toggles.
* **Drag.** Offset = (cursor − drag start), multiplied by 0.1 when Alt is held.
  * If more than one knot is selected, or the grabbed element is the point: every selected knot gets "move point to (snapshot P + offset)" (§2.3; axis factors and partner apply).
  * Otherwise (one knot, a handle grabbed): "set handle" to snapshot handle + offset (locks apply). If the knot is continuous, the *other* handle is then set (locks apply) to

        P − ℓ_other · u,   u = unit(new grabbed handle − snapshot P),  ℓ_other = |snapshot other handle − snapshot P|

    i.e. it is mirrored to point exactly opposite and **keeps its own original length**. If ℓ_other = 0 or the grabbed handle has zero length, the mirroring step is skipped.
* **Keyboard** (only when not dragging; one press = one step, holding repeats; the step count `k` grows by 1 per repeat and the offset is always applied to the snapshot, so motion is one step per repeat). Step size = 1 screen pixel in board units, ×0.1 with Alt. Directions honour view flips.
  * Arrow keys: move like a drag by k steps.
  * Lengthen / shorten the grabbed handle (one knot, handle grabbed): offset = ±k steps along the snapshot handle direction.
  * Rotate the grabbed handle: by ±k·1° (±k·0.1° with Alt) about P relative to the snapshot (the mirrored-handle rule applies).
  * Cycle the grabbed element among point/prev/next; select the next knot of the curve (wrapping).
  * Releasing the key commits the edit.
* **Continuity toggle.** Sets the flag on the first selected knot, then performs a zero-offset handle move on the grabbed handle (the previous handle if the point was grabbed). Turning continuity on therefore immediately re-mirrors the other handle.
* **Make horizontal / make vertical** (one knot). With P = (px, py), lengths ℓp, ℓn and signs
  σp = +1 if (Hp.x − px) > 0 else −1, σn = +1 if (Hn.x − px) ≥ 0 else −1 (for "vertical" use the y components):
  * Horizontal: if the point or the prev handle is grabbed: Hp := (px + ℓp·σp, py); if continuous, Hn := (px − ℓn·σp, py). Else (next handle grabbed): Hn := (px + ℓn·σn, py); if continuous, Hp := (px − ℓp·σn, py).
  * Vertical: if point or prev handle grabbed: Hp := (px, py + ℓp·σp); if continuous, Hn := (px, py − ℓn·σp). Then, if point or next handle grabbed: Hn := (px, py + ℓn·σn); if continuous, Hp := (px, py − ℓp·σn). (With the point grabbed both blocks run.)
  * All assignments go through "set handle" (locks apply). Signs are computed once, before any change.
* **Numeric entry** of the grabbed element's coordinates performs a drag with offset (typed − snapshot); all rules above apply.
* **Add point.** On the active curve (**(fix)** when deck and bottom are both active, only the curve nearest to the click; §13): the segment nearest to the click (§4.8) is split at the nearest parameter t (§3.8); the new knot is inserted after the segment's start knot; the neighbours' handles are shortened so the shape is unchanged. The new knot is not continuous and has no locks and axis factors (1, 1). **(compat)** (It receives locks only at the next load; see §13.)
* **Delete point(s).** Each selected knot is deleted from the curve that contains it. First and last knots of a curve are never deleted. For a deleted interior knot K between A and B: compute t = nearest parameter (§3.5.3, 32-sample variant) of K.P on the temporary segment (A.P, A.Hn, B.Hp, B.P) — i.e. using the *current* (short) handles. If t ≠ 0 and t ≠ 1: A.Hn := A.P + (A.Hn − A.P)/t and B.Hp := B.P + (B.Hp − B.P)/(1 − t) (exact inverse of the split shortening). Then remove K. Direct assignment, no locks.
* Every committed edit triggers the board update hook of the edited view (§6.6).

### 2.9 Constraint assignment (after every load/import)

If the outline has fewer than 2 knots nothing is assigned. Otherwise:

| curve | knot(s) | axis factors | prev-handle locks | next-handle locks |
|---|---|---|---|---|
| outline | first, last | (0, 0) | | |
| outline | every knot | | X-not-more | X-not-less |
| outline | first | | | + Y-not-less |
| outline | last | | + Y-not-less | |
| deck | first, last | (0, 1) | | |
| deck | every knot | | X-not-more | X-not-less |
| bottom | first, last | (0, 1) | | |
| bottom | every knot | | X-not-more | X-not-less |
| every section (including end sections) | first, last | (0, 0) | | |
| every section | first | | | X-not-less (replaces) |
| every section | last | | X-not-less (replaces) | |

(Interior section knots: no locks.)

Partner links: deck first ↔ bottom first, deck last ↔ bottom last (both directions). The deck-to-bottom link is established first, so on establishment the bottom end points are snapped onto the deck end points. When a link from master M to partner S is established the partner point is snapped to M.P. **(fix)** The partner's handles must be translated by (M.P − S.P_old) so that they keep their position relative to the partner point; the original translates by the opposite vector (only visible if the end points did not already coincide; §13).

---

## 3. The cubic segment

### 3.1 Definition

A segment between consecutive knots A and B uses the four control points

    P0 = A.P,  P1 = A.Hn,  P2 = B.Hp,  P3 = B.P

and is the cubic Bezier curve, evaluated in power form (Horner) per coordinate:

    a = P3 − 3·P2 + 3·P1 − P0
    b = 3·(P2 − 2·P1 + P0)
    c = 3·(P1 − P0)
    d = P0
    C(t) = ((a·t + b)·t + c)·t + d,   t ∈ [0, 1]

Coefficients may be cached and are invalidated when either knot changes. Evaluation outside [0, 1] is permitted (it just extrapolates the polynomial) and some searches below do so.

### 3.2 Derivatives, tangent angle, curvature

    C'(t)  = (3a·t + 2b)·t + c
    C''(t) = 6a·t + 2b
    θ(t)   = atan2(X'(t), Y'(t))                     (§1.5)
    κ(t)   = (X'·Y'' − Y'·X'') / (X'² + Y'²)^(3/2)    (signed; positive for counter-clockwise turning)

### 3.3 Arc length

Length over [t0, t1] (adaptive midpoint subdivision):

1. p0 = C(t0), p1 = C(t1), tm = t0 + (t1 − t0)/2, pm = C(tm).
2. poly = |p0 pm| + |pm p1|, chord = |p0 p1|.
3. If (poly − chord) > 0.001 **and** (t1 − t0) > 0.001: return length(t0, tm) + length(tm, t1).
4. Else return poly (the two-chord length, not the chord).

The full segment length is length(TLO, THI) and may be cached.

### 3.4 Parameter at a given arc length

t_for_length(target) = search(TLO, THI, target):

1. ts = t0 + (t1 − t0)/2; sl = length(t0, ts).
2. If |t0 − t1| < 1e-5: return t0.
3. If |sl − target| > 0.001: if sl > target return search(t0, ts, target), else return search(ts, t1, target − sl).
4. Else return ts.

### 3.5 Grid-refine searches

Several searches share this pattern; they MUST be reproduced exactly **(compat)**, including the peculiar termination test.

#### 3.5.1 Extremum of x or y — value or parameter

refine(t0, t1, n), maximising or minimising a coordinate f ∈ {X, Y}:

1. seg = (t1 − t0)/n. best = −1e7 for max (+1e7 for min), best_t = 0.
2. For i = 0, 1, …, n−1 (note: t1 itself is never sampled): t = t0 + seg·i (computed as seg·i + t0); skip if t < 0 or t > 1; v = f(t). For max accept if v ≥ best; for min accept if v ≤ best (ties → the later sample).
3. If (best_t − (t1 − t0)/2) < 0.0001 → return (signed difference; compares best_t with the *half-width*, not the midpoint).
4. Else if n ≤ 2 → return.
5. Else return refine(best_t − seg, best_t + seg, ⌊n/2⌋).

Initial call: refine(0, 1, 96). The value variant returns best; the parameter variant returns best_t. Consequences the implementer must preserve: when the coarse best lies at t < 0.5001 the result is the coarse 1/96-grid sample; otherwise it refines through n = 48, 24, 12, 6, 3, 1 (at n = 1 only the left bracket end is sampled and returned; this is a ~1e-6 offset).

The segment's min/max x/y = refine(0, 1, 96) of the corresponding coordinate.

#### 3.5.2 Parameter for x by subdivision (fallback of §3.6)

sub(x, t0, t1, n):

1. seg = (t1 − t0)/n; best_err = 1e9; best_t = 0.
2. For i = 1 … n−1 (both ends excluded): t = seg·i + t0; skip if t < 0 or t > 1; e = |x − X(t)|; accept if e < best_err (strict; ties → earlier).
3. If best_err < 0.003 → return best_t.
4. Else if |best_t − (t1 − t0)/2| < 0.0001 → return best_t.
5. Else if n ≤ 2 → return best_t.
6. Else return sub(x, best_t − seg, best_t + seg, ⌊n/2⌋).

Initial call sub(x, 0, 1, 96). Note: if no sample ever comes within 0.003 the recursion reaches n = 1, samples nothing and returns 0 (unless step 4 fires earlier, which happens for x slightly below the segment's range, returning a t ≈ 6e-6). **(compat)**

#### 3.5.3 Nearest parameter to a point

near(t0, t1, n) with distance to the point q:

1. seg = (t1 − t0)/n; best = 1e9; best_t = 0.
2. For i = 0 … n−1: t = seg·i + t0; skip if outside [0, 1]; dist = |C(t) − q|; accept if dist ≤ best (ties → later).
3. If (best_t − (t1 − t0)/2) < 0.001 → return best_t.
4. Else if n ≤ 2 → return best_t.
5. Else recurse near(best_t − seg, best_t + seg, ⌊n/2⌋).

Initial call near(0, 1, 32).

#### 3.5.4 Parameter at a given distance from a point (used only by machining helpers)

Same pattern as 3.5.3 (i = 0…n−1, strict "<" on |dist − D|, tolerance 0.0001 in the termination test, initial n = 96).

### 3.6 Parameter for a given x

t_for_x(x, t_start):

1. tn = t_start; e = x − X(tn); n = 0.
2. While |e| > 0.003 and (n++ < 30): tn := tn + e / X'(tn); e := x − X(tn). (The counter is incremented only when the first condition holds.)
3. If tn < 0, tn > 1, tn is NaN, n ≥ 30, or |e| > 0.003: tn := sub(x, 0, 1, 96) (§3.5.2).
4. Return tn.

Starting guesses used by callers:
* "y at x" on a segment: t_start = (x − P0.x)/(P3.x − P0.x) (may be ±∞/NaN if P3.x = P0.x; then step 2 does not execute because |NaN| > 0.003 is false, and step 3 falls back).
* "tangent at x" and "curvature at x" at segment level: t_start = (x − P3.x)/(P0.x − P3.x) **(compat; mirrored guess)**.
* "length up to x": t_start = THI.
* range-limited extremum: 0.1 for the range start, 0.9 for the range end.

y at x = Y(t_for_x(x, guess)).

### 3.7 Parameter for a given tangent angle

t_for_tangent(target, t_cur, t_last) (target is a tangent angle in radians; tolerance τ = 0.05° = 0.05·DEG; limit 50):

Phase 1 (iterative):
1. a_cur = θ(t_cur), a_last = θ(t_last), e = target − a_cur, n = 0.
2. While |e| > τ and (n++ < 50) and t_cur > TLO and t_cur < THI:
   slope = (a_cur − a_last)/(t_cur − t_last);
   t_last := t_cur; t_cur := t_cur + e·slope **(compat: multiplies by the slope rather than dividing — see §13)**;
   a_last := a_cur; a_cur := θ(t_cur); e := target − a_cur.

Phase 2 (bisection fallback), executed if |θ(t_cur) − target| > τ or t_cur < TLO or t_cur > THI:
3. n := 0; lo = 0; hi = 1.
4. While |e| > τ and (n++ < 50) and (hi − lo) > 1e-5 (e is carried over from phase 1):
   t_cur := lo + (hi − lo)/2; e := target − θ(t_cur); if e < 0 then lo := t_cur else hi := t_cur.
   (This assumes θ decreases with t, true for cross-sections.)
5. Return t_cur.

Note that if t_cur starts at exactly 0 or 1 phase 1 does nothing and phase 2 runs. The returned value is within τ of the target whenever a solution exists in the segment's monotone range; otherwise it is the bisection's end point.

### 3.8 Splitting, inserting and removing

**Split at t** (de Casteljau):

    q1 = P0 + t(P1 − P0),  q2 = P1 + t(P2 − P1),  q3 = P2 + t(P3 − P2)
    r2 = q1 + t(q2 − q1),  r3 = q2 + t(q3 − q2)
    r1 = r2 + t(r3 − r2)

New knot: P = r1, Hp = r2, Hn = r3 (continuous flag false). To insert without changing the shape the neighbours are updated: A.Hn := q1 = P0 + t(P1 − P0); B.Hp := q3 = P3 + (1 − t)(P2 − P3). These are direct assignments (no locks).

When the split position comes from a point q, t is the nearest parameter (§3.5.3) on the nearest segment (§4.8), and the same t is used for the handle shortening (it is recomputed on the original segment with the same procedure, giving the same value).

**Remove with compensation**: see "Delete point" in §2.8.

### 3.9 Other segment queries

* Tangent at x: θ(t_for_x(x, mirrored guess)).
* Curvature at x (segment level): κ(t_for_x(x, mirrored guess)).
* The obsolete "angle at t from split" helper and an unused tangent subdivision search exist in the original; they are not needed.

---

## 4. The spline (chain of segments)

### 4.1 Structure

A spline is an ordered list of knots K0 … Km; segment i joins Ki and Ki+1 (m segments). A spline may be empty, have a single knot (zero segments — used only for the end sections, §6.1) or more.

* insert(i, K): K becomes knot i; following knots shift.
* remove(i): knot i disappears; its neighbours become adjacent (no compensation — compensation is done by the editing command, §2.8).
* "assign spline": make the knot count equal (removing from the front or appending fresh knots), then assign knot values index by index (§2.1).

### 4.2 Segment lookup for x

* **Forward**: the first segment i with Ki.P.x ≤ x ≤ Ki+1.P.x. If none, the first segment whose numerically found x-range [min x, max x] (§3.5.1) contains x (either order). If none: not found.
* **Reverse**: the last segment (scanning from the end) whose end-point x's bracket x in either order. If none, the last segment (scanning from the end) with min x ≤ x ≤ max x. If none: not found.

### 4.3 Values at x

* value_at(x): forward lookup; not found → 0; else "y at x" (§3.6) on that segment.
* value_at_reverse(x): same with reverse lookup (used for the deck part of cross-sections).
* tangent_at(x): forward lookup; not found → 0; else segment tangent at x (§3.9).
* normal at x: angle = tangent_at(x) + 90°; unit vector (sin, cos).
* curvature_at(x): **(fix)** forward lookup; not found → 0; else κ(t) with t = t_for_x(x, mirrored guess) on that segment. (The original passes x itself as t; §13.)

### 4.4 Extremes

* max x: maximum over **all** segments of the segment max x.
* min y: minimum over all segments of the segment min y.
* max y and min x: **(fix)** over all segments. The original skips the last segment for these two (§13); in practice this only changes results when the extreme lies in the last segment or the spline has a single segment.
* x of max y: the segment with the largest segment max y (strict ">", first wins) over all segments; then t* = parameter variant of §3.5.1 on that segment; return X(t*).
* max y in [x0, x1] (machining helper): for each segment whose numeric x-range overlaps [x0, x1]: t0 = t_for_x(x0, 0.1), t1 = t_for_x(x1, 0.9), candidate = refine-max of y over [t0, t1] with n = 96; return the maximum (initial −100000).
* x of max y in [x0, x1]: the segment with the largest such candidate; **(fix)** return the x at the range-limited maximum (the original returns the x of that segment's unrestricted maximum; §13).

### 4.5 Length and arc-length parametrisation

* length: sum of segment lengths.
* length up to x: forward lookup (not found → 0); t = t_for_x(x, THI); result = length(TLO, t) on that segment + full lengths of all earlier segments.
* point at arc length ℓ: if ℓ ≤ 0 → C0(TLO) of the first segment; if ℓ ≥ total → C(THI) of the last segment; otherwise walk the segments subtracting full lengths until ℓ < current segment length, then t = t_for_length(ℓ) (§3.4) and return C(t).
* point at fraction s: point at arc length s·total.
* tangent at arc length ℓ: as above but without the end clamping. **(fix)** For ℓ ≥ total use the last segment at THI (the original evaluates at t = −1; §13).
* curvature at arc length ℓ: 0 if ℓ ≤ 0 or ℓ ≥ total; else κ(t_for_length(ℓ)) on the segment reached by the walk (walk stops when ℓ < segment length).
* fraction s at x: length up to x / total.

### 4.6 Segment-index parameter "g" and generic root finding

The global parameter g ∈ [0, 1] maps to segment i = ⌊g·m⌋ and local t = g·m − i; if g ≥ 1 then i = m − 1, t = THI.

* point_by_g(g) = C_i(t); normal_by_g(g) = θ_i(t) + 90°.
* g_for_x(x) = generic root (§5.1) of point_by_g(g).x = x on [0, 1].
* g_for_normal(α) = generic root of normal_by_g(g) = α on [0, 1].

### 4.7 Fraction s at a given normal angle — reverse search (core of all rail features)

Input: normal angle α (radians), flag `snap-to-corner` (true in every geometric use). Let T = α − 90° (target tangent angle). Output: s ∈ [TLO, THI].

1. length = 0; best_err = 1e4; best_t = −1; best_i = −1; found = false.
2. For i = m−1 down to 0 (segment i):
   a. a0 = θ_i(TLO), a1 = θ_i(THI).
   b. If a1 > T: (the target lies past this segment's end — a corner or beyond)
      * with snap-to-corner: set i := i + 1 (so the result is the knot at this segment's end);
      * without: length := length_i(TLO, THI − 0.05);
      found = true; stop the loop.
   c. t0 = clamp((T − a0)/(a1 − a0), 0, 1) (NaN stays NaN → clamping leaves NaN; see note); tl = t0 + 0.1; if tl > 1 then tl := tl − 0.2.
   d. t = t_for_tangent(T, t0, tl) (§3.7); err = |θ_i(t) − T|.
   e. If err < best_err: best_err = err, best_t = t, best_i = i.
   f. If err ≤ τ (0.05°): length := length_i(TLO, t); found = true; stop the loop.
3. If not found and best_i ≥ 0: **(fix)** length := length_{best_i}(TLO, best_t) + full lengths of segments 0 … best_i − 1, and skip step 4. (The original omits the preceding segments here; §13.)
4. Otherwise length := length + full lengths of segments 0 … i−1 (with i as left by the loop).
5. s = clamp(length / total length, TLO, THI).

Note on step 2c: if a1 = a0 the division yields ±∞ or NaN; clamping maps ±∞ to 1/0 and leaves NaN; with t_cur = NaN phase 1 of §3.7 does not iterate (comparisons false) and phase 2 runs because NaN fails "> TLO".

Forward variant (used only by machining cutter profiles): for i = 0 … m−1, with t0/tl computed the same way, t = t_for_tangent(T, t0, tl); if |θ_i(t) − T| < τ: length += length_i(TLO, t) and stop; else length += full length. s = length/total (no clamp).

### 4.8 Nearest point, split point, hit test

* nearest segment to q: for each segment, t = near(0, 1, 32) (§3.5.3); distance |C(t) − q|; the first segment with strictly smaller distance wins.
* split point near q: on the nearest segment at its t, the split knot (§3.8) and insertion index = segment index + 1.
* distance from q to spline: the minimum of those distances.

### 4.9 Scaling a spline

"Scale spline by (vertical v, horizontal h)": every knot is scaled by (h, v) (§2.5) — x by h, y by v. No locks, axis factors or partners are involved.

### 4.10 Integral of the spline as a function of x

∫ value_at(x) dx over [a, b] with Simpson's rule (§5.2) and the given panel count.

---

## 5. Generic numerical utilities

### 5.1 Generic root finder (target value on [lo, hi]; tolerance 0.001 on the function value; 50 iterations per method)

Secant phase:
1. fa = f(lo), fb = f(hi); x = clamp((target − fa)/(fb − fa), lo, hi). **(compat)** This is a fraction, not mapped into [lo, hi]; it is correct for [0, 1], which is every geometric use.
2. x_last = x + (hi − lo)/10; if x_last > hi then x_last = x − (hi − lo)/10.
3. v = f(x), v_last = f(x_last), e = target − v.
4. While |e| > 0.001 and (n++ < 50) and v ≠ v_last: d = ((x − x_last)/(v − v_last))·e; x_last := x; x := clamp(x + d, lo, hi); v_last := v; v := f(x); e := target − v.

If |f(x) − target| > 0.001, bisection phase:
5. lo' = lo, hi' = hi, e_lo = f(lo') − target, cur_e = 1e8, x = 0, n = 0.
6. While |cur_e| > 0.001 and (n++ < 50) and (hi' − lo') > 1e-4: x = (lo' + hi')/2; cur_e = f(x) − target; if cur_e·e_lo < 0 then hi' := x else (lo' := x; e_lo := cur_e).
7. Return x (even if still outside tolerance).

### 5.2 Simpson's rule (n panels)

h = (b − a)/n; position p starts at a and is advanced by repeated addition p := p + h. For each panel: f0 = f(p) (reused from the previous panel's right end), f1 = f((p + (p + h))/2), f2 = f(p + h); NaN values are replaced by 0; panel = ((p + h − p)/6)·(f0 + 4f1 + f2). Sum the panels.

### 5.3 Trapezoid rule on a parametric curve (n steps)

For a curve s ↦ (X(s), Y(s)) on [a, b]: s0 = a, then n times s := s + (b − a)/n: add ((Y_prev + Y_cur)/2)·|X_cur − X_prev|. (Area between the curve and the axis Y = 0, always counted positively in X.)

### 5.4 Polyline length (n steps, default 10)

Sum of |p_i − p_{i−1}| for p_i = curve(a + i·(b − a)/n), i = 1 … n. **(compat)** The original accumulates this sum in single precision (32-bit float); reproduce if bit-level agreement is wanted.

---

## 6. Board data model

### 6.1 Contents

* Outline, bottom and deck splines (§1.3).
* Guide points: three lists of 2-D points (outline, bottom, deck) in the corresponding curve frames, plus one list per cross-section in that section's frame. They are reference markers only; no operation moves or scales them.
* Cross-sections: an ordered list sorted by position (stable sort). Each has a position x, a section spline (§1.4) and its guide points.
* **End sections**: the first and the last list entries are placeholders at the tail and the nose: position 0 and position L, each with a single knot at (0, 0) with both handles at (0, 0). They are part of the native file data and are added by every importer of a foreign format (inserted at the front with position 0 and appended with position = board length). They are never evaluated (lookups clamp to the real sections) and only their positions matter. Real ("interior") sections are indices 1 … n−2; a board needs n ≥ 3 for any section-based computation (volume and centre of mass return 0 otherwise).
* Fins (§6.2), metadata (§6.3).
* Current-section index (initially 1; "next"/"previous" cycle within 1 … n−2 with wrap-around; after removing a section the index is decreased while it is ≥ n−1 and > 1).
* Surface-model selection: control-point matching (the program default) or section blend (§8).
* Cached centre of mass (0 = not computed).

### 6.2 Fins — the nine numbers

All in the outline plane (x from the tail, y = lateral offset from the stringer), in cm except where noted.

| index | meaning |
|---|---|
| 0 | side fin, rear mark, x |
| 1 | side fin, rear mark, y |
| 2 | side fin, front mark, x |
| 3 | side fin, front mark, y |
| 4 | centre fin, rear mark, x |
| 5 | centre fin, front mark, x |
| 6 | centre fin depth |
| 7 | side fin depth |
| 8 | side fin splay — **(fix)** an angle in degrees, dimensionless; the original parses and displays it as a length (§13) |

Side fins are drawn as the segment (fin0, fin1)–(fin2, fin3) and its mirror (y negated); the centre fin as (fin5, 0)–(fin4, 0). The printed sheet reports side-fin toe-in as fin3 − fin1 (a lateral distance). Fin information is printed only when fin2 ≠ 0, fin5 ≠ 0 or a fin-type string is present. A free-text fin type string is stored alongside.

### 6.3 Metadata

Free-text: name, author, designer, surfer, model, description, comments (multi-line), fin type, blank file, machine output folder (default "c:\machine"), aux1–aux3, file name. Version string (default "V4.4"). Numeric machining parameters (cut counts, cutter diameter, pivots, angles, margins, nose/tail lengths, x-steps, speeds, three-component strut/start/tail positions, security level, current unit code, stored "nose/tail rocker one foot" values, "show original" flag). None of these affect geometry; their encoding belongs to the file-format specification. The user-editable info dialog edits designer, surfer, model and comments.

### 6.4 Primitive board queries

* **Length L** = the largest x among the outline knots' *points* (handles ignored; not the curve's numerical maximum). 0 for an empty outline. A board is "empty" when the outline has no knots.
* **Length over curve** = arc length of the bottom spline.
* width(x) = 2·outline.value_at(x).
* rocker(x) = bottom.value_at(x); deck(x) = deck.value_at(x); thickness(x) = deck(x) − rocker(x).
* Max width = 2·outline max y (§4.4).
* Position of max width = outline "x of max y" (§4.4).
* Max thickness and its position: sample x_i = i/10 for i = 0 … ⌊10·L⌋ − 1 (every millimetre, L itself excluded); keep the first strict maximum of thickness(x_i).
* Centre width = width(L/2); "thickness" = thickness(L/2).
* Max rocker = bottom max y (§4.4).
* Distance from tail over the bottom curve at x = bottom length up to x; from nose = bottom length − that.
* x at a given over-curve distance from the tail = x of bottom point at that arc length.
* Max deck height within the first / last foot (machining): deck "max y in range" over [0, 30.48] / [L − 30.48, L] and the corresponding x.

### 6.5 Neighbouring sections of a station x

Let n be the number of sections (including end sections) and pos_k the positions.

* nearest(x): among k = 1 … n−2 the one with the smallest |pos_k − x|; ties keep the earlier index (strict comparison). (None if n < 3.)
* previous-position(x): k = nearest(x); if pos_k ≥ x then k := k − 1; return pos_k (may be an end section's position).
* next-position(x): k = nearest(x); if pos_k < x then k := k + 1; return pos_k (may be an end section's position).
* previous-section(x): k as for previous-position; if k = 0 then k := 1; **(fix)** if k > n−2 then k := n−2 (unreachable in practice).
* next-section(x): k as for next-position; if k = 0 then k := 1; if k > n−2 then k := n−2.

Thus near the tail both neighbouring shapes are the first real section and near the nose both are the last real section, while the blend fraction is still computed from the end sections' positions.

### 6.6 Invariants and update hooks

After an interactive edit of a curve the corresponding hook runs:

* **Outline edited**: invalidate centre of mass; re-fit sections (below).
* **Bottom or deck edited**: invalidate centre of mass; if the (off-by-default) rocker-stick option is on, apply §10.3; then **zero the rocker** (§10.2); then re-fit sections.
* **Section edited**: invalidate centre of mass; re-fit sections.

**Re-fit sections**: for every interior section k, scale it (§7.2) to (thickness(pos_k), width(pos_k)).

Invariants maintained by these hooks and by the constraints of §2.9:

* outline end points fixed; deck/bottom end points movable vertically only;
* deck and bottom share their first and last points (partner link);
* bottom minimum at z = 0 after any profile edit;
* every interior section's width equals the outline width at its position and its centre thickness equals the stringer thickness there;
* sections sorted by position; end sections first and last.

### 6.7 Section operations

* **Add section at p**: rejected unless 0 < p ≤ L **(fix: require 0 < p < L, see §13)**. New section = a copy of the interpolated section at p (§8.2.1), position p, scaled to (thickness(p), width(p)); appended and the list stably sorted.
* **Remove current section**: refused when n ≤ 3.
* **Move current section to p**: same validity check as add; set position, stable sort. **(fix)** Remember the previous position so that undo restores it; and re-fit the moved section to its new station (§6.6). (Original: undo sets position 0 and no re-fit happens until a later edit; §13.)
* **Copy / paste section**: paste replaces the current section's spline by a copy of the copied section's spline and scales it to (thickness, width) at the current section's position.
* Undo/redo of section and knot commands re-runs the edited view's hook.

### 6.8 Asymmetric boards (SurfCraft extension)

Not in the original. A board MAY carry a second outline, making it asymmetric; without it everything above
applies unchanged and results are bit-identical to a board that never had one.

* **Rails.** "Left" and "right" are the rider's, standing on the deck facing the nose. The board frame of §1.2 is
  right-handed (x to the nose, z up), so **y > 0 is the left rail** and y < 0 the right rail. In the Outline view
  (deck seen from above, nose to the right) the left rail is drawn above the stringer and the right rail below.
* **Data.** The outline of §1.3 is the **right** rail's half outline; the **left** outline is a second spline of
  the same kind (same knot rules, locks and fixed end points, so both have length L), with its own guide points.
  Stored in `.brd` under p61 (`file-formats.md` §3.11). A symmetric board's single outline describes both rails.
* **Surface.** Sections stay symmetric shapes. On each half the surface models of §8 use that rail's half width
  wherever they use the outline: the section-blend model scales each neighbouring section to (2 · half width of the
  rail, stringer thickness); the control-point model scales the interpolated section to the same; the fallback
  shape likewise. A point of the right half is computed on the y ≥ 0 side and mirrored (y → −y, n_y → −n_y). The
  stored sections are fitted (§6.6) to the full width (left + right half), i.e. to the average half width.
* **Metrics.** width(x) = left half width + right half width; the wide point is the maximum of that sum
  (sampled, then refined); section area = area of the left half + area of the right half, so volume, centre of
  mass and the volume distribution follow, and the volume equals the average of the volumes of the two
  symmetric boards made by mirroring each rail. Glass areas and the rail length of the weight calculator use the
  average of the two rails. Per-rail figures: maximum half width and its position.
* **Operations.** Making a board asymmetric copies the outline to the left rail. Making it symmetric keeps one
  rail's outline (and guide points) for both sides and re-fits the sections. Scaling scales both outlines by the
  same factors (the width factor is new maximum width / old maximum width). Flipping nose and tail mirrors both
  outlines along the length, each staying on its rail. Fin presets place each side fin from its own rail.
* **Points at (x, y).** deck/bottom height at (x, y) of an asymmetric board use the rail on the side of y and |y|.

---

## 7. Cross-section model

### 7.1 Derived quantities

* width = 2 · (section spline max x) (§4.4, all segments).
* centre thickness = (last knot).P.y − (first knot).P.y.
* bottom at lateral u = value_at(|u|) (forward lookup); deck at u = value_at_reverse(|u|); thickness at u = deck − bottom.
* point at fraction s = spline point at arc-length fraction s (§4.5) — s = 0 is the bottom centre, s = 1 the deck centre.

### 7.2 Scaling to a target thickness and width

scale_to(T_new, W_new):
1. W_old = width, T_old = centre thickness; each floored at 0.1.
2. kT = |T_new / T_old|, kW = |W_new / W_old|.
3. If T_old·kT ≤ 0.1 or W_old·kW ≤ 0.1: do nothing.
4. Scale the spline by (vertical kT, horizontal kW) (§4.9) — about the section origin.

### 7.3 Rail metrics of a section (shown in the cross-section view)

**Release angle.**
1. s_t = reverse search (§4.7) for normal 175°; q = point at s_t.
2. K = knot whose point or handle is nearest to q (hit-test order of §2.8). If none → 0.
3. If |K.P − q| > 0.9 cm → 0.
4. Result = angle between K's two handles (§1.5), displayed in degrees with one decimal (a smooth knot reads 180°).

**Tuck radius.**
1. s_a = reverse search for normal 90° (apex); s_t = reverse search for normal 175° (tuck).
2. If (u(s_a) − u(s_t)) < 0.18 cm → 0.
3. step = (s_t − s_a)/5; κ̄ = mean of curvature at arc length (s_a + i·step)·total for i = 0 … 4 (end excluded).
4. Result = 1/κ̄ (signed; positive for normal convex rails), displayed as a length.

Other section readouts: position (straight, or bottom length up to the position when "over curve" mode is on, suffixed "O.C"), width.

---

## 8. Surface models

Both models produce, for a station x and a parameter s ∈ [0, 1] along a sub-range of the section, a 3-D point (x, u, v + bottom(x)). The sub-range is chosen with two normal angles (degrees):

### 8.1 Section parameter range from angles

Given α_start and α_end and a (possibly scaled/interpolated) section spline:

* s_min = THI (deck centre) unless α_start > 0, in which case s_min = reverse search (§4.7) at α_start.
* s_max = TLO (bottom centre) unless α_end < 270, in which case s_max = reverse search at α_end.
* (control-point model only) clamp s_min ≤ THI, s_max ≥ TLO.
* s_cur = (s_max − s_min)·s + s_min.

Standard ranges:

| use | α_start | α_end | s = 0 | s = 1 |
|---|---|---|---|---|
| whole section | −360 or −90 | 360 | deck centre | bottom centre |
| deck | −90 | 90 | deck centre | rail apex |
| bottom | 90 | 270 or 360 | rail apex | bottom centre |
| feature line at angle α (apex 90, tuck 175, flow lines 10/27.5/45) | −45 | α | — | point where the normal is α |

### 8.2 Control-point matching model (program default)

#### 8.2.1 Interpolated section at station x

1. If there are fewer than 3 sections (no interior section), x < 0 or x > L: none.
2. k = nearest(x); if pos_k > x then k := k − 1; k2 = k + 1.
3. f = (x − pos_k)/(pos_k2 − pos_k); if infinite or NaN, f = 0.
4. If k < 1 then k := 1 (k2 unchanged). If k2 > n − 2 then k := n − 2 and k2 := k.
5. S = blend(section_k, section_k2, f) (§8.2.2). If the blend fails: none.
6. Scale S to (max(thickness(x), 0.5), max(width(x), 0.5)) (§7.2); set its position to x.

(Near the tail, k is forced to 1 while k2 = 1 as well, so the shape is the first real section; similarly at the nose.)

#### 8.2.2 Blending two sections A (this) and B (target) with fraction f

1. A' = copy of A; B' = copy of B scaled to (centre thickness of A, width of A) (§7.2).
2. If A' and B' have different knot counts, make them equal by splitting the one with fewer knots (call it F) using the one with more knots (call it R):
   a. sx = (F max x)/(R max x); sy = (F max y′)/(R max y′), where **max y′ is the maximum of y over all segments except the last** (this deliberate choice reproduces the original's matching numbers; the generic spline max-y of §4.4 is not used here).
   b. While R has more knots than F:
      * For each interior knot M_i of R (i = 1 … count−2): best_i = 1e7; for each interior knot O_j of F (j = 1 … count−2): take a clone of O_j, move its point to (O_j.P.x·sx, O_j.P.y·sy) carrying its handles (§2.3), score = similarity(M_i, clone) (§2.7); best_i = min(best_i, score).
      * worst = the first i with the largest best_i, using strict ">" against a running maximum that starts at 0. If no i qualifies (all scores 0) the blend fails.
      * Split F at the point nearest to M_worst.P (§4.8): insert the split knot at the returned index and shorten the neighbouring handles (§3.8) with t recomputed as the nearest parameter of M_worst.P on the original segment.
3. Result Q = copy of B' (after any splitting). For every knot index i and each of P, Hp, Hn: Q_i = A'_i + f·(B'_i − A'_i). Flags (continuity etc.) are those of B'.

Knots are paired strictly by index after the counts are equalised.

#### 8.2.3 Surface point

point(x, s, α_start, α_end):
1. Clamp x to [0.1, L − 0.1].
2. S = interpolated section at x; if none return (0, 0, 0).
3. s_cur from §8.1 on S; (u, v) = S point at fraction s_cur.
4. Return (x, u, v + bottom(x)).

#### 8.2.4 Surface normal

normal(x, s, α_start, α_end):
1. Clamp x to [0.1, L − 0.1], then to [1.0, L − 1.0].
2. S = interpolated section at x, S2 = interpolated section at xo = x + 0.1.
3. s_cur from §8.1 on S (S2 uses the same s_cur). so = s_cur + 0.01; flip = false; if so > 1 then so = s_cur − 0.01, flip = true.
4. p = S(s_cur), ps = S(so), px = S2(s_cur); r = bottom(x), ro = bottom(xo).
5. vc = normalise(0, p.u − ps.u, p.v − ps.v); vl = normalise(0.1, px.u − p.u, px.v − p.v + ro − r).
6. N = normalise(vl × vc); if flip, N := −N.

This yields the outward normal of the +y half (up on the deck, down on the bottom, outward at the rail).

#### 8.2.5 Section area at x (for volume)

1. S = interpolated section at x; none → 0.
2. g_r = g_for_normal(90°) on S (§4.6; secant + bisection, value tolerance 0.001 rad).
3. A_deck = trapezoid (§5.3) of point_by_g over [g_r, 1] with n steps; A_bot = trapezoid over [0, g_r] with n steps.
4. Area = max(0, 2·(A_deck − A_bot)).

n is the caller's split count (10 for volume and centre of mass, 8 for the volume-distribution graph).

### 8.3 Section-blend model ("S-blend", selectable)

#### 8.3.1 Surface point

point(x, s, α_start, α_end):
1. Clamp x to [0.1, L − 0.1].
2. C1 = previous-section(x), C2 = next-section(x) (§6.5).
3. W = width(x), T = thickness(x). For each Ck: wk = Ck width, tk = Ck thickness at lateral TLO (deck-at-TLO minus bottom-at-TLO, §7.1); scale a copy of Ck's spline by (vertical T/tk, horizontal W/wk) (§4.9).
4. On each scaled copy compute s_cur,k from §8.1 (independently).
5. qk = scaled copy k at fraction s_cur,k.
6. p1 = previous-position(x), p2 = next-position(x); d = (x − p1)/(p2 − p1).
7. (u, v) = (1 − d)·q1 + d·q2. Return (x, u, v + bottom(x)).

#### 8.3.2 Surface normal

As 8.2.4 but: x clamped to [0.1, L − 0.1] only; the offset points are taken on the two scaled copies at s_cur,k ± 0.01 and blended with the same d; the "−0.01 and flip" branch is taken if either s_cur,k + 0.01 > 1 **or** snap-to-corner is false; the third point pxo is a full surface point (§8.3.1, ranges recomputed there, x re-clamped) at x + 0.1; with p the surface point at (x, s) and ps the blended offset point (both including bottom(x) in z), vc = (0, p.u − ps.u, p.z − ps.z) and vl = (0.1, pxo.u − p.u, pxo.z − p.z) are *not* normalised before the cross product N = normalise(vl × vc); flip as in 8.2.4.

#### 8.3.3 Section area at x

A_deck = trapezoid over s ∈ [0, 1] (10 steps, always — the caller's split count is ignored) of (u, z) of point(x, s, −90, 90); A_bot likewise with (90, 360); area = max(0, 2·(A_deck − A_bot)). (z includes the rocker in both terms, which cancels.)

### 8.4 Deck and bottom height at (x, y)

* deck_point(x, y): s* = generic root (§5.1) of point(x, s, −90, 90).u = y on [0, 1]; return point(x, s*, −90, 90).
* bottom_point(x, y): same with (90, 270).
* Board-level deck height at (x, y): 0 if width(x)/2 < y; deck(x) if y = 0 exactly; else deck_point(x, y).z. Bottom likewise with bottom(x). Thickness at (x, y) = deck − bottom.

### 8.5 Board-level surface queries

* Surface point (x, s) uses the whole-section range (s = 0 deck centre, s = 1 bottom centre).
* Surface point/normal by (x, α_start, α_end, i, n) uses s = i/n.
* Section point at (x, s) without rocker = surface point minus bottom(x) in z.
* **Deck normal at (x, y)** and **bottom normal at (x, y)** — **(fix)**: the outward unit normal of the requested surface:
  1. Let F(x, y) be the deck height (resp. bottom height) of §8.4. h = 0.01.
  2. Use backward differences when x − h ≥ 0 and y − h ≥ 0, otherwise forward differences in that coordinate: gx = (F(x, y) − F(x − h, y))/h (or (F(x + h, y) − F(x, y))/h), gy likewise.
  3. Deck: N = normalise(−gx, −gy, 1). Bottom: N = normalise(gx, gy, −1).
  The original returns the bottom formula for the deck and vice versa, and its finite-difference vectors have inconsistent signs (§13).
* Tangent/normal helpers parameterised by (x, s) that build tangent vectors with x as their first component exist in the original but are unused; if provided, they must use the correct difference vectors (0 first component for the across-direction, the step for the along-direction).

### 8.6 Feature lines (displays)

Feature lines are polylines through surface points at s = 1 of the range (−45, α): starting at x = 0.01 and then, for each consecutive pair of section positions (including end sections; the first interval starts at 0), 10 equal steps (1000 for the outline view's tuck-under line). Apex α = 90° (profile view) / 88° (cross-section view); tuck-under α = 175°; flow lines α = 10°, 27.5°, 45°.

---

## 9. Metrics shown to the user

All metrics are recomputed after each change (the GUI debounces: 200 ms for the measurements, 500 ms for area and volume). "Foot" = 30.48 cm.

### 9.1 Specification panel

Two modes: "straight line" (default) and "over curve".

Stations (straight mode):

| name | x |
|---|---|
| centre | L/2 |
| 1 ft from nose | L − 30.48 |
| 1 ft from tail | 30.48 |
| 2 ft from nose | L − 60.96 |
| 2 ft from tail | 60.96 |
| nose (rocker only) | L − 0.005 |
| tail (rocker only) | 0.001 |

Over-curve mode replaces the station x's (except nose and tail rocker stations) with the x of the bottom-curve point at the corresponding arc length from the tail: centre at Lc/2, 1 ft from nose at Lc − 30.48, 1 ft from tail at 30.48, 2 ft at Lc − 60.96 and 60.96, where Lc = length over curve.

| displayed item | value |
|---|---|
| Length (straight) | L (large units) |
| Length over curve | Lc |
| Centre width | width(centre station) |
| Max width @ position | max width; position shown relative to the centre: straight: x_wmax − L/2; over curve: (bottom length up to x_wmax) − Lc/2 |
| Max thickness @ position | max thickness (§6.4); position relative to centre as above using x_tmax |
| Nose width / tail width | width at the 1-ft stations |
| Centre thickness | thickness(centre station) |
| Nose / tail thickness | thickness at the 1-ft stations |
| Nose rocker / tail rocker | rocker(L − 0.005) / rocker(0.001) |
| Rocker 1 ft from nose / tail | rocker at the 1-ft stations |
| Rocker 2 ft from nose / tail | rocker at the 2-ft stations |
| Area | planform area (§9.3) |
| Volume | §9.3 |

The window title shows L (large units) and the max width.

### 9.2 Cursor readouts ("sliding info")

* Outline view at cursor x: width(x); in over-curve mode also distance from tail and from nose over the bottom curve; optionally the moment of inertia (§9.3) about the cursor point.
* Profile view at cursor x: thickness(x), rocker(x), radius = 1/(curvature of the first active curve at x) **(fix: curvature at x as in §4.3)**, and over-curve distances.
* Cross-section view at cursor lateral u on the current section: thickness at |u| and its percentage of the centre thickness (thickness at TLO), integer-truncated, printed with two digits; bottom at |u|; distance from rail = width/2 − |u|; distance from centre = |u|.

### 9.3 Integrated quantities

* **Planform area** = Simpson (§5.2) of width(x) over [TLO, L − TLO] with 10 panels. Displayed in m² (÷10000) or ft² (×0.00107639104), 3 decimals.
* **Volume** = 0 if fewer than 3 sections; else Simpson of section area A(x) (§8.2.5 or §8.3.3 with split count 10) over [0.01, L − 0.01] with 30 panels. Displayed in litres, 3 decimals.
* **Centre of mass** (x only): 0 if fewer than 3 sections; returns the cached value if non-zero. Otherwise a = 0.01, b = L − 0.01, h = (b − a)/10; for 10 panels: I_k = (h/6)(A(p) + 4A(p + h/2) + A(p + h)) with NaN → 0 and A computed with split count 10; moment += (p + h/2)·I_k; mass += I_k; result = moment/mass, cached. **(fix)** The cache must also be cleared by scaling (§10.1) (the original only clears it on curve/section edits).
* **Moment of inertia** about a point (x0, y0) in the outline plane (kg·m²): 0 if fewer than 3 sections; a = 0.01, b = L − 0.01, h = (b − a)/30; for 30 slices starting at p = a: V = Simpson of A(x) (split 10) over [p, p + h] with **1** panel, ÷1000 (litres); r = √((p + h/2 − x0)² + y0²)/100 (m); I += V·0.1·r² (density 3 kg/30 l = 0.1 kg/l). Displayed in kg·m² or lb·ft² (×2.20462262 × (100/30.48)²).

### 9.4 Volume distribution graph

Section area A(x) with split count 8 at x = i·L/20, i = 0 … 20, drawn as a polyline.

### 9.5 Printed specification sheet

Widths at x = 2.54, 30.48, 60.96, x_wmax, L − 60.96, L − 30.48, L; thickness at 30.48, L/2, L − 30.48; rocker at 0, L, 30.48, L − 30.48; "measurements" = L × centre width × centre thickness; volume; fin data (§6.2).

---

## 10. Board operations

### 10.1 Scaling

#### 10.1.1 Plain scale to (L', W', T')

1. kL = L'/L, kW = W'/(max width), kT = T'/(max thickness) (all from §6.4, computed before any change).
2. Outline: x·kL, y·kW. Deck: x·kL, y·kT. Bottom: x·kL, y·kT (the rocker is scaled with the thickness).
3. Interior section positions ·kL; the nose end section's position := L'. (Tail end section stays at 0.)
4. Re-fit sections (§6.6).

Not scaled: guide points, fins (separately, §10.1.5). Knots are scaled directly (no axis factors/locks/partners), so deck and bottom ends stay coincident.

#### 10.1.2 Rocker-preserving scale ("scale bottom rocker" option) to (L', W', T')

1. kL, kW, kT as above; ΔT = T' − (max thickness); x_m = position of max thickness (old board).
2. For each interior deck knot i (not first/last): xi = clamp(P.x, TLO, L − TLO); record ti = P.y − rocker(xi).
3. Outline: x·kL, y·kW. Deck: x·kL, y unchanged. Bottom: x·kL, y·kL (rocker scales with length).
4. φ = atan2(ΔT, x_m).
5. For each interior deck knot i: xi = clamp(P.x, TLO, L_new − TLO); dy = ti·kT − (P.y − rocker(xi)); move the point by (0, dy) (§2.3); rotate the next handle and then the previous handle counter-clockwise by φ·(x_m − xi)/x_m (§2.5; locks apply after each; x_m is the *old* position).
6. Section positions and re-fit as in 10.1.1.

**(fix)** After step 3 the deck's first and last knots must be moved (with their handles) to coincide with the bottom's first and last points (the original leaves them at the old height while the bottom ends are scaled, opening/closing the nose and tail; §13).

#### 10.1.3 Scale command (measurements tab)

Inputs: length, max width, max thickness (pre-filled with L, max width, max thickness), "straight line" or "over curve" for length, "scale fins" (default on), "scale bottom rocker" (default on), "constrain proportions" (default off). With constrain proportions, leaving one field rescales the other two proportionally to the values present when the field gained focus. Any non-positive value is rejected.

* Straight: apply 10.1.2 if "scale bottom rocker" else 10.1.1, with (L', W', T'). If "scale fins": fin scaling with (L'/L_old, W'/W_old).
* Over curve (L' is the desired length over curve): repeat 5 times: guess = L·(L'/Lc) + 0.01 (L and Lc of the current, already partially scaled board); apply 10.1.2 or 10.1.1 with (guess, W', T'). Fin scaling with (guess/L_old, W'/W_old). The resulting straight length is recorded for redo.

#### 10.1.4 Scale by factor

L' = L·k, W' = max width·k, T' = max thickness·k; k ≤ 0 rejected. Apply plain scale 10.1.1. Fins scaled by (k, k) if the factor-tab "scale fins" is on. **(fix)** Undo/redo must use the same variant (plain); the original uses the rocker-preserving variant for undo/redo of a factor scale (§13).

Undo of a scale applies the same variant with the old (L, max width, max thickness) and inverse fin ratios; it is therefore an approximate, not exact, inverse **(compat)**.

#### 10.1.5 Fin scaling (rx, ry)

fin0, fin2, fin4, fin5 ·rx; fin1, fin3 ·ry; depths (6, 7) and splay (8) unchanged.

#### 10.1.6 Scale ghost board

Scales the reference ("ghost") board with plain scale to (current L, current centre width, current centre thickness). **(compat/quirk)** The targets are centre values while plain scale normalises by the ghost's *max* values; the ghost therefore ends with max width = current centre width. Keep or correct to max values — the original behaviour is as described.

### 10.2 Put lowest rocker point at zero

m = bottom min y (§4.4, all segments). Subtract m from the y of P, Hp and Hn of every bottom knot and every deck knot (direct, no axis factors). Guide points are not shifted. Runs automatically after every profile edit.

### 10.3 Rocker-stick adjustment (setting, default off)

Purpose: rotate the profile so that the bottom's tangent at its arc-length midpoint is horizontal, then re-align the tail to x = 0 and stretch x so the rocker length equals the outline length.

**(fix)** Correct behaviour: θc = tangent angle of the bottom at arc-length fraction 0.5 (§4.5); rotate all bottom and deck knot coordinates about the origin counter-clockwise by ψ = θc − 90° (so the midpoint tangent becomes +x); subtract the bottom's first knot x from all bottom and deck x's; scale bottom and deck x by L / (bottom max x). The original's "rotation" uses the matrix [[sin β, cos β], [cos β, sin β]] with β = π − θc, which is not a rotation (§13). Afterwards §10.2 runs.

### 10.4 Flip nose/tail

A display operation only: mirrors the outline, profile and section views horizontally and re-fits the views. No geometry changes.

### 10.5 New board

"New" offers the built-in templates (shortboard, funboard, longboard, scan template) and loads the chosen one exactly as stored; there is **no** "template at target dimensions" in the original — the user scales afterwards (§10.1). Template contents are data for the file-format specification. After loading: continuity check (§2.6) and constraints (§2.9) as for any load; the undo history is cleared.

### 10.6 Other

Background-image alignment, ghost/original boards and view fitting are GUI matters outside this specification.

---

## 11. Weight calculator

### 11.1 Inputs and defaults

| input | unit | default |
|---|---|---|
| stringer width | cm | see 11.2 |
| stringer density | kg/l | 0.4 |
| foam density | kg/l | 0.045 |
| deck glass unit weight | kg/m² | see 11.2 |
| deck lap width | cm | 2.54 |
| bottom glass unit weight | kg/m² | see 11.2 |
| bottom lap width | cm | 2.54 |
| resin ratio (resin mass / glass mass) | — | 1.5 |
| hot-coat mass | kg | 0.03 · (L / 30.48) |
| plugs and fins mass | kg | 0.2 |

### 11.2 Length-dependent defaults — **(fix)**

* L > 9 ft (274.32 cm): stringer 0.8 cm, deck glass 0.400, bottom glass 0.200.
* else L > 7 ft (213.36 cm): stringer 0.5 cm, deck glass 0.400, bottom glass 0.200.
* else: stringer 0.3 cm, deck glass 0.270, bottom glass 0.135.

(The original tests the 7-ft condition independently after the 9-ft one, so boards over 9 ft get 0.5 cm; §13.)

### 11.3 Formulas

* Stringer area = Simpson(deck(x), [0, L], 20 panels) − Simpson(bottom(x), [0, L], 20 panels) (cm²); stringer volume = area · stringer width (cm³).
* Stringer mass = (volume/1000) · stringer density.
* Foam volume = board volume (§9.3) − stringer volume; foam mass = (foam volume/1000) · foam density.
* Deck glass area = 2 · Simpson over x ∈ [TLO, L − TLO] with 10 panels of g_deck(x), where g_deck(x) = polyline length (§5.4, 10 steps, s ∈ [0, 1]) of (u, z) of the surface point with range (−90, 90) (deck centre → apex) using the application's selected surface model.
* Bottom glass area: same with range (90, 360) (apex → bottom centre).
* Glass mass = (area/10000) · unit weight, for deck and bottom.
* Lap area = (outline arc length) · lap width · 2 (deck and bottom each); lap mass uses the deck (resp. bottom) unit weight.
* Total glass = deck glass + deck lap + bottom glass + bottom lap.
* Resin = total glass · resin ratio.
* Total = stringer + foam + total glass + resin + hot-coat + plugs-and-fins.

**(fix)** All dependent values (including the total) must be recomputed whenever any input changes; the original refreshes the total only on its "calculate" button and on reopening.

Displays: volumes in litres; areas in m²/ft²; masses per §12.3 ("small units" for the input unit weights, hot-coat and plugs: grams/ounces below 1 kg). The glass unit weight is displayed with a mass unit although it is per m² **(compat)**.

---

## 12. Units, display formats and input parsing

### 12.1 Offered units (selection order in the GUI)

1. feet-inches with fractions (default),
2. decimal inches,
3. millimetres,
4. centimetres,
5. metres.

Fraction accuracy (denominator) is a setting, default 16. Decimal-inch decimals: 2.

### 12.2 Length display (value v in cm; "large units" flag as stated per use)

* **Feet-inches with fractions**: sign "-" if v < 0, then use |v|. I = ⌊|v|/2.54⌋ (integer truncation). If large units and I > 36: F = ⌊I/12⌋, I := I mod 12, emit `F'`. If I ≥ 1: emit `I` (space-separated from preceding text). frac = ⌊(|v|/2.54 − (I + 12F))·D⌋ with D = accuracy; if frac > 0: while frac is even, halve frac and D; emit `frac/D` (space-separated). If nothing was emitted, or only "-", or the text ends with `'`: append `0`. Append `"`. Examples (D = 16): 190.5 (75 in) with large units → `6' 3"`; 182.88 with large units → `6'0"` (no space before the appended 0) provided the quotient 182.88/2.54 evaluates to ≥ 72 in double precision; 5.08 → `2"`; 0.3 → `1/16"`; −5.08 → `- 2"` (a space follows the minus sign whenever more text follows it). Values that are exact multiples of an inch or of 1/D inch can land on either side of the truncation because 2.54 is not exactly representable; reproduce the double-precision division and truncation literally.
* **Decimal inches**: sign; i = |v|/2.54 computed in single precision; if large units and i > 36: F = ⌊i/12⌋, i := i − 12F, emit `F'`; if i ≥ 0.01 emit i with 2 decimals, appended directly (no space, e.g. `6'3.00"`); if nothing or only "-" emitted append `0`; append `"`. **(compat)** A whole number of feet with large units prints as `F'"`.
* **Centimetres**: if large units and v > 100: `%.3f m` of v/100; else `%.2f cm`.
* **Metres**: `%.3f m` of v/100.
* **Millimetres**: `%.1f mm` of 10v.

Number formatting follows the user locale's decimal separator.

### 12.3 Other displays

* Area: metric `%.3f` m² (v/10000); imperial (both inch modes) `%.3f` ft² (v·0.00107639104).
* Volume: always litres `%.3f` (v/1000).
* Mass (kg): imperial: small units and < 1 kg → ounces (v/0.0283495231), else pounds (v/0.45359237), `%.3f`; metric: small units and < 1 kg → grams `%.0f`, else kg `%.3f`.
* Density (kg/l): imperial lb/ft³ (×62.4279606), metric kg/l, `%.3f`.
* Moment of inertia: §9.3.

### 12.4 Length input parsing (result in cm)

1. Replace "," by ".".
2. If the text contains `'` or `"`: if it contains `'`, the part before `'` is feet (× 30.48) and the remainder continues; if the remainder contains `"`, cut it at the `"`; the remainder is inches (× 2.54). Sum.
3. Else if it contains "m" (or the localised metre symbol): ends with "mm" → number before the first "m" × 0.1; ends with "cm" → number before the "c" × 1; ends with "m" → number before the first "m" × 100; otherwise the whole text as centimetres.
4. Else a bare number in the current unit: mm × 0.1, feet-inches × 2.54, metres × 100, centimetres × 1. **(fix)** Decimal inches × 2.54 (the original treats it as centimetres; §13).
5. Number syntax: leading blanks removed, trailing non-digits removed; "W N/D" (whole and fraction separated by blanks) or "N/D" or a decimal number. A malformed fraction contributes 0; any parse failure yields 0.

### 12.5 Mass, density and volume input

* Mass: suffix kg/kilo/kilogram → kg; g/gram → ×0.001; lb/lbs/pounds → ×0.45359237; oz/ounces → ×0.0283495231; none → pounds in either inch mode, else kg.
* Density: "mass/volume" → mass parsed as above divided by the volume of "1"+volume-unit (litres). Without "/": the volume is guessed from the mass unit: kg → 1000 l (per m³); g → 1 l; lb → 28.3168466 l (per ft³); oz → 0.016387064 l (per in³); none → 28.3168466 in inch modes else 1.
* Volume (litres): suffix l/liter/litre/dm3/dm^3/dm³ → ×1; m/m3/m^3/m³/cubic meter(metre)/cubic → ×1000; cubic feet/foot/ft, cu ft, ft, ft3, ft^3, ft³, feet³, foot³ → ×28.3168466. **(fix)** Only strip a trailing "3" that belongs to a unit suffix, and recognise "cm3" as ×0.001 (the original strips a trailing digit 3 from any text and maps "cm3" to m³; §13).

---

## 13. Known defects of the original

Each item: what the original does, its effect, and the required behaviour.

1. **Spline max-y and min-x skip the last segment.** Effect: max width/max rocker are wrong when the extreme lies in the last segment; a single-segment spline returns the search's initial value (max y: −1.797e308, which becomes −∞ when doubled for the width; min x: +1.797e308). Required: all segments (§4.4). The section-matching heuristic intentionally keeps the skip (§8.2.2).
2. **Deck and bottom normals swapped, and sign error in their finite differences.** The board-level deck normal returns the bottom computation and vice versa; in each, the across-vector pairs (y − y_offset) with (z_offset − z) and the along-vector (x − x_offset) with (z_offset − z), so the result is the normal of a mirrored surface. Required: §8.5.
3. **Moving a section, then undo, sets its position to 0** (the old position is never recorded). Also the moved section is not re-fitted until a later edit. Required: restore the previous position; re-fit after move (§6.7).
4. **Splay is parsed and displayed as a length** (unit conversion applied, e.g. "3" in inch mode becomes 7.62). Required: plain degrees (§6.2).
5. **Vector-subtraction helper with reversed operands.** The shared helper returns "second minus first". Every call site was written against that convention, so the net results (handle vectors, interpolation a + f(b − a), split handle shortening, handle scaling) are correct as specified here; implementers must use ordinary vector maths and must not mirror the operand order.
6. **Weight-calculator branch bug**: the "> 9 ft" defaults are always overwritten by the "> 7 ft" branch. Required: else-if chain (§11.2).
7. **Spline curvature at x passes x as the curve parameter** (profile radius readout is meaningless). Required: §4.3.
8. **Tangent-angle "secant" step multiplies the error by the slope** instead of dividing. Converges only by luck; the bisection fallback rescues it, and results are within 0.05° either way. Specified as-is for numerical compatibility (§3.7); a correct secant step is an acceptable alternative where bit-level agreement is not required.
9. **Reverse normal-angle search, not-found case, drops preceding segment lengths** (s becomes the fraction within one segment only). Required: §4.7 step 3.
10. **Previous-section index clamp sets the index to n instead of n − 2** (out of range; unreachable). Required: n − 2.
11. **Rocker-preserving scale leaves the deck ends at their old height** while the bottom ends are scaled by the length factor, so nose/tail thickness becomes non-zero or negative. Also the handle-rotation weighting uses the old max-thickness position against new x's (compat). Required: re-snap deck ends to the bottom ends (§10.1.2).
12. **Factor scaling uses the plain scale but undo/redo use the rocker-preserving scale.** Required: same variant (§10.1.4).
13. **Scale-ghost** passes centre width/thickness as targets to a max-based scale (§10.1.6).
14. **Centre-of-mass cache is not invalidated by scaling**, and a true value of exactly 0 is recomputed every time. Required: invalidate on any geometry change.
15. **Rocker-stick adjustment is not a rotation** (matrix [[sinβ, cosβ], [cosβ, sinβ]]). Required: §10.3. (Setting is off by default.)
16. **Partner-link establishment shifts the partner's handles by the wrong sign** when the two end points did not already coincide (§2.9).
17. **Adding a section at exactly x = L** sorts it after the nose end section (stable sort), breaking the end-section invariant. Required: reject p ≥ L (§6.7).
18. **Decimal-inch mode treats a bare number as centimetres** (§12.4).
19. **Volume input strips a trailing "3" from any text** ("23" → 2) and treats "cm3" as cubic metres (§12.5).
20. **Range-limited "x of max y"** returns the x of the chosen segment's unrestricted maximum (§4.4).
21. **Continuity check uses unsigned handle angles**, so V-shaped handles mirrored about the vertical pass as continuous (§2.6) (compat).
22. **Hit test with deck and bottom both active takes "grabbed element" (point/prev/next) from the first curve**, not from the curve that matched, so the wrong element of a bottom knot may be dragged (§2.8).
23. **Tangent at arc length ≥ total evaluates the last segment at t = −1** (§4.5).
24. **Board-level normal-at-(x, s) helpers put x into the first component of both tangent vectors** (unused) (§8.5).
25. **Raw point assignment multiplies by the axis factors** (§2.3) (compat; axis factors are 1 when it is used).
26. **Segment-level tangent/curvature at x use a mirrored Newton start** (compat; §3.6).
27. **Grid-refine termination tests** compare best_t with the half-width using a signed difference; on the left half of a segment the coarse sample is returned (compat; §3.5).
28. **Subdivision fallback for x returns t = 0** when no sample is within tolerance (x above the segment range) (compat; §3.5.2).
29. **Weight calculator** does not refresh the total on every input change; glass unit weight shown with a mass unit (§11.3).
30. **Section normal at s** returns (cos, sin) instead of (sin, cos) of the normal angle (unused).
31. **Flip view** toggles the third view based on the second view's state (GUI only).
32. **Newly added knots carry no handle locks** until the board is reloaded (§2.8).
33. **Section blending fails** (no section; surface point (0, 0, 0)) when every interior match score truncates to 0 (§8.2.2) — degenerate edge case; required behaviour: when all scores are 0 pick the first interior knot of R as the worst match.
34. **Spline length up to a "g" value** uses the knot count instead of the segment count (unused).
35. **Generic root finder's secant start** is a fraction not mapped into [lo, hi] (correct only for [0, 1]) (§5.1).
36. **Add point with deck and bottom both active** splits *both* curves and inserts one and the same knot object into both (the second split overwrites the values of the knot already inserted into the deck), corrupting the deck. Required: in that mode add the point only to the curve nearest to the click.
37. **Delete points with deck and bottom both active** tries to delete every selected knot from both curves; a knot not belonging to a curve causes a failure. Required: delete each knot only from the curve that contains it.

---

## 14. Appendix: constant tables

### 14.1 Curve numerics

| constant | value | use |
|---|---|---|
| TLO / THI | 1e-10 / 1 − 1e-10 | near-end parameters |
| position tolerance | 0.003 cm | t for x (Newton and subdivision) |
| Newton iteration limit | 30 | t for x |
| grid-refine samples | 96 | min/max, t-for-x fallback, distance search |
| grid-refine tolerance | 0.0001 | min/max, t-for-x fallback, distance search |
| nearest-point samples / tolerance | 32 / 0.001 | nearest parameter |
| length tolerance | 0.001 cm | arc length and t at length |
| length min interval | 0.001 (t) | arc length recursion |
| t-at-length min interval | 1e-5 | |
| angle tolerance | 0.05° | t for tangent, normal-angle searches |
| angle iteration limit | 50 | t for tangent (each phase) |
| angle bisection min interval | 1e-5 | |
| corner fallback | THI − 0.05 | reverse search without snap-to-corner |
| root finder value tolerance | 0.001 | generic root finder |
| root finder iteration limit | 50 (each phase) | |
| root finder bisection min interval | 1e-4 | |

### 14.2 Board numerics

| constant | value |
|---|---|
| apex normal angle | 90° (88° in the cross-section view's apex line) |
| tuck-under normal angle | 175° |
| release-angle max knot distance | 0.9 cm |
| tuck radius minimum apex–tuck lateral gap | 0.18 cm |
| tuck radius curvature samples | 5 |
| interpolated-section minimum size | 0.5 cm (thickness and width) |
| section scale floor / no-op threshold | 0.1 cm |
| volume: x range, panels, section splits | [0.01, L − 0.01], 30, 10 |
| centre of mass: panels, splits | 10, 10 |
| moment of inertia: slices, density | 30, 0.1 kg/l |
| planform area panels | 10 over [TLO, L − TLO] |
| section-blend area steps | 10 |
| volume distribution | 20 intervals, split 8 |
| max thickness sampling | every 0.1 cm |
| surface normal offsets | x + 0.1, s ± 0.01 |
| surface x clamps | [0.1, L − 0.1]; control-point normal additionally [1, L − 1] |
| deck/bottom normal step (fixed version) | 0.01 cm |
| continuity tolerance | 0.02 rad |
| similarity weights | continuity 1.0; position 4.0 per (Δ/0.5)²; angle 15.0 per (Δdeg/5)² per handle; length 0.3 per Δ² per handle |
| hit-test radius | 4 screen pixels |
| weight-calc stringer integral panels | 20 |
| weight-calc girth polyline steps / x panels | 10 / 10 |
| foot / inch | 30.48 / 2.54 cm |
