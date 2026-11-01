# CNC Machining (CAM) — Behavioural Specification

Status: clean-room specification of the machining features of BoardCAD 3.0.1, written for an independent Rust implementation.
Companion documents: `geometry.md` (board model, curves, surface models, angle conventions), `file-formats.md` (board and blank files), `templates.md` (printable sandwich templates).

Conventions used in this document:

* **MUST / SHOULD / MAY** are normative.
* "*Original:*" paragraphs describe what BoardCAD 3.0.1 does. Where that is harmless it is also the required behaviour. Where it is a bug, the paragraph is followed by "*Required:*" and the defect is listed in §13 with an identifier `C-nn`.
* "**(new)**" marks behaviour or settings that the original does not have and that this specification adds to satisfy the safety requirements of §8.8 and §9.4. An implementation offering a "compatibility mode" MAY omit (new) items only where stated.
* Angles are in degrees unless stated otherwise. Board geometry is in centimetres (cm). Machine output is in millimetres (mm).

---

## Table of contents

1. Scope and overview
2. Inputs from the board model
3. Machine configuration (settings)
4. Coordinate systems and transforms
5. Surface-splits strategy (the main 3-/4-axis generator)
6. Width-splits strategy
7. Hot-wire and template-cutter outputs (profile, outline, Atua cores)
8. Cutters and tool compensation
9. Blank holding systems (supports, end clamps, rotating board)
10. Program output (G-code dialect and formatting)
11. Progress, statistics and time estimate
12. Legacy script interface (out of scope)
13. Known defects of the original and intended behaviour
14. Points that could not be fully determined
Appendix A. Worked numbers for the default configuration
Appendix B. Constants

---

## 1. Scope and overview

The CAM feature turns a board design into CNC programs:

1. **Machining job (milling)** — the main feature. Opens a "machine" dialog that holds the machine configuration (§3), shows the blank, the board, the holding system and the computed toolpaths, and on "Generate" writes **two** G-code programs: one for the deck side and one for the bottom side, using the **surface-splits** strategy (§5).
2. **Hot-wire outputs** (menu "G-Code"): a *profile* program and an *outline* program for a two-axis hot-wire foam cutter (§7.1, §7.2).
3. **Width-splits outputs** (menu "G-Code → Deck/Bottom"): a simpler raster strategy (§6).
4. **Atua cores outputs** (menu "Extensions → Atua Cores"): normalised two-column profile/outline files for a template cutter (§7.3).

Processing pipeline for every milled sample (normative order):

```
board sample (x, side, pass)        -- §5: surface point P and outward normal n, board frame, cm
  -> side flip (bottom programs)    -- §4.3
  -> sandwich compensation          -- §5.11 (optional)
  -> placement transform            -- §4.4: reference frame, mm
  -> 4th-axis rotation              -- §8.7 (rotating-board machines only)
  -> cutter compensation            -- §8.2–8.6: tool-tip position
  -> stringer stay-away             -- §8.4
  -> gouge verification             -- §8.8 (new)
  -> holding-system avoidance       -- §9.4
  -> machine offset                 -- §4.5
  -> axis scaling and formatting    -- §10
```

Machining runs in the background; the user can cancel. *Original:* cancelling closes and deletes the file currently being written; the remaining file of the job is then also abandoned (the cancel flag persists). This is the required behaviour.

---

## 2. Inputs from the board model

The CAM component consumes a **board** (the design, always present) and an optional **blank** (the foam blank, loaded from a board-format file; see §3.2 "Blank"). Both expose the same model (`geometry.md` §6). The CAM needs only the following queries; all lengths in cm, board frame as in `geometry.md` §1.2 (x from tail 0 to nose L, y lateral, z up, rocker datum at the lowest bottom point).

| Query | Meaning | Reference |
|---|---|---|
| L | board length | geometry §6.4 |
| w(x) | outline half-width at x | geometry §1.3, §4.3 |
| z_d(x), z_b(x) | deck and bottom height on the stringer (centreline) | geometry §1.3 |
| T(x) | centre thickness z_d − z_b | geometry §1.3 |
| profile normal at x | for the outline, deck and bottom curves: unit vector (sin α, cos α) with α = tangent angle + 90° | geometry §4.3 |
| deck height H_d(x, y), bottom height H_b(x, y) | heights of the surfaces above a plan point; 0 outside the outline | geometry §8.4 |
| outward deck/bottom normal at (x, y) | | geometry §8.5 (with its fix) |
| **band point** P(x, φa, φb, s, corner) and **band normal** N(x, φa, φb, s, corner) | point and outward unit normal on the right half (y ≥ 0) of the cross-section at station x, at fraction s ∈ [0,1] of the girth sub-range between the place where the outward normal angle equals φa (s = 0) and where it equals φb (s = 1) | geometry §8.1, §8.5 ("surface point/normal by (x, α_start, α_end, i, n)", s = i/n) |
| whole-section point P(x, s) | s = 0 deck centre, s = 1 bottom centre | geometry §8.5 |
| max rocker | maximum of the bottom curve | geometry §6.4 |
| max deck within the first / last foot | (x, z) of the highest deck point in [0, 30.48] / [L − 30.48, L] | geometry §6.4 |
| line–curve intersection | point where a straight line meets the bottom or deck profile curve | geometry §4.6 (tt root) |
| distance from a plan point to the outline curve | used by sandwich compensation | geometry §4.8 |
| tt-parametrised profile/outline points and normals | segment-index parameter | geometry §4.6 |

**Normal-angle convention** (`geometry.md` §1.5): the outward normal of a cross-section is described by its angle φ measured from straight up (+z) towards the outside (+y): φ = 0° deck centre, φ = 90° rail apex (normal horizontal, outward), φ = 180° bottom centre. Values between 90° and 180° are the underside of the rail (tuck). The band point lookups treat a minimum angle ≤ 0 as "deck centre" and a maximum angle ≥ 270 as "bottom centre" (geometry §8.1). Inside a sharp corner (tangent discontinuity, e.g. a hard edge) the corner flag chooses between the corner point itself (flag true) and a point just short of the corner (flag false); see geometry §4.7.

**Orientation warning for profile normals.** The profile-curve normal of geometry §4.3 is the right-hand perpendicular of the direction of travel (tail → nose). For the deck curve it points **down into the board**, for the bottom curve it points **down, out of the board**, and for the outline (right half) it points **towards the stringer** (into the board). The CAM requires **outward** normals everywhere; §5.4 states, per pass type, how the outward normal is obtained. (The original used these vectors as if they were outward; see C-05.)

---

## 3. Machine configuration (settings)

### 3.1 Value types

| Type | Storage | Display / entry |
|---|---|---|
| length | real, centimetres | shown and entered in the user's current length unit (cm, mm, m, inches, decimal inches); converted to cm on entry |
| real | real, unit as stated per setting (no conversion) | plain number |
| integer | integer | plain number |
| boolean | true/false | checkbox |
| choice | integer index into a fixed list of labels | drop-down |
| file | path string | file chooser |

Settings are grouped in **categories**, displayed in this order: General, Toolpath, Speed, Control, Machine offset, Cutter, Blank holding system, Scaling, Sandwich compensation, and (new) Safety. Each setting has a *label* (shown to the user, English text below), a type, a default and a valid range.

All values are persisted per user when the job is generated and loaded when the dialog opens. After loading, every derived quantity (cutter dimensions, scale factors, placement, sandwich values) MUST be recomputed from the loaded values. *Original:* this happens by re-signalling every setting after loading; settings whose derived value is only refreshed on change notification would otherwise keep stale or zero values (e.g. scale factors would be 0).

### 3.2 General

| Label | Type | Default | Valid | Meaning |
|---|---|---|---|---|
| Use settings in .brd file | boolean | true | — | Prefill from and write back to fields of the board file (§3.13). |
| Cutter type | choice: 0 "Bullnose cutter", 1 "Stl cutter", 2 "Rotating board system" | 0 | 0–2 | Selects the cutter model (§8). Coupled with the holding system (§3.12). |
| Blank holding system type | choice: 0 "Supports", 1 "Endclamps", 2 "Rotating board system" | 0 | 0–2 | Selects §9.1, §9.2 or §9.3. |
| Blank | file | "" | readable board-format file | The foam blank. Extension `.s3d` → that format, `.srf` → that format, `.cad` → ignored (not supported as a blank), anything else → native `.brd` format (`file-formats.md`). Changing it recomputes the placement. |
| Rotating board system cutter | choice: 0 "Bullnose cutter", 1 "Stl cutter" | 0 | 0–1 | Present only while Cutter type = 2: the cutter mounted on the rotating-board machine. |

### 3.3 Toolpath

| Label | Type | Default | Valid | Meaning |
|---|---|---|---|---|
| lengthwise resolution | length | 1.0 cm | 0.01 cm … L | Spacing Δ of samples along every lengthwise pass (§5.2). |
| Deck cuts | integer | 28 | 0 … 1000 | Passes per side in the deck band (§5.3). |
| Deck rail cuts | integer | 7 | 0 … 1000 | Passes per side in the deck-rail band. |
| Bottom cuts | integer | 26 | 0 … 1000 | Passes per side in the bottom band. |
| Bottom rail cuts | integer | 0 | 0 … 1000 | Passes per side in the bottom-rail band. |
| Bottom clean-up cuts | integer | 2 | 0 … 100 | Clean-up passes per side, bottom program (§5.4.6). |
| Deck clean-up cuts | integer | 2 | 0 … 100 | Clean-up passes per side, deck program. |
| Deck stringer cut | boolean | true | — | Cut one pass along the centreline of the deck. |
| Bottom stringer cut | boolean | true | — | Same, bottom. |
| Staw away from stringer *(sic)* | boolean | true | — | Keep the tool edge away from the centreline (§8.4). |
| Stringer width | length | 0.5 cm | 0 … 10 cm | Clearance W used by §8.4 and by the side-change and cut-off moves. |
| Deck outline cut | boolean | true | — | One outline pass per side in the deck program (§5.4.5). |
| Deck outline cut depth | choice: 0 "To rail apex", 1 "To bottom" | 0 | 0–1 | Height of the deck outline pass. |
| Bottom outline cut | boolean | true | — | One outline pass per side in the bottom program. |
| Cut off nose | boolean | false | — | Run the nose cut-off sequence in the deck program (§5.4.7). |
| Cut off tail | boolean | false | — | Run the tail cut-off sequence in the bottom program. |
| Deck definition angle | real, degrees | 28 | 0 … 180 | Outer end of the deck band (normal angle). |
| Deck rail definition angle | real, degrees | 130 | DeckAngle … 180 | Outer end of the deck-rail band. **On 3-axis machines limited to 90 (§5.13).** |
| Bottom definition angle | real, degrees | 90 | 0 … 180 | Outer end of the bottom band (normal angle in board terms, 180 = bottom centre). |
| Bottom rail definition angle | real, degrees | 90 | 0 … BottomAngle | Outer end of the bottom-rail band. **On 3-axis machines limited to ≥ 90.** |

### 3.4 Speed

Speeds are feed rates of the tool along the path, in **mm per second**; they are output as mm/min (× 60).

| Label | Type | Default | Valid | Used for |
|---|---|---|---|---|
| Cutting speed | real, mm/s | 80 | > 0 | band passes (deck, bottom), their transitions, clean-up passes, side changes |
| Stringer cutting speed | real, mm/s | 254 | > 0 | stringer passes and cut-off sequences |
| Rail cutting speed | real, mm/s | 254 | > 0 | rail-band passes and their transitions |
| Outline cutting speed | real, mm/s | 254 | > 0 | outline passes |

### 3.5 Control

The four action settings share the choice list: 0 "Do nothing", 1 "Home", 2 "Home Ref.", 3 "Lift clear", 4 "Script". Their effect is defined in §10.4.

| Label | Type | Default | Valid | Notes |
|---|---|---|---|---|
| Before deck cut | choice | 1 (Home) | 0–4 | |
| Before deck cut script | file | "" | — | present only while the action is 4 |
| Deck start at | choice: 0 "Nose", 1 "Tail" | 1 | 0–1 | end of the board where the deck program starts |
| After deck cut | choice | 1 | 0–4 | |
| After deck cut script | file | "" | — | present only while the action is 4 |
| Before bottom cut | choice | 1 | 0–4 | |
| Before bottom cut script | file | "" | — | |
| Bottom start at | choice: 0 "Nose", 1 "Tail", 2 "Previous from deck cut" | 0 | 0–2 | 2 = start at the end where the deck program finished |
| After bottom cut | choice | 1 | 0–4 | |
| After bottom cut script | file | "" | — | |
| Clear height | length | 20.0 cm | 0 … 100 cm | present only while at least one of the four actions is 3 |

### 3.6 Machine offset

| Label | Type | Default | Meaning |
|---|---|---|---|
| X ref. offset | length | 0 | Position of the machine's program zero in the reference frame (§4.5). |
| Y ref. offset | length | 0 | |
| Z ref. offset | length | 0 | |

### 3.7 Cutter

The content of this category depends on the cutter type.

Bullnose cutter (also the inner cutter of the rotating-board system when its sub-type is 0):

| Label | Type | Default | Valid | Meaning |
|---|---|---|---|---|
| Diameter | length | 2.54 cm | > 0 | D, full diameter of the end mill |
| Corner radius | length | 0.5 cm | 0 … D/2 | r; r = 0 flat end mill, r = D/2 ball nose |
| Height | length | 6.5 cm | > 0 | H, length of the cutting part above the tip (used for collision checks; §8.9) |

STL cutter (and inner cutter sub-type 1):

| Label | Type | Default | Valid | Meaning |
|---|---|---|---|---|
| Scale X | real | 1.0 | > 0 | multiplies the STL x coordinates |
| Scale Y | real | 1.0 | > 0 | |
| Scale Z | real | 1.0 | > 0 | |
| Cutter file(.stl) | file | "" | ASCII STL | §8.6 |

When the cutter type changes, the values of the previous cutter's category are saved to preferences and the new cutter's values are loaded (each cutter kind keeps its own values).

### 3.8 Blank holding system

Supports (type 0). Values are **real numbers in millimetres** (not length-typed):

| Label | Default | Valid | Meaning |
|---|---|---|---|
| Tailstop position | 1000 mm | ≥ 0 | X of the tail-stop plane against which the blank's tail end rests |
| Support 1 from tailstop | 600 mm | > 0 | X distance from the tail stop to support 1 |
| Support 2 from zero | 2600 mm | > TailstopPos + Support1Pos | **absolute** X of support 2 (note the different reference, as the label says) |
| Support 1 height | 260 mm | ≥ 0 | Z of the top of support 1 |
| Support 2 height | 300 mm | ≥ 0 | Z of the top of support 2 |

End clamps (type 1) and rotating board system (type 2), length-typed:

| Label | Default | Valid | Meaning |
|---|---|---|---|
| Clamp length | 80 cm | > 0 | extent of each clamp along X (see §14 on the magnitude) |
| Clamp height | 20 cm | > 0 | extent along Z, centred on the clamp axis |
| Clamp width | 20 cm | > 0 | extent along Y, centred on the clamp axis |
| Clamp safe zone *(label key exists without English text; show "Clamp safe zone")* | 5 cm | ≥ 0 | clearance added around the clamps |
| Clamp overlap on stringer | 8 cm | ≥ 0 | how far each blank end is inside its clamp |
| Blank vertical offset | 10 cm | any | vertical distance of the blank's deck reference line from the clamp axis (§9.2) |

When the holding system type changes, the previous category values are saved and the new type's values loaded.

### 3.9 Scaling

Real numbers, applied to the output words (§10.6):

| Label | Default | Valid |
|---|---|---|
| X scale | 1.0 | ≠ 0 |
| Y scale | 1.0 | ≠ 0 |
| Z scale | 1.0 | ≠ 0 |
| A scale | 1.0 | ≠ 0 |
| F scale | 1.0 | > 0 |

A negative scale mirrors that machine axis (useful when the machine's axis direction differs from the reference frame).

### 3.10 Sandwich compensation

| Label | Type | Default | Meaning |
|---|---|---|---|
| Deck compensation enabled | boolean | false | §5.11 |
| Deck compensation | length | 0.3 cm | deck skin thickness t_d (field disabled while off) |
| Deck rail limit | length | 0.0 cm | no deck compensation within this plan distance of the outline (0 = off) |
| Bottom compensation enabled | boolean | false | |
| Bottom compensation | length | 0.3 cm | t_b |
| Bottom rail limit | length | 0.0 cm | |
| Rail compensation enabled | boolean | false | |
| Rail compensation | length | 0.3 cm | t_o, applied to outline passes |

Every time the machining dialog is opened the three "enabled" switches are forced off (a deliberate safety measure of the original, to avoid machining an undersize core by accident). Required: keep this.

### 3.11 Safety (new)

| Label | Default | Valid | Meaning |
|---|---|---|---|
| Gouge tolerance | 0.05 mm | 0.001 … 1 mm | maximum allowed penetration of the tool into the board (§8.8) |
| Retract clearance | 25.4 mm | ≥ 5 mm | clearance above the highest obstacle or toolpath point for retracts and traverses (§10.5) |
| Support length along X | 50 mm | ≥ 0 | X extent of each support post (§9.1.6) |
| Tail-stop height above support 1 | 100 mm | ≥ 0 | Z extent of the tail stop above support 1's top (value taken from the original's 3-D machine model) |
| Linking sample spacing | 2 mm | 0.1 … 10 mm | maximum horizontal spacing when verifying straight moves (§8.8) |
| Maximum A step | 2° | 0.1 … 45° | maximum change of the A axis between two output points (§8.7) |

### 3.12 Interdependencies

* Cutter type 2 (rotating board) and holding system 2 (rotating board) imply each other. Selecting cutter type 2 sets the holding system to 2. Selecting holding system 2 sets the cutter type to 2. Selecting cutter type 0 or 1 while the holding system is 2 sets the holding system to 0 (supports). Selecting holding system 0 or 1 while the cutter type is 2 sets the cutter type to 0.
* **Machine kind** (new, derived): *4-axis* when the cutter type is 2, otherwise *3-axis*.
* "Clear height" exists only while any of the four Control actions is "Lift clear"; each "… script" setting exists only while its action is "Script".
* The rotating-board sub-type setting exists only while the cutter type is 2.

### 3.13 Prefill from the board file

When the dialog opens and "Use settings in .brd file" is on, settings are overwritten from board-file fields (field numbers per `file-formats.md` §3.6):

| Setting | From field |
|---|---|
| Support1Pos, Support1Height | strut 1 elements 0 and 1 (p27) — only when the holding system is Supports |
| Support2Pos, Support2Height | strut 2 elements 0 and 1 (p28) — idem |
| Blank | blank file (p10) |
| DeckCuts | top cuts (p11) |
| DeckRailCuts | top shoulder cuts (p46) |
| BottomCuts | bottom cuts (p12) |
| BottomRailCuts | bottom rail cuts (p47) |
| DeckAngle | top shoulder angle (p44) |
| DeckRailAngle | maximum angle (p17) |
| CuttingSpeed, RailCuttingSpeed, OutlineCuttingSpeed | regular speed (p26) |
| StringerCuttingSpeed | stringer speed (p25) |

When the dialog closes (and the option is still on) the same fields of the board in the editor are written back (speeds truncated to integers). *Original:* the regular-speed field is written twice, the second time with the rail speed, so the rail speed wins (C-30). *Required:* write CuttingSpeed to the regular-speed field only. *Required (C-31):* a board file field that is 0 (the file default) MUST NOT overwrite a setting whose valid range excludes 0 (speeds) and SHOULD NOT overwrite cut counts or angles; otherwise boards without CAM data produce F0 programs or empty programs.

The machining job works on a copy of the board taken when the dialog opens.

### 3.14 Validation (required; the original validates nothing)

Before generation, reject (with a message naming the setting) any value outside the "Valid" column, and additionally:

* D > 0, 0 ≤ r ≤ D/2, H > 0; for the STL cutter, a successfully loaded file with ≥ 1 facet.
* At least one pass would be produced for each program, otherwise warn.
* DeckAngle ≤ DeckRailAngle; BottomRailAngle ≤ BottomAngle.
* On 3-axis machines: if DeckRailAngle > 90 or DeckAngle > 90, or BottomAngle < 90 or BottomRailAngle < 90, show a warning and use the limited values of §5.13 (do not refuse).
* Supports: Support2Pos − (TailstopPos + Support1Pos) > 0 (support 2 beyond support 1).
* A blank file, if given, must load; generation without a blank is allowed (C-25).
* Script files, if selected, must be readable (otherwise a warning; the program gets the comment of §10.4).

---

## 4. Coordinate systems and transforms

### 4.1 Board frame (cm)

As `geometry.md` §1.2: x along the stringer from the tail (0) to the nose (L); y lateral, the modelled half is y ≥ 0 ("right"), the mirror half y ≤ 0 ("left"); z up.

### 4.2 Side frame (cm)

Each program machines one side from above (+z of the side frame):

* **Deck side**: identical to the board frame.
* **Bottom side**: the board turned over. *Original and required:* the turn-over is modelled as a mirror of z only: a point (x, y, z) becomes (x, y, −z) and a normal (n_x, n_y, n_z) becomes (n_x, n_y, −n_z). (A physical turn about the stringer also negates y. Because boards are symmetric this does not change the geometry; it only means that the "right" passes of the bottom program physically lie on the other rail. Keep the mirror convention.)

### 4.3 Bottom-side mapping summary

Throughout §5, "flip" means the mapping of §4.2. Every bottom-program point and normal MUST be flipped exactly once, including stringer, outline and clean-up passes (see C-05, C-12).

### 4.4 Placement transform: side frame → reference frame (mm)

The holding system (§9) provides, per side, a placement offset o = (o_x, 0, o_z) in cm and a rotation angle θ in radians about the lateral (y) axis. A side-frame point p (cm) and normal n map to the reference frame by

```
p_ref = 10 · ( R(θ) · p + o )          (mm)
n_ref = R(θ) · n                        (unit, no scaling)

         |  cos θ   0   sin θ |
R(θ)  =  |    0     1     0   |
         | −sin θ   0   cos θ |
```

i.e. rotate about y through the side-frame origin (the tail end of the stringer at the rocker datum), translate, then convert cm → mm. A positive θ lowers points with larger x (the nose goes down).

The factor 10 is the cm→mm conversion; the hot-wire and Atua outputs use other factors (§7).

### 4.5 Reference frame and machine frame (mm)

The **reference frame** is fixed to the machine table and defined by the holding system:

* Supports (§9.1): origin at the machine reference point (drawn as a cross in the machine view); +X along the board towards the nose; Z = 0 at the base plane from which support heights are measured; Y = 0 in the vertical plane through the stringer.
* End clamps / rotating board (§9.2, §9.3): origin on the clamp (rotation) axis at the inner face of the tail-end clamp; +X along the axis towards the nose; Z up; Y across.

The **machine frame** is the reference frame shifted so that the program zero is at the configured offset:

```
q = tip − 10 · (CutterXOffset, CutterYOffset, CutterZOffset)      (offset settings are cm)
```

where `tip` is the tool-tip position in the reference frame (§8.1). Output words are then scaled (§10.6).

### 4.6 Output units

* X, Y, Z: millimetres (the program begins with G21).
* A: degrees (required; C-03).
* F: millimetres per minute.
* Dwell P: seconds.

---

## 5. Surface-splits strategy

This is the generator behind "Generate" in the machining dialog. It cuts the board as a set of lengthwise passes; across the width the passes are placed in **bands delimited by normal angles** and spaced evenly in girth (arc length of the cross-section) within each band.

### 5.1 Program structure and files

The user chooses a file path. Let *base* be that path without its extension. Two files are written, in this order, by one background job:

* `<base>_deck.nc` — deck program,
* `<base>_bottom.nc` — bottom program.

(The suffixes come from the translatable strings "_deck" and "_bottom".)

Per program the generator produces an ordered sequence of **sample points** (side frame, cm) with an outward normal and a **feed class** (stringer, rail, outline, cutting), grouped into **passes**. Each sample goes through the pipeline of §1. The program text is defined in §10.

### 5.2 Lengthwise sampling

Let Δ = LengthwiseResolution (cm) and

```
L' = L − 0.1                      (cm)
N  = floor(L' / Δ) + 1
```

The x of sample k (k = 0 … N) of a pass is

* tail → nose: x_k = k·Δ,
* nose → tail: x_k = (N − k)·Δ,

each clamped to [0.1, L']. So every pass runs from 1 mm inside the tail to 1 mm inside the nose (or back), on the grid of multiples of Δ, with a shorter last step at the far end.

Band passes (§5.4.2, §5.4.3) consist of the 5 transition samples of §5.5 followed by these N + 1 samples. Stringer, outline and clean-up passes consist of the N + 1 samples only. *Original:* these three pass types emit one extra sample at k = N + 1, which clamps to the same x as k = N and is dropped by the duplicate rule (§10.5); emitting N + 1 samples is equivalent.

*Original:* normals of deck passes are evaluated with x clamped to [1.0, L' − 1.0] while the points use [0.1, L']; bottom passes clamp normals to [0.1, L' − 0.1]. Harmless; an implementation MAY evaluate the normal at the same x as the point.

### 5.3 Angle bands and pass placement across the width

A **band** is defined by two normal angles (φa, φb) and a pass count n ≥ 1. Pass i (i = 1 … n) lies at band fraction s = i/n, i.e. at the band point P(x, φa, φb, i/n, corner) with normal N(x, φa, φb, i/n, corner) (§2). The band's start (s = 0) is *not* cut by the band itself: it is covered by the previous band or by the stringer pass. Because s is a fraction of the girth between the two angle-defined end points at each station, the stepover is constant in girth at a given x and shrinks where the board narrows; passes converge at the tips.

Bands (right half; the left half is the mirror y → −y, n_y → −n_y):

| Program | Band | φa (s = 0) | φb (s = 1) | n | corner flag |
|---|---|---|---|---|---|
| deck | deck band | deck centre (0°) | DeckAngle | DeckCuts | true |
| deck | deck-rail band | DeckAngle | DeckRailAngle | DeckRailCuts | true |
| bottom | bottom band | **bottom centre** | BottomAngle | BottomCuts | true |
| bottom | bottom-rail band | BottomAngle | BottomRailAngle | BottomRailCuts | true |

Bottom-program points and normals are flipped (§4.2) after evaluation.

*Original:* the bottom band's start is requested as "normal angle 360°". The surface model looks this angle up like any other (only angles ≥ 270 as *maximum* are treated as bottom centre), finds no match, and falls back to the best-matching segment while dropping the lengths of the preceding segments, so the start of the bottom band is ill-defined and may lie some distance out from the centreline. The strip between the centreline and that point is then only touched by the stringer pass — a coverage gap typical of flat and concave bottoms (C-06). *Required:* the bottom band starts exactly at the bottom centre (girth fraction of the bottom centre, s_section = 0 in geometry §1.4 terms).

### 5.4 Pass types

All points below are in the side frame (cm) before flipping unless stated. "Right" = y ≥ 0; the corresponding left pass is its mirror (y → −y, n_y → −n_y).

#### 5.4.1 Stringer pass

One pass along the centreline (y = 0).

* Deck: P = (x, 0, z_d(x)). Outward normal: the deck profile's normal vector (sin α, cos α) of geometry §4.3 **negated**, i.e. n = (−sin α, 0, −cos α) (pointing up).
* Bottom: P = (x, 0, z_b(x)); outward normal = the bottom profile's normal (sin α, 0, cos α) as is (pointing down); then flip both (§4.2).

*Original:* uses the deck profile normal un-negated (pointing into the board) and, for the bottom, negates the x component instead of flipping z; both stringer normals therefore point into the board (C-05). With the bullnose cutter this puts the tool about 2r too deep wherever the centreline is not exactly horizontal in the reference frame, and inverts sandwich compensation on these passes.

Feed class: stringer.

#### 5.4.2 Band passes (deck and bottom bands)

For pass i of the band: transition (§5.5), then the N + 1 lengthwise samples at P(x, φa, φb, i/n, true), N(…) . Feed class: cutting.

#### 5.4.3 Rail-band passes

Same as §5.4.2 with the rail band. Feed class: rail (also for their transitions).

#### 5.4.4 Order across a side

Within one side: the band passes i = 1 … n of the main band, then i = 1 … n of the rail band, then the outline pass, then the clean-up passes (§5.6).

#### 5.4.5 Outline pass (one per side, optional)

Samples at P = (x, w(x), z_o(x)) where

* deck program, depth "To rail apex": z_o = z of the band point at normal 90° (rail apex), i.e. P(x, 90°, 90°, 0, true).z;
* deck program, depth "To bottom": z_o = z of P(x, BottomAngle, BottomAngle, 0, true) — with the default BottomAngle = 90° this is again the apex (see §14);
* bottom program: the deck-program outline point (depth choice included), then z negated.

Normal: the **outward** horizontal plan normal of the outline, n = (−n_ox, −n_oy, 0) where (n_ox, n_oy) is the outline profile normal of geometry §4.3 (which points towards the stringer). Feed class: outline.

*Original:* uses the outline profile normal un-negated, i.e. pointing into the board (C-05). With the bullnose cutter this positions the tool axis D/2 *inside* the outline and r below z_o, cutting a slot into the rail.

#### 5.4.6 Clean-up passes

Clean-up passes remove blank material beside the rail, below the outline pass.

* **Deck program**, pass i = 0 … DeckCleanUpCuts − 1: take the rail-apex band point A(x) = P(x, 90°, 90°, 0, true) and its normal (≈ horizontal, outward); replace the z of the point by z_b(x) + 1.0·i (cm). (So the first clean-up pass is at the centreline bottom height, the next 1 cm higher, …)
* **Bottom program**, pass i = 0 … BottomCleanUpCuts − 1: take the point B(x) = P(x, BottomAngle, BottomRailAngle, 1, true) (the outer end of the bottom-rail band) and its normal; flip; then move the point outward by 1.0·(i + 1) cm in y (before mirroring for the left side).

Feed class: cutting. *Original:* the bottom clean-up normal is not flipped (only the point); the normal is nearly horizontal there, so the effect is small (C-12). Required: flip it.

#### 5.4.7 Nose and tail cut-off sequences

Optional 8-point sequences that cut away the blank on both sides of the stringer close to the board tip, leaving the stringer (and W of foam, when stay-away is on) — presumably so that the excess blank at the tip can be broken off. The nose cut-off runs in the deck program, the tail cut-off in the bottom program.

Reference point Q (side frame, cm):

* nose (deck program): the deck-centre point at x_q = L − 1.0;
* tail (bottom program): the bottom-centre point at x_q = 1.0, flipped.

Let T_q = T(x_q), lift = 5.0 cm, W = StringerWidth, D and r the cutter diameter and corner radius (cm), u = +1 (required; see below). The eight positions are Q plus:

| Step | Δy | Δz | Purpose |
|---|---|---|---|
| 0 | +(W + D) | +u·(T_q + lift) | above, left of stringer |
| 1 | +(W + D) | −r | down beside the stringer |
| 2 | +(D/2 + 0.1) | −r | in towards the stringer, 1 mm from it |
| 3 | +(D/2 + 0.1) | +u·(T_q + lift) | lift |
| 4 | −(W + D) | +u·(T_q + lift) | above, other side |
| 5 | −(W + D) | −r | down |
| 6 | −(D/2 + 0.1) | −r | in, 1 mm from the stringer |
| 7 | nose: −(D/2 + 1.0); tail: −(D/2 − 0.1) | nose: +u·(T_q + lift); tail: +u·(T_q + r + lift) | lift |

The positions are treated as *tool-side* positions, not surface contacts: the tool is placed with its cylindrical side tangent to the plane x = x_q from outside the board, i.e. the tool axis is shifted by +D/2 in x for the nose and −D/2 for the tail (after the placement transform, in the reference frame along the direction of the board's x axis), with no vertical compensation. The stay-away rule (§8.4) applies to these positions. Feed class: stringer.

*Original:* (a) the positions are passed through the general cutter compensation with a horizontal normal (±1, 0, 0); because of C-04 this shifts the axis by the flat radius R_f = D/2 − r instead of D/2. (b) For the tail sequence on the bottom side the "lift" terms are multiplied by −1 although the point is already in the flipped frame, so the "lift" moves go *down through the board* (u = −1; C-13). Required: u = +1. (c) The small inconsistencies in step 7 (1 cm instead of 1 mm, the extra r for the tail) are harmless and are kept.

#### 5.4.8 Side change

Between the right side and the left side of each program. Let x_e be the x at which the tool currently is (0.1 if the next pass runs tail → nose, L' − 0.1 if it runs nose → tail — the original uses these two values), and φ_last = the rail-band outer angle if that band has passes, else the main band outer angle. Eleven samples j = 0 … 10, feed class cutting.

* **Stay-away off:** sample j is the band point P(x_e, φ0, φ_last, (10 − j)/10, c) of the right side, i.e. the tool slides over the surface from the last band's outer edge back to the centre; φ0 = deck centre (deck program) or bottom centre (bottom program, then flipped); c = true for the deck, and for the bottom c = true only when bottom rail passes exist.
* **Stay-away on:** for j = 0 … 8: if the last band reached past the apex (deck: φ_last > 90; bottom: φ_last < 90), the sample is the band point P(x_e, 90°, φ_last, (10 − j)/10, c) (moving back towards the apex); otherwise no new sample is produced (the tool waits). For j = 9: the previous position raised by 5.0 cm in z. For j = 10: that raised position moved to y = −2.5·W (deck program) or y = −0.5·W (bottom program); the stay-away rule then pushes it to −(D/2 + W) (§8.4). The normal used for j = 9 and 10 is the horizontal unit vector along ±x (+x when the next pass runs nose → tail).

The side change is skipped when the side has no band, rail, clean-up or outline passes.

### 5.5 Transitions between band passes

Before the N + 1 samples of band pass i (i = 1 … n) the tool moves across the surface at the pass's start station x_s (the x of its first sample) in M = 5 steps: transition sample m (m = 0 … 4) is the band point at band fraction

```
s = ((i − 1)·M + m) / (n·M)          (clamped to ≤ 1)
```

i.e. from the previous pass (s = (i − 1)/n) in steps of 1/(5n) towards the new pass (s = i/n), which is then reached by the first lengthwise sample. The transitions use the band's angles, corner flag, mirror and flip. Their feed class is that of the band. For pass 1 of the main band the transition starts at the band start (centreline), where the stringer pass ended (or the previous side change); for pass 1 of the rail band it starts at the main band's outer edge.

Outline and clean-up passes, and the first pass after a cut-off sequence, have **no** transition: the tool moves in a straight line to their first sample. Such straight moves are subject to §8.8 (linking moves).

### 5.6 Program sequences and directions

Passes alternate direction (serpentine): every pass ends at the end of the board where the next pass starts. **Required:** the first sample of every pass and of every cut-off sequence is at the end of the board where the tool currently is; there is never a lengthwise traverse between passes.

Deck program (the tool starts at the end chosen by "Deck start at"):

1. If starting at the tail: stringer pass (if enabled) tail → nose; then nose cut-off (if enabled).
   If starting at the nose: nose cut-off (if enabled); then stringer pass (if enabled) nose → tail.
2. Right side: deck band passes, deck-rail band passes, outline pass (if enabled), clean-up passes.
3. Side change.
4. Left side: same as 2, mirrored.

Bottom program (the tool starts at the end chosen by "Bottom start at"; "Previous from deck cut" = the end at which the deck program finished):

1. If starting at the tail: tail cut-off (if enabled); stringer pass (if enabled) tail → nose.
   If starting at the nose: stringer pass (if enabled) nose → tail; then tail cut-off (if enabled).
2. Right side, side change, left side as for the deck, with the bottom bands.

*Original deviations* (C-14):

* Deck, start at nose, with nose cut-off: after the cut-off the direction is flipped, so the stringer pass starts at the tail while the tool is at the nose (a full-length move to the start).
* Bottom, start at nose: the tail cut-off is never executed (the direction flip before the next-state decision inverts the test).
* Bottom, start at tail, with tail cut-off: the tail cut-off is executed twice, the second time starting with the tool at the nose.

Required: the sequences above.

### 5.7 Feed per sample

The feed in mm/min of a sample is `trunc(v · 60 · ScaleF)` where v (mm/s) is chosen by feed class: stringer → StringerCuttingSpeed; rail → RailCuttingSpeed; outline → OutlineCuttingSpeed; cutting → CuttingSpeed. Transitions inherit their band's class; the side change and clean-up passes are "cutting"; the cut-off sequences are "stringer". The first cutting move of each program uses the plunge feed (§10.5): `trunc(0.05 · v · 60 · ScaleF)`, minimum 1.

*Original:* ScaleF is never applied (C-02b); otherwise identical.

### 5.8 Program start and end (toolpath part)

The first sample of the program (the first sample of the first pass that exists) is approached as follows: an **approach point** 5.08 cm (2 inches) above it in the side frame (same normal, so the compensated tool position is 50.8 mm above the compensated cutting position) is reached first, then the tool descends to the first sample at the plunge feed. The surrounding G-code (preamble, before/after actions, retracts) is defined in §10.4–10.5.

### 5.9 Number of points (for testing)

Per program side, with N from §5.2: band pass = 5 + (N + 1) samples; stringer/outline/clean-up pass = N + 1; cut-off = 8; side change = 11 (fewer distinct points with stay-away on). Duplicate consecutive output positions are removed (§10.5).

### 5.10 Machining both sides — coverage requirement

The deck program covers girth from the deck centre to the deck-rail band's outer angle; the bottom program covers from the bottom centre to the bottom-rail band's outer angle. Together they MUST cover the whole section; with the 3-axis limits of §5.13 the deck program covers 0°…≤ 90° and the bottom program 180°…≥ 90°, meeting at the rail apex. The centreline strip of width 2W is deliberately left when stay-away is on (§8.4).

### 5.11 Sandwich compensation

For sandwich construction the core is machined undersize by the skin thickness, measured along the surface normal. Applied to the side-frame point (cm) after flipping and before placement:

```
P' = P − t · n̂        (n̂ = outward unit normal in the side frame)
```

* Outline passes (both programs): t = t_o if rail compensation is enabled.
* Other deck-program samples (stringer, band, rail-band, transitions, clean-up): t = t_d if deck compensation is enabled, except within the deck rail limit.
* Other bottom-program samples (same kinds): t = t_b if bottom compensation is enabled, except within the bottom rail limit.
* Side-change samples and cut-off sequences: never compensated. (*Original:* the nose cut-off is shifted by t_d along its horizontal "normal" when deck compensation is on, the tail cut-off is not; neither is a surface contact, so required: no compensation.)
* Rail limit: if the limit ℓ > 0 and the plan distance from (x, |y|) to the outline curve is < ℓ, the sample is not compensated.

*Original deviations* (C-16): the deck rail-limit setting is stored into the bottom rail limit (so the deck limit is always 0 and the bottom limit takes the last edited of the two); the distance to the outline uses the signed y, so left-side samples are never inside the limit; deck clean-up passes are treated as outline passes (rail compensation instead of deck compensation; bottom clean-ups are not). Required: as specified above (deck clean-ups get deck compensation).

### 5.12 Stay-away from the stringer

See §8.4; it acts on the compensated tool position.

### 5.13 3-axis restrictions

On a 3-axis machine the tool approaches from +Z only and cannot reach surface regions whose outward normal points below horizontal without cutting through the material above them. Therefore, on 3-axis machines:

* The deck program MUST NOT place any contact point beyond the rail apex: use DeckAngle_eff = min(DeckAngle, 90) and DeckRailAngle_eff = min(DeckRailAngle, 90).
* The bottom program likewise: BottomAngle_eff = max(BottomAngle, 90) and BottomRailAngle_eff = max(BottomRailAngle, 90).
* A band whose two ends coincide (e.g. 90…90) produces no passes.
* When a limit changes a value, add a comment line to the program (§10.7) and warn the user.

*Original:* no limit; with the default DeckRailAngle = 130° the deck-rail passes go round the apex into the tuck, and since every such contact needs the tool tip below the rail, the tool cuts through the rail from above (C-07).

On 4-axis machines no limit applies (the board is rotated so that every contact normal points up, §8.7).

---

## 6. Width-splits strategy

A simple raster strategy offered from the "G-Code" menu ("Deck" and "Bottom" items; *original:* both items write both files). It is independent of the angle bands.

Parameters: n_w = 30 width divisions (fixed), lengthwise step Δ_w = 1.0 cm (fixed). The output files are `<base>_top<ext>` and `<base>_bottom<ext>` (the user's chosen name with "_top"/"_bottom" inserted before the extension).

Passes per program: 2·n_w = 60, i = 0 … 59.

* Lateral fraction f_i = (i mod n_w)/n_w; sign σ_i = −1 if i > n_w, else +1. Pass i runs along y_i(x) = σ_i · f_i · w(x). (Pass i = n_w repeats the centre pass; the outermost passes are at (n_w − 1)/n_w of the half-width; no pass on the outline. Harmless; keep.)
* Samples: j = 0, 1, … while j < L/Δ_w: x = j·Δ_w for even i, x = L − j·Δ_w for odd i, clamped to [0, L].
* Deck program: P = (x, y_i, H_d(x, y_i)), n = outward deck normal at (x, y_i). Bottom program: P = (x, y_i, H_b(x, y_i)), n = outward bottom normal, then **flipped** (required; the original does not flip).
* Feed: StringerCuttingSpeed for pass 0, CuttingSpeed otherwise, converted to mm/min (× 60 × ScaleF).
* Program text: the generic preamble/postamble of §10.4 (no before/after actions, no approach point), cutter compensation per the selected cutter, placement per the selected holding system.

*Original:* the generator is not usable: no placement transform is ever set (generation fails immediately), the deck and bottom normal queries are swapped (geometry C-fix in §8.5 of geometry.md), feeds are written in mm/s without the × 60, and the two files are written by two concurrent background jobs sharing state (C-21). Required: as specified above, files written sequentially.

---

## 7. Hot-wire and template-cutter outputs

These outputs use a pass-through cutter (tool position = sample point) and the plain G-code writer (no axis scaling). The board is sampled per profile/outline **curve** with the segment-index parameter tt (geometry §4.6), not per x.

### 7.1 Hot-wire profile

Dialog values: "Cutting speed (per minute)" — length setting, default 50 cm, used as F = value in mm (i.e. 500 → "F500"); "Additional thickness" — length setting, default 0 cm.

File: chosen path with extension forced to `.nc`, then "_profile" inserted before the extension.

Placement (scale 10, i.e. output in mm): rotation θ = atan2(z_n − z_t, x_n − x_t) where (x_t, z_t) and (x_n, z_n) are the highest deck points within the first and last foot; offset o = (−z_b(0) · sin θ, 0, −z_t · cos θ). This tilts the board so that the two deck high points are level, with the deck top near Z = 0.

Samples (5001 per curve, tt_j = j/5000 for j = 0 … 5000), points in the x–z plane (y = 0):

1. Deck curve from nose to tail (tt from 1 to 0), each point raised by the additional thickness;
2. Bottom curve from tail to nose (tt from 0 to 1).

This forms one closed loop. Normal: unused (pass-through cutter). Feed: the dialog speed on every sample.

*Original:* the additional thickness is converted to mm and then added to a centimetre value, so it acts 10× too large (C-22); a third segment (re-tracing the start of the deck) is defined but never produced because of an operator-precedence slip — harmless, keep the two-curve loop.

### 7.2 Hot-wire outline

File: as §7.1 with "_outline". Placement: θ = 0, o = 0, scale 10. Samples (5001 per half): right half of the outline from nose to tail (tt 1 → 0), then the mirrored left half from tail to nose (tt 0 → 1). The output point is (x, 0, ±w): **the half-width is written as Z** (the hot-wire machine's second axis). Expansion (additional width) is 0 for this output.

### 7.3 Atua cores outputs

For a template cutter that expects coordinates normalised to the board length. File extension `.atua`, with "_profile" or "_outline" inserted. Writer: plain text lines (no G-code), described in §10.9.

* Profile: option "NoRotation" (default false). With rotation: θ as in §7.1; L_r = sin θ · (z_b(L) − z_b(0.1)) + cos θ · L; offset as in §7.1; scale **1/L_r** (instead of 10). Without rotation: θ = 0, offset (0, 0, −z_n) (z_n = nose-foot deck maximum), scale 1/L. Samples: deck nose → tail then bottom tail → nose (5001 each), no thickness expansion.
* Outline: θ = 0, offset 0, scale 1/L; samples as §7.2.

Header lines (profile): `Type: Rocker`, empty line, `Code: <board file name>`, empty line, `Longueur template BoardCAD: <L in mm, one decimal> mm`, `Longueur template avec rotation: <√((z_b(L) − z_b(0))² + L²) in mm> mm`, `Required thickness: <value> mm`, then the column header line. Outline: `Type: Outline`, empty, `Code: …`, empty, `Longueur: <L mm> mm`, empty, column header.

*Original:* "Required thickness" is computed from normalised coordinates and an unrelated "+ 12", then formatted as if centimetres; the number is meaningless (C-23). Required: the vertical extent of the transformed (rotated) profile, max(deck z′) − min(bottom z′), converted to mm, plus 12 mm (interpretation; see §14).

---

## 8. Cutters and tool compensation

### 8.1 Tool reference point

For all milling cutters the controlled point ("tool tip") is the centre of the lowest end of the tool, on its axis. The axis is parallel to the reference-frame +Z in the frame in which compensation is computed (after 4th-axis rotation, if any). Compensation is computed in millimetres in the reference frame: inputs are the contact point P and the outward unit surface normal n (both already transformed by §4.4).

### 8.2 Bullnose (torus) cutter — exact compensation (required)

Tool: diameter D, corner radius r, flat radius R_f = D/2 − r ≥ 0 (all mm = setting × 10). Let ρ = √(n_x² + n_y²) and the tilt φ = atan2(ρ, n_z) ∈ [0°, 180°].

* If ρ < 1e−9 and n_z > 0 (normal straight up): tip = P (the flat bottom rests on the point; the centre of the flat is placed on it).
* If ρ < 1e−9 and n_z < 0: the point is unreachable from above; report and skip it (3-axis). (*Original:* returns P.)
* Otherwise, with the horizontal unit direction ĥ = (n_x, n_y, 0)/ρ:

```
tip = P + (R_f + r·sin φ) · ĥ − r·(1 − cos φ) · ẑ
```

This places the corner torus tangent to the surface at P (the corner's tube centre is at P + r·n). The original uses exactly this formula whenever the lateral tilt atan2(n_y, n_z) is 8° or more (§8.3 covers the other case). It is valid for 0 < φ ≤ 90°. For φ > 90° the formula describes a contact on the upper half of the corner, which a real end mill does not have; such contacts MUST NOT be requested on 3-axis machines (§5.13) and are rejected by §8.8.

Special cases: r = 0 (flat end mill): tip = P + (D/2)·ĥ for any tilt > 0 (edge contact); R_f = 0 (ball nose): tip = P + r·n − r·ẑ (ball centre at P + r·n).

### 8.3 The original's flat-zone approximation (informative; optional)

*Original:* if the lateral tilt atan2(n_y, n_z) is less than 8° in magnitude, the exact formula is replaced by:

* y-shift = R_f · clamp(sin(atan2(n_y, n_z)) / sin 8°, −1, 1),
* x-shift = R_f · clamp(sin(atan2(n_x, n_z)) / sin 8°, −1, 1),
* no z change.

The intended rule had a second x branch (|atan2(n_x, n_z)| ≥ 8° → x-shift = D/2 − r(1 − sin(x-tilt))), but the comparison compares an angle in radians with 8, which is always true, so that branch never runs (C-04). The ramp smooths the jump of the contact position from the flat's centre to its edge as the surface starts to tilt, but it gouges: at small tilts by up to about R_f·(1 − sin φ/sin 8°)·tan φ (≈ 0.25 mm with the default tool), and on steep lengthwise slopes (nose and tail rocker) by roughly r·sin φ·tan φ − r(1 − cos φ) (≈ 0.8 mm at 30° with the default tool), because the corner term is ignored.

Required: use §8.2 everywhere. An implementation MAY offer the smoothing as an option, with the degree comparison fixed and only together with the gouge verification of §8.8 (which then raises the tool where needed).

### 8.4 Stringer stay-away

When enabled, after compensation: if |tip_y| − D/2 < W (W = StringerWidth in mm), set tip_y = ±(D/2 + W), with the sign of tip_y (a value of exactly 0 goes to the negative side). This keeps the tool's edge at least W from the stringer plane, so a strip of total width 2W (not W) around the centreline stays uncut. (This is the original's rule; the setting name suggests the stringer's full width, but the rule uses it as a per-side clearance. Keep, and label it "Stringer clearance (per side)" in a new UI if desired.)

In 4-axis mode the original applies the rule in the rotated frame. Required: apply it in the rotated frame only when |A| ≤ 45°; otherwise (rail region) skip it.

### 8.5 Cut-off positions

See §5.4.7: horizontal shift of ±D/2 along the board axis, no vertical compensation, stay-away applies.

### 8.6 STL cutter

**File format.** ASCII STL (binary STL is not supported by the original; an implementation SHOULD also accept binary STL). Units: millimetres. The tool tip (the controlled point) is at the STL origin, the tool axis is +Z and the cutting end is at the bottom.

*Original parsing:* lines are scanned for the text `normal` (start of a facet, the three numbers after it are the facet normal) and `endsolid` (end of file); after a facet line, the next three lines containing `vertex` give three vertices, each stored as a sample point carrying the facet normal. Matching is case-sensitive (lower case), the normal keyword must not start the line, at most 10000 vertices are read, and the last stored vertex is ignored by every later loop. Required: case-insensitive keywords, any position, no fixed limit, all vertices used.

**Scaling:** vertex coordinates are multiplied by (CutterScaleX, CutterScaleY, CutterScaleZ). *Original:* the scale is applied to the already-scaled points on every change (cumulative) and is lost on reload (C-19). Required: always scale the loaded geometry from the file values.

**Compensation.** *Original:* among all vertices, choose the one whose facet normal is most opposite to the surface normal (minimum |n + m_v|), breaking exact ties by the smallest distance of the vertex from the origin, and place the tool at tip = P − v. Required (more exact, same intent): for a convex tool, the contact point is the tool point that is extreme in the direction −n:

```
v* = argmin over tool vertices v of (n · v)       (ties: smallest |v|)
tip = P − v*
```

For non-convex tool shapes this is approximate; the gouge verification of §8.8 MUST then use the tool's convex hull or its true shape.

**Collision shape and rotation (scripts only).** The original can derive a "collision cutter" (vertices scaled by 0.9 in x and y and raised by 1 mm) or load one from a second STL, and can rotate the cutter about the 4th (x) and 5th (z) axes about a configurable centre (using π ≈ 3.1415). These are used only by the legacy scripts (§12) and are out of scope.

*Original:* when chosen in the machining dialog the STL cutter cannot work: its arrays of transformed vertices are only created by an initialisation path the dialog never calls, so the first compensation fails (C-18).

**Tool body for collisions (§8.9):** the vertical cylinder enclosing the scaled STL (radius = max horizontal distance of a vertex from the axis, height = max z).

### 8.7 Rotating board (4th axis)

The board is held between end clamps on a rotary axis (A) parallel to the reference X axis through the reference origin (§4.5). For every sample:

1. In the reference frame (after §4.4), the rotation that brings the normal into the vertical plane pointing up is
   `α = atan2(n_y, n_z)` (degrees; positive when the normal leans towards +Y).
   *Original:* takes the unsigned angle between (n_y, n_z) and (0, 1) and gives it the sign of n_y, with n_y = 0 counted as negative (a normal pointing straight down gives −180°).
2. Rotate the point and the normal about the X axis by α (right-hand rule): y′ = y cos α − z sin α, z′ = y sin α + z cos α (same for n). Now n′ has no Y component and n′_z ≥ 0.
3. Compensate with the mounted (inner) cutter (§8.2 or §8.6): tip′.
4. Output X, Y, Z = tip′ (then offset and scaling) and **A = α in degrees**.

Required additions:

* A MUST be written in degrees, multiplied by ScaleA (C-03; the original computes radians and then drops the value).
* Unwrap A: choose α + k·360° closest to the previous A of the program.
* Subdivide any move whose |ΔA| exceeds "Maximum A step" (new, 2°) by interpolating the *side-frame* sample (x, girth fraction) and recomputing, so that the linear interpolation of the machine between output points does not cut through the board.
* The machine interprets F for combined XYZA moves according to its own rules (LinuxCNC applies F to the XYZ distance when it is non-zero). Implementations MAY offer inverse-time feed (G93) as an option; the default is the original's units-per-minute mode.
* Holding-system collisions must be checked with the rotation taken into account (§9.3).

### 8.8 Gouge verification (required, new)

The original performs no check of the tool against the board (its board-collision test always answers "no collision"; C-08). Required: for every output position, and for every straight move between consecutive output positions (sampled at most every "Linking sample spacing" horizontally), the tool solid MUST NOT penetrate the board by more than the gouge tolerance.

Reference method for 3-axis (drop-cutter), in the frame in which the tool axis is +Z:

1. Build the **upper envelope** U(X, Y) of the board as positioned for this program: for every horizontal position, the highest Z of the board (deck surface for the deck program, flipped bottom for the bottom program, including the rail), as a height field on a grid with spacing ≤ min(0.5 mm, r/4) (or evaluate it analytically).
2. Tool lower profile: e(ρ) = 0 for ρ ≤ R_f; e(ρ) = r − √(r² − (ρ − R_f)²) for R_f < ρ ≤ D/2 (ρ = horizontal distance from the axis). For an STL tool, the lower envelope of its convex hull.
3. The lowest gouge-free tip height at (X, Y) is z_drop(X, Y) = max over grid points within ρ ≤ D/2 of [U − e(ρ)].
4. If tip_z < z_drop − tolerance, raise tip_z to z_drop. For straight moves, insert intermediate points at the sampling spacing and raise those that need it.

For 4-axis machines apply the same method in the rotated frame of each output point (the envelope is that of the rotated board). Raised points SHOULD be counted and reported (§11).

### 8.9 Tool body for holding-system collisions

A vertical cylinder of radius D/2 starting at the tip. Required: unbounded upwards (the spindle is above the tool). *Original:* height H taken in centimetres while the rest is in millimetres, i.e. a 6.5 mm tall cylinder for the default 65 mm tool (C-17).

---

## 9. Blank holding systems

A holding system supplies, for each program (deck, bottom), the placement (o, θ) of the **board** (used by the toolpath) and of the **blank** (used for display and tail-stop contact), and defines the obstacles of §9.4.

### 9.1 Supports (3-axis)

The blank lies on two supports (pucks or rails across the machine) with its tail end against a tail stop.

#### 9.1.1 Geometry (reference frame, cm in the formulas below)

* Tail stop: the plane X = X_T, X_T = TailstopPos/10.
* Support 1 top: (X_1, h_1) = (X_T + Support1Pos/10, Support1Height/10).
* Support 2 top: (X_2, h_2) = (Support2Pos/10, Support2Height/10).
* Support line angle: α_s = atan2(h_1 − h_2, X_2 − X_1) (positive when the supports descend towards the nose).

#### 9.1.2 Blank placement for the deck program (required statement of intent)

The blank lies **bottom down**. Find θ_B, o_B such that, with the transform of §4.4 applied to the blank:

1. the blank's bottom curve (centreline) passes through both support tops (it rests on them), and
2. the rearmost point of the blank's tail end touches the tail-stop plane X = X_T.

Solve these three conditions for (θ_B, o_Bx, o_Bz) to 1e−6 cm (e.g. fixed-point iteration on θ_B: for the current θ_B, set o_Bx from condition 2, then find the bottom-curve points vertically above the two supports, and set θ_B so that the line through them matches the support line).

*Original (informative):* 10 fixed-point iterations, starting from θ = α_s, each doing: e = min(D_t·sin θ, B_t·sin θ) with D_t, B_t the blank's deck and bottom heights at x = 0.1 (an approximation of the tail end's rearmost point); blank-frame support distances b_k = (d_k + e)/cos θ with d_1 = Support1Pos/10 and d_2 = Support2Pos/10 − X_T; contact points P_k = intersection of the blank bottom curve with the line through (b_k, 0) in direction (−sin θ, cos θ) (the support's vertical expressed in the blank frame; the line is constructed with length = max rocker); β = signed slope angle of P_1→P_2 (positive when P_1 is higher); θ := α_s − β; and o_x = X_T − e, o_z = h_1 + b_1 sin θ − P_1.z / cos θ. The result agrees with the exact solution to within the accuracy of the small-angle approximations involved.

#### 9.1.3 Board placement inside the blank (deck program)

Default ("approximate") placement: tail-aligned (o_x of the board = o_x of the blank), with the board's deck tangent-level to the blank's deck at the tail:

* Pick x_t = 1.0 and x_n = L − 0.5 (board length).
* Blank deck heights at x_t, x_n, rotated by θ_B (z′ = z cos θ_B − x sin θ_B); board deck heights at x_t, x_n.
* Board rotation θ_b = signed angle between the board's deck chord (x_n − x_t, Δz_board) and the blank's rotated deck chord (x_n − x_t, Δz′_blank) — positive when the board chord rises more.
* Board o_z = blank o_z + (blank rotated tail deck height − board rotated tail deck height), with the board's rotated tail height z cos θ_b − x_t sin θ_b.

The user can then adjust the board placement in the machine view: drag (1 screen pixel, 0.1 with Alt), arrow keys (one pixel/scale; 0.1 with Alt) for o_x and o_z, and keys Q/W to change θ_b by ∓0.004 rad (∓0.0004 with Alt). Adjustment is possible only in the deck view. *Original:* changing any support setting recomputes the default placement and discards manual adjustments; manual adjustments are not persisted. Keep, but SHOULD warn.

Without a blank, the placement is (o = 0, θ = 0) (*original*); Required (new): without a blank, place the board itself as if it were the blank (resting on the supports, tail at the tail stop).

#### 9.1.4 Bottom program placement

For the bottom program the partly machined blank is turned over: the **board's deck** (already machined) rests on the supports and the blank's tail end (whose position relative to the board is known from the deck placement) touches the tail stop. Required: solve the same three conditions as §9.1.2 with the flipped board deck curve resting on the supports and the rearmost point of the flipped blank tail end on the tail stop, giving the board's (θ, o) for the bottom program; the blank's bottom placement follows from the same relative pose.

*Original (informative):* 5 fixed-point iterations analogous to §9.1.2, using the board deck curve for the contacts (line direction (+sin θ, cos θ)), the rotation θ := α_s + β, o_z = −(−h_1 − b_1 sin θ − P_1.z/cos θ), and the blank-tail term computed from the deck-program difference between blank and board placements; the original reads "the current board of the editor" here instead of the job's copy.

#### 9.1.5 Collisions (original)

None: the supports system never reports a collision.

#### 9.1.6 Obstacles (required, new)

* Tail stop: X ≤ X_T (all Y), Z ≤ h_1 + tail-stop height (Safety, default 100 mm).
* Support k: X_k − ℓ/2 ≤ X ≤ X_k + ℓ/2 (ℓ = support length, default 50 mm), all Y, Z ≤ h_k.

Handled by §9.4.

### 9.2 End clamps (3-axis)

The blank is held at both ends by clamps on a common axis.

#### 9.2.1 Blank placement

With (x_t, z_t), (x_n, z_n) the highest blank deck points within the first and last foot:

* Deck program: θ_B = atan2(z_n − z_t, x_n − x_t); v = cos θ_B · z_t − sin θ_B · x_t − BlankVerticalOffset; h = −sin θ_B · z_t; blank offset o_B = (h − ClampOverlap, 0, −v).
* Bottom program: θ = −θ_B; o = (h − ClampOverlap, 0, +v).
* Rotated blank length (cm): L_r = sin θ_B · (z_b,blank(L_blank) − z_b,blank(0)) + cos θ_B · L_blank.

#### 9.2.2 Board placement

The board's deck placement is **not** computed automatically: it starts at o = 0, θ = 0 and is set by the user in the machine view (§9.1.3 interaction). The bottom placement is derived from it: o_bottom = (o_x, o_y, −o_z), θ_bottom = −θ (the mirror about the clamp axis that corresponds to turning the blank over in the clamps).

#### 9.2.3 Clamp geometry (reference frame, mm; all settings × 10)

Let L_c, H_c, W_c, s, v_o = clamp length, height, width, safe zone, overlap.

* Tail-end clamp: X ∈ [−L_c, 0], |Y| ≤ W_c/2, |Z| ≤ H_c/2.
* Nose-end clamp: X ∈ [10·L_r − 2·v_o, 10·L_r − 2·v_o + L_c], same Y and Z extents.
* Safety boxes (obstacles): each clamp box enlarged by s in Y and Z and towards the blank in X (the tail-end clamp's box reaches X = +s; the nose-end clamp's box starts at 10·L_r − 2·v_o − s).
* Beyond-the-clamps zones: X < −L_c and X > 10·L_r − 2·v_o + L_c (positions there cannot be machined).

*Original naming:* the original calls the X ≤ 0 clamp the "nose" clamp and the far one the "tail" clamp (it notes the mix-up itself); behaviour is as above. *Original deviation:* the far clamp's safety box is shifted by −s instead of enlarged (its outer end is short by s) (C-28).

#### 9.2.4 Collision handling (original, informative)

Tested on the tool tip after compensation, before the machine offset, with the tool body of §8.9 (with C-17):

* A position in a beyond-the-clamps zone is silently dropped (no output, no retract) (C-26).
* A position colliding with a clamp safety box is replaced by: (first collision of a run, if a previous good position exists) a vertical lift from the previous good position to z_c, then the position (X_e, Y, z_c) compensated with a horizontal normal, where X_e = s for the X ≤ 0 clamp and 10·L_r − 2·v_o − s for the other, and z_c = top of the X ≤ 0 clamp's safety box + 25 mm.
* The first good position after a run: a horizontal move at the lifted height to above it, then the move down to it.

### 9.3 Rotating board system (4-axis)

Geometry and placement as §9.2. The clamps rotate with the blank about the A axis. Required: test collisions in the board's (unrotated) frame, or equivalently treat each clamp as the solid of revolution of its safety box about the axis (radius √((W_c/2 + s)² + (H_c/2 + s)²)) over its X range. *Original:* the rotating-board cutter never reports a clamp collision (C-27).

### 9.4 Obstacle avoidance (required)

For every output position (and along every straight move, sampled as in §8.8) test the tool body (§8.9) against the obstacles of §9.1.6 / §9.2.3 / §9.3. For a maximal run of consecutive colliding positions:

1. From the last good position, retract vertically (G00) to Z_safe (§10.5).
2. Traverse at Z_safe (G00) to above the next good position.
3. Descend vertically to that position (G00 down to 5 mm above it, then G01 at the plunge feed).
4. The skipped positions are not machined. Add a comment line naming the X range of the skipped stretch and report it (§11).

Positions in a beyond-the-clamps zone are always skipped this way. The tool body MUST never intersect an obstacle, including during retracts and traverses.

---

## 10. Program output

### 10.1 Dialect

RS274/NGC as accepted by LinuxCNC (EMC2). Plain ASCII text, one block per line, lines terminated by "\n". Numbers use "." as decimal separator irrespective of locale (the original forces a US locale for this).

### 10.2 Words and line formats

| Purpose | Exact text |
|---|---|
| comment | `(` text `)` — one line per line of the text |
| metric units | `G21` |
| absolute coordinates | `G90` |
| spindle (tool) on | `M3` |
| spindle off | `M5` |
| program end | `M2` |
| dwell | `G04 P` seconds with 6 decimals, e.g. `G04 P2.000000` |
| feed | `F` integer, on its own line, e.g. `F4800` |
| linear move | `G01 ` followed by coordinate words (only when switching into G01; afterwards bare coordinate lines) |
| rapid move (new) | `G00 ` followed by coordinate words |
| coordinates, 3 axes | `X%.4f Y%.4f Z%.4f`, e.g. `X1234.5000 Y-12.3400 Z45.0000` |
| coordinates, 4 axes | `X… Y… Z… A…` with 4 decimals each, single spaces |
| Z only | `Z%.4f` |
| home | `G28` (*original:* `G28 ` with a trailing space) |
| home reference | `G28.1` (*original:* with a trailing space) |

Number formatting: fixed 4 decimals; required: print "-0.0000" as "0.0000". F is the truncated integer value.

### 10.3 Comments

Text of a comment MUST NOT contain "(" or ")" (replace by "[" and "]") and SHOULD contain only printable ASCII (replace others by "?"); multi-line text is split into one comment per line.

### 10.4 Program layout (surface splits)

Required order:

1. Header comments:
   * `(Generated with BoardCAD v2.0)` (or the implementation's own product string),
   * `(Deck cut)` or `(Bottom cut)`,
   * `(Board: <file name> <length> x <width> x <thickness>)` (lengths in the user's unit),
   * `(Blank: …)` — same description of the blank, or `(Blank: none)`,
   * `(Cutter: …)` — e.g. `(Cutter: Bullnose cutter Diameter:2.54 cm, Corner radius:0.50 cm, Height:6.50 cm)`,
   * `(Holding system: …)` — the type, its dimensions, and the board and blank placement (offset x, y, z and angle) of this program,
   * one comment per limitation applied (§5.13) or file problem.
2. `G21`, `G90`.
3. Before-cut action for this program:
   * Do nothing — nothing;
   * Home — `G28`;
   * Home Ref. — `G28.1` (in LinuxCNC this stores the current position as the G28 position, so an after-cut action "Home" then returns the machine to where it was when the program started);
   * Lift clear — `G00 Z<z>` with z = machine-frame Z of the program's first tool position + 10·ClearHeight (scaled as Z);
   * Script — the text of the script file inserted verbatim (all lines, each followed by "\n"); if it cannot be read, the comment `(Failed to read pre cut script <file>, Error: <message>)`.
4. `M3`, then `G04 P2.000000` (2 s spindle run-up).
5. Approach (§10.5) and the toolpath.
6. Retract to Z_safe (`G00 Z…`).
7. `M5`.
8. After-cut action (as 3; Lift clear uses the last tool position + clear height, but not lower than Z_safe; the script error comment says "post cut").
9. `M2`.

*Original deviations:* the before-cut action is written before G21/G90 (C-10); "Lift clear" writes a value computed from the board's centreline height in cm plus the clear height in cm, untransformed, as if it were millimetres, with `G01` and no feed (C-10); script files are never actually read — the text is lost and the program gets an empty script, and an error yields an empty comment (C-09); the spindle is switched off before the final lift; the header comments contain untranslated placeholders (shown as `!KEY!`) and an empty board description, and the holding-system text loses the blank part (C-24); generation fails altogether when no blank is loaded (C-25).

### 10.5 Motion rules

* **Z_safe** (new): the maximum of (highest tool-tip Z of the program, top of every obstacle) + Retract clearance, in the machine frame, computed after the toolpath has been generated.
* **Approach** (§5.8): `G00 Z<Z_safe>`; `G00 X<x> Y<y>` of the approach point; `G00 Z<z>` of the approach point (if a blank is loaded and its upper surface at that XY is higher than approach Z − 5 mm, the approach Z is raised to blank surface + 5 mm); `F<plunge>`; `G01 X Y Z` of the first cutting position.
* **Cutting**: all toolpath positions with G01. Before a position whose feed differs (as an integer) from the current one, write `F<n>` on its own line. Every program MUST set F before its first G01 (C-02).
* **Duplicates**: a position equal (exactly, before offset and scaling) to the previous output position is not written.
* **Retracts/traverses** (new): only vertical moves up to Z_safe, moves at Z_safe, and vertical descents to ≥ 5 mm above the next position may be G00 (§9.4).
* *Original:* no G00 at all; the very first move is a `G01` to the approach point with no F word ever written in that file before it (C-02); feeds are not reset between the deck and the bottom file, so the bottom file may have no F word at all if its first speed equals the deck's last (C-02).

### 10.6 Axis scaling

Output words: X = ScaleX·q_x, Y = ScaleY·q_y, Z = ScaleZ·q_z, A = ScaleA·A_deg, F = trunc(F_mm/min · ScaleF). Each factor is applied exactly once. *Original:* X, Y, Z are multiplied by the scale twice for 3-axis output (C-01); A and F scales are never applied (C-02b, C-03); the "Lift clear" Z word is never scaled.

### 10.7 Program layout (other generators)

Width splits and hot-wire programs (plain writer, no scaling):

```
(Generated with BoardCAD)
G21
G90
M3
G01 F<n>                      -- original: the "G01 " prefix is completed by the first feed word
X… Y… Z…
…
M5
M2
```

Required for width splits (milling): the approach and retract rules of §10.5. Hot-wire programs: "M3"/"M5" switch the wire heat; no approach point (the wire starts at the first point); F = dialog speed.

### 10.8 A-axis words

Written only for 4-axis machines, on every position line (`X… Y… Z… A…`). *Original:* the A value is computed but dropped before writing (C-03).

### 10.9 Atua cores file format

Plain text: header lines as in §7.3 written verbatim (no comment delimiters), then the column header line `x` + 30 spaces + `y`, then one line per sample: `%.8f` + 18 spaces + `%.8f` (the normalised x and the normalised second coordinate — z for the profile, the half-width for the outline). No preamble, no postamble, no feed.

### 10.10 Example (default settings, 3-axis, supports; abbreviated)

Original (with its defects):

```
(Generated with BoardCAD v2.0)
(!DECKCUT_STR!)
(board)
(Blank)
(!CUTTER_STR!Bullnose cutter Diameter:1", Corner radius:3/16", Height:2 1/2", )   <- lengths in the user's unit, here inches
(!BLANKHOLDINGSYSTEM_STR!Board offset: <x>,<y>,<z> Angle:<rad>)                    <- six-decimal numbers
G28 
G21
G90
M3
G04 P2.000000
G01 X… Y… Z…          <- approach point, no F yet
F762                   <- plunge: trunc(0.05·254·60)
X… Y… Z…               <- first stringer point
F15240
X… Y… Z…
…
M5
G28 
M2
```

Required:

```
(Generated with BoardCAD v2.0)
(Deck cut)
(Board: …)
(Blank: …)
(Cutter: …)
(Holding system: …)
G21
G90
G28
M3
G04 P2.000000
G00 Z<Z_safe>
G00 X… Y…
G00 Z…                 <- approach point
F762
G01 X… Y… Z…
F15240
X… Y… Z…
…
G00 Z<Z_safe>
M5
G28
M2
```

---

## 11. Progress, statistics and time estimate

*Original:* no machining-time estimate and no statistics. A progress dialog ("Generating toolpath", "Working... n% Done.") shows n = trunc(100·(passes completed)/(total)), with total = 4 + 2·(DeckCuts + DeckRailCuts + DeckCleanUpCuts) + 2·(BottomCuts + BottomRailCuts + BottomCleanUpCuts) (one stringer pass and one extra unit per program); passes are counted whenever a pass or cut-off sequence ends regardless of the enabled options, so the percentage is approximate and can exceed 100. A 3-D view shows the surface samples, their normals and the tool positions.

Required: the progress indication (any accurate measure). Recommended (new, not written into the program unless the user asks): per program, the number of passes, output positions, positions raised by §8.8, skipped stretches (§9.4), total feed-path length and rapid-path length, and an estimated time = Σ(segment length / segment feed) + rapid length / rapid rate (rapid rate a user value, default 5000 mm/min).

---

## 12. Legacy script interface (out of scope)

The original also contains an older, script-driven path (a properties file with machine data, NURBS-based deck/bottom scripts, a scanner import, STL collision cutters and 4-/5-axis tool rotations). It is not reachable from the machining dialog described here and is out of scope. Background only: in those scripts the A axis is expressed in degrees, which supports §4.6.

---

## 13. Known defects of the original and intended behaviour

| Id | Area | Original behaviour | Required behaviour |
|---|---|---|---|
| C-01 | output scaling | X, Y, Z of 3-axis positions are multiplied by the axis scale twice (scale²). | Apply each scale once (§10.6). |
| C-02 | feeds | The first G01 of every program (to the approach point) has no F word; the current feed is not reset between the deck and bottom files, so the bottom file may contain no F at all. | Write F before the first G01 of each program; approach with G00 (§10.5). |
| C-02b | feed scale | ScaleF is never applied. | F = trunc(feed · ScaleF). |
| C-03 | A axis | The rotation angle is computed (in radians) and then dropped; positions are written with X Y Z only; ScaleA unused. | Write `A` in degrees, scaled, unwrapped, with bounded steps (§8.7). |
| C-04 | bullnose flat zone | The "nearly flat" approximation compares an angle in radians with 8 (meant as 8°), so the lengthwise branch never applies; z is not compensated in the flat zone. Gouges up to ≈ 0.25 mm at small tilts and ≈ 0.8 mm on 30° rocker slopes with the default tool; cut-off moves are shifted by R_f instead of D/2. | Exact torus compensation (§8.2); optional smoothing only with gouge verification. |
| C-05 | normals | Stringer passes (deck and bottom) and outline passes use profile-curve normals that point into the board (right-hand perpendicular of a tail→nose curve). The bullnose then puts the tool ≈ 2r too deep on stringer passes wherever the centreline is not horizontal in the machine, and D/2 inside the outline on outline passes; sandwich compensation acts outward on these passes. (Determined by analysis of the angle conventions; see §14.) | Outward normals for all passes (§5.4.1, §5.4.5). |
| C-06 | bottom coverage | The bottom band's start is requested as normal angle 360°, which the surface lookup cannot match; the fallback ignores the lengths of preceding segments, so the bottom passes may start away from the centreline, leaving a strip (flat/concave bottoms) covered only by the stringer pass. | Bottom band starts exactly at the bottom centre (§5.3). |
| C-07 | 3-axis rail gouge | Default DeckRailAngle 130° (and any band beyond 90°) sends 3-axis passes past the rail apex; every such position cuts through the rail from above. | Limit deck bands to ≤ 90° and bottom bands to ≥ 90° on 3-axis machines, with a warning (§5.13); gouge verification (§8.8). |
| C-08 | gouge check | The cutter-versus-board check always answers "no collision"; no gouge protection at all, including straight linking moves (outline/clean-up/side-change starts). | Drop-cutter verification of positions and moves (§8.8). |
| C-09 | scripts | Script files are read line by line but the text is discarded (string concatenation result ignored); the program receives an empty script; on error the comment is empty. | Insert the file text; on error write the explanatory comment (§10.4). |
| C-10 | lift clear | Before-cut actions are written before G21/G90; "Lift clear" writes board-frame cm values (centreline height + clear height) as machine mm, unplaced, unscaled, with G01 and no F; the after-cut lift happens after M5 and uses the last *surface* point. | §10.4 items 2–3 and 6–8. |
| C-11 | approach | The approach to the first point is a G01 from wherever the machine is, without F. | G00 approach via Z_safe (§10.5). |
| C-12 | bottom clean-up | Bottom clean-up point is flipped but its normal is not. | Flip both (§5.4.6). |
| C-13 | tail cut-off | On the bottom side the "lift" moves of the cut-off go down through the board (sign applied twice). | Lift upward in the side frame (§5.4.7). |
| C-14 | sequencing | Nose-start deck with nose cut-off starts the stringer pass at the far end; nose-start bottom never runs the tail cut-off; tail-start bottom with tail cut-off runs it twice, the second time after a full-length move. | §5.6 sequences; no lengthwise traverses. |
| C-15 | side change | With stay-away the deck and bottom use different temporary lateral positions (−2.5·W vs −0.5·W) before the stay-away rule moves them anyway. | Harmless; keep or unify. |
| C-16 | sandwich | Deck rail-limit setting writes the bottom rail limit; left-side rail distances use signed y; deck clean-up passes get rail instead of deck compensation. | §5.11. |
| C-17 | tool height | Tool cylinder height for clamp collisions uses cm as mm (6.5 mm instead of 65 mm). | Unbounded upward cylinder (§8.9). |
| C-18 | STL cutter (dialog) | Transformed-vertex arrays are never created when the STL cutter is selected in the dialog; compensation and scaling fail. | Working STL cutter (§8.6). |
| C-19 | STL parsing/scaling | Case-sensitive keywords, last vertex ignored, 10000-vertex limit, cumulative re-scaling on every scale change, facet-normal vertex selection. | §8.6 (support-function contact, scale from file values). |
| C-20 | 4-axis stay-away | Stay-away applied in the rotated frame for every rotation. | Only for |A| ≤ 45° (§8.4). |
| C-21 | width splits | No placement transform (fails), deck/bottom normals swapped, feeds in mm/s, bottom not flipped, two concurrent writers. | §6. |
| C-22 | hot-wire thickness | Additional thickness added 10× too large. | Add the thickness in cm before scaling (§7.1). |
| C-23 | Atua thickness | "Required thickness" is meaningless. | §7.3 (interpretation). |
| C-24 | header comments | Untranslated placeholders (`!KEY!`), empty board description, holding-system text missing the blank part. | §10.4 item 1. |
| C-25 | no blank | Generation fails if no blank file is loaded. | Allowed; placement per §9.1.3 "without a blank". |
| C-26 | beyond clamps | Positions beyond the clamp ends are dropped silently; the next good position is reached by a straight feed move at cutting depth. | Retract–traverse–descend (§9.4). |
| C-27 | rotating-board collisions | The rotating-board cutter never reports clamp collisions. | §9.3. |
| C-28 | clamp safety box | The far clamp's safety box is shifted rather than enlarged. | Enlarged box (§9.2.3). |
| C-29 | supports collisions | Supports and tail stop are never checked. | Obstacles of §9.1.6, avoidance §9.4. |
| C-30 | .brd write-back | Regular speed written twice, the rail speed wins. | §3.13. |
| C-31 | .brd prefill | Zero-valued board-file fields overwrite settings (speeds 0 → F0; cuts 0 → empty programs). | §3.13. |
| C-32 | collision handler | If a holding system reports a collision but returns no replacement path, processing crashes (only reachable with a generic holding system). | Treat as §9.4. |

---

## 14. Points that could not be fully determined

1. **Surface-model internals** (girth lookup by normal angle, section blending, finite-difference normals) are specified in `geometry.md` §4.7 and §8; this document only uses them through the band point/normal contract of §2.
2. **Inverted normals (C-05)** were established by analysing the tangent/normal angle conventions (consistent with `geometry.md` §1.5 and §4.3), not by running the original. If an implementer finds evidence that the original's profile normals point outward, C-05 does not change the required behaviour (outward normals), only the description of the original.
3. **Purpose of the nose/tail cut-off** is inferred from the geometry of the moves (notches beside the stringer near the tip).
4. **End-clamp defaults** (clamp length 80 cm, height and width 20 cm, safe zone 5 cm, overlap 8 cm, vertical offset 10 cm) are stored as centimetres; they may have been intended as millimetres. They are kept as specified.
5. **Speed units**: the speeds are treated as mm/s because the original multiplies by 60 "to mm/min"; the defaults (80 and 254 mm/s, i.e. 4800 and 15240 mm/min) are kept although the stringer/rail/outline defaults are faster than the general one.
6. **"To bottom" outline depth** uses the BottomAngle point, which equals the apex with the default 90°; the intended "tuck" angle is unknown.
7. **"Home Ref."** emits G28.1, which in LinuxCNC stores (not visits) a reference position; whether that was intended could not be confirmed. It is kept.
8. **Hot-wire profile third segment** (re-tracing part of the deck to close the loop) was apparently intended for the last 1/m of the deck curve but is never produced; the closed two-curve loop is specified.
9. **Atua "Required thickness"** intended formula is an interpretation.
10. **Support and tail-stop geometry** (support length along X, tail-stop height) are not defined in the original (except the 100 mm tail-stop height in its 3-D model); the Safety defaults are new choices.
11. **STL units and orientation** (mm, tip at origin, axis +Z) are inferred from how the vertices are used.
12. **A-axis sign and location**: the rotary axis is assumed to coincide with the reference X axis (the clamp axis) and positive A to follow the right-hand rule about +X; machines with the opposite convention use ScaleA = −1.

---

## Appendix A. Worked numbers for the default configuration

Default tool: D = 25.4 mm, r = 5 mm, R_f = 7.7 mm, H = 65 mm.

* Board L = 182.88 cm, Δ = 1 cm → L' = 182.78, N = 183; band pass = 5 + 184 samples; stringer pass = 184 samples.
* Deck: stringer (stringer feed 15240 mm/min, plunge 762), 28 deck-band passes per side at s = 1/28 … 1 between the deck centre and the 28° point (4800 mm/min), 7 rail passes per side between 28° and min(130°, 90°) = 90° on 3-axis (15240), 1 outline pass per side at the apex height (15240), 2 clean-up passes per side at z_b and z_b + 1 cm (4800), side change of 11 samples.
* Bottom: start at nose; stringer nose → tail; 26 bottom-band passes per side between the bottom centre and the apex; no bottom-rail passes; outline pass; 2 clean-up passes 1 cm and 2 cm outside the apex.
* Stay-away: tool axis kept at |Y| ≥ 12.7 + 5 = 17.7 mm; 10 mm strip left on the centreline.
* Supports: X_T = 100 cm, X_1 = 160 cm, X_2 = 260 cm, h_1 = 26 cm, h_2 = 30 cm, α_s = atan2(−4, 100) ≈ −2.29° (nose up).
* Torus check: normal tilted 30° sideways → tip = P + (7.7 + 2.5)·ĥ − 0.67·ẑ mm.

## Appendix B. Constants

| Constant | Value |
|---|---|
| tip margin of passes | 0.1 cm at each end (L' = L − 0.1) |
| transition steps between band passes | 5 |
| side-change steps | 10 (11 samples) |
| side-change lift (stay-away) | 5.0 cm |
| approach height | 5.08 cm (2 in) above the first sample |
| plunge feed | 5 % of the pass feed |
| spindle dwell after M3 | 2 s |
| cut-off reference stations | x = L − 1.0 cm (nose), x = 1.0 cm (tail) |
| cut-off lift | thickness + 5.0 cm |
| cut-off stringer approach | D/2 + 0.1 cm from the centreline |
| clean-up step | 1.0 cm (deck: upward from z_b; bottom: outward) |
| flat-zone angle (original approximation) | 8° |
| mm per cm | 10 |
| clamp lift (original) | clamp safety-box top + 25 mm |
| width-splits divisions / step | 30 / 1.0 cm |
| hot-wire / Atua samples per curve | 5001 (tt = j/5000) |
| foot (for "max deck within a foot") | 30.48 cm |
| support line in placement (original) | 10 iterations (deck), 5 (bottom) |
| manual rotation step | 0.004 rad (0.0004 with Alt) |
