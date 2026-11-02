# File formats — clean-room specification

Status: normative for the new implementation. Describes every file format that
BoardCAD 3.0.1 ("the original") reads or writes, the exact behaviour of the
original where interoperability depends on it, and the corrected behaviour the
new implementation must have where the original is defective.

Conventions used in this document:

* "MUST", "SHOULD", "MAY" have their RFC 2119 meaning.
* "Original" = observed behaviour of BoardCAD 3.0.1. "Required" = what the new
  implementation does. Where they differ, the reason is given and the item is
  repeated in [section 12](#12-known-defects-of-the-original).
* Grammar is written in EBNF (ISO 14977 flavour): `"x"` literal, `[ ]` optional,
  `{ }` zero or more, `( | )` alternatives, `NL` = line terminator.
* Examples are file content, not code.

## Table of contents

1. [Common board model](#1-common-board-model)
2. [Number formatting shared by the text formats](#2-number-formatting-shared-by-the-text-formats)
3. [`.brd` — native board file](#3-brd--native-board-file)
4. [Legacy encrypted `.brd`](#4-legacy-encrypted-brd)
5. [Partial curve files `.otl`, `.pfl`, `.crs`](#5-partial-curve-files-otl-pfl-crs)
6. [STEP (`.stp` / `.step`, "BCstep 1.0")](#6-step-stp--step-bcstep-10)
7. [Shape3D design `.s3d`](#7-shape3d-design-s3d)
8. [`.srf` binary board](#8-srf-binary-board)
9. [`.cad` legacy NURBS binary](#9-cad-legacy-nurbs-binary)
10. [Export-only formats](#10-export-only-formats)
11. [File-type dispatch in the original](#11-file-type-dispatch-in-the-original)
12. [Known defects of the original](#12-known-defects-of-the-original)
13. [Undetermined points](#13-undetermined-points)

---

## 1. Common board model

All board formats are read into, and written from, one in-memory model. This
section defines only what the file formats need.

### 1.1 Units and axes

* Internal length unit: **centimetre**. All `.brd`, `.otl`, `.pfl`, `.crs`
  numbers are centimetres. STEP uses millimetres, `.srf` metres, DXF exports
  metres, `.s3d` is taken as centimetres without conversion.
* Board length axis **x**: 0 at the tail, increasing towards the nose.
* **Board length L** := the largest x among the end points of the outline
  knots (not the handle points). Several formats use L.

### 1.2 Curves and knots

A *curve* is an ordered list of *knots*; consecutive knots are joined by one
cubic Bezier segment. A knot carries:

| Item | Meaning |
|---|---|
| end point **E** | the point the curve passes through |
| previous handle **P** | control point governing the segment *arriving* at E (for knot *i*, the third control point of segment *i−1*) |
| next handle **N** | control point governing the segment *leaving* E (second control point of segment *i*) |
| continuous flag | boolean; true means P, E, N are meant to be collinear (tangent-continuous / "smooth" knot); false means a corner is allowed |
| auxiliary flag | second boolean. The original stores and round-trips it but never interprets it; always `false` in every file seen. MUST be preserved. |

Segment *i* (between knot *i* and knot *i+1*) has control points
E<sub>i</sub>, N<sub>i</sub>, P<sub>i+1</sub>, E<sub>i+1</sub>.
The P of the first knot and the N of the last knot do not influence geometry
but are stored and MUST be round-tripped by formats that carry them.

### 1.3 The four kinds of curve

| Curve | Plane, x | Plane, y | Typical shape |
|---|---|---|---|
| Outline | board x (cm from tail) | half-width (≥ 0) | starts at (0,0), ends at (L,0) |
| Bottom ("rocker") | board x | height of the bottom at the centreline | lowest point at height 0 |
| Deck | board x | height of the deck at the centreline | first knot equals the bottom's first knot (tail), last knot equals the bottom's last knot (nose); the deck's own first/last segments are near-vertical tail/nose edges |
| Cross-section | lateral distance from centreline (≥ 0) | height above the bottom at the centreline | first knot at the centre bottom (0,0), last knot at the centre deck (0, t) |

Each cross-section additionally has a **position** (board x, cm).

### 1.4 Zero-size end sections

The list of cross-sections is ordered by position. By convention its **first
and last entries are dummy "end sections"** at position 0 and position L. Each
consists of exactly one knot whose E, P and N are all (0,0) and whose flags are
both false. They are not editable and only anchor interpolation at the tips.
The original identifies them purely by list index (first and last); the
"current section" index of the editor ranges over the inner entries only.

### 1.5 Guide points

Each curve (outline, bottom, deck, every cross-section) owns an ordered list of
*guide points*: 2-D reference points in the same plane as the curve, used only
for display. They have no influence on geometry and MUST be round-tripped.

### 1.6 Metadata

Text fields, integers, reals, booleans and fixed-size real arrays listed in
[3.6](#36-field-table). Their defaults are the "Default" column there.

### 1.7 Continuity normalisation after import

Several readers finish by re-checking each knot's continuous flag against its
geometry. The test, for one knot:

* Let **p** = P − E and **n** = N − E.
* Let α = angle between **p** and the unit vector (0, 1) (range 0…π, computed as
  arccos of the normalised dot product); α := 0 if **p** has zero length.
* Let β = the same for **n**.
* The knot is *geometrically continuous* iff | (π − α) − β | < 0.02 (radians,
  ≈ 1.15°).

Two modes are used:

* *demote-only* (after `.brd`, `.s3d` load, and after partial-curve import):
  if the knot is not geometrically continuous, its flag becomes false;
  otherwise the flag is left unchanged.
* *both ways* (after `.srf` load): the flag becomes exactly the result of the
  test.

The normalisation applies to the outline, bottom, deck and every cross-section
(including end sections, whose single knot always ends up false).

Note (defect D-29): the test only compares angles to the y axis and therefore
cannot tell a handle pointing to the upper-left from one pointing to the
lower-left mirrored; e.g. **p** = (−1, 1), **n** = (−1, −1) passes although
the knot is a cusp. Required: apply the test above and, when both handles have
non-zero length, additionally require **p** and **n** to point in opposite
directions (2-D cross product ≈ 0 relative to the lengths, i.e. sine of the
angle between them below 0.02, and dot product < 0). For zero-length handles
keep the original rule so that end knots normalise exactly as in the original.

---

## 2. Number formatting shared by the text formats

The original writes every real number in the `.brd` family, in STEP and in DXF
with the Java platform's default double-to-text conversion. To produce files
byte-identical to the original, a writer MUST reproduce it:

* NaN → `NaN`; infinities → `Infinity` / `-Infinity`; negative zero → `-0.0`.
* If 10<sup>−3</sup> ≤ |v| < 10<sup>7</sup>: plain decimal notation, sign if
  negative, integer part without leading zeros (a single `0` if < 1), a `.`,
  then the fraction digits; at least one fraction digit (so `5` is written
  `5.0`, `100.0` is `100.0`).
* Otherwise: computerised scientific notation: one non-zero digit, `.`, at least
  one further digit, `E`, exponent in decimal with a `-` sign when negative and
  no `+` sign. Examples: `-9.617417041187615E-4`, `1.0E7`, `1.0804142968407003E-14`.
* Digits: the shortest decimal digit string that rounds back to the same binary
  double (a standard shortest-round-trip algorithm such as Ryū or Grisu-exact
  produces the right digits; then lay them out as above).

Integers are written in plain decimal without sign for non-negative values.
Booleans are `true` / `false`.

Readers MUST accept any decimal real syntax: optional sign, digits with optional
fraction, optional exponent with `e` or `E` and optional sign; SHOULD accept
`NaN`/`Infinity`; MUST ignore surrounding spaces/tabs.

---

## 3. `.brd` — native board file

### 3.1 Physical level

* Plain text. The original reads and writes with the operating system's default
  character set (Windows-1252 on Windows, UTF-8 on most Unix/macOS JVMs), which
  makes non-ASCII text non-portable (defect D-12).
  Required: write **UTF-8 without BOM**. Read: if the bytes are valid UTF-8 use
  UTF-8, otherwise decode as Windows-1252. A leading UTF-8 BOM MUST be skipped.
* Line terminator written: a single LF (0x0A) on every platform. Readers MUST
  accept LF, CR LF and lone CR.
* A file whose first line begins with `%BRD-` is an encrypted file, see
  [section 4](#4-legacy-encrypted-brd).

### 3.2 Grammar

```
brd-file        = { line } ;
line            = scalar-line | curve-block | sections-block | ignorable-line ;

scalar-line     = key sep value NL ;
key             = "p" digit digit ;               (* always exactly two digits *)
sep             = " : " ;                         (* space, colon, space *)
value           = { any-char-except-NL } ;

curve-block     = curve-key sep "(" NL
                  { knot-line }
                  [ guide-block ]
                  ")" NL ;
curve-key       = "p32" | "p33" | "p34" ;         (* outline | bottom | deck *)

sections-block  = "p35" sep "(" NL
                  { section-block }
                  ")" NL ;
section-block   = "(p36 " real NL
                  { knot-line }
                  [ guide-block ]
                  ")" NL ;

knot-line       = "(cp [" real "," real "," real "," real "," real "," real "] "
                  bool " " bool ")" NL ;
guide-block     = "gps : (" NL { guide-line } ")" NL ;
guide-line      = "(gp [" real "," real "])" NL ;

bool            = "true" | "false" ;
real            = see section 2 ;
ignorable-line  = (* anything else: see 3.8 *) ;
```

Key/value lines are recognised by position: characters 1–3 are the key, the
value is everything from character 7 to the end of the line (i.e. after the
3-character key and the 3-character separator), with leading and trailing white
space removed.

### 3.3 Value syntax per field type

| Type | Written as | Read as (required) |
|---|---|---|
| real | [section 2](#2-number-formatting-shared-by-the-text-formats) | any real syntax |
| int | decimal integer, e.g. `200` | decimal integer; required readers SHOULD also accept a real with zero fraction (`3.0`) and round it; the original aborts the whole load on `3.0` (defect D-13) |
| bool | `true` / `false` | case-insensitive `true` → true; anything else → false (original behaviour, keep it) |
| text | raw text, with every LF in the value replaced by the two characters `\` `n` | see escaping below |
| real array | `[` r `,` r … `]`, no spaces, e.g. `[140.0,0.0,25.0]` | strip the outer brackets, split on `,`, trim each element |

**Text escaping.** The original writer replaces each LF inside any text value by
the two-character sequence `\n` (backslash, `n`) and writes nothing else
specially; it does not escape backslashes and does not touch CR. The original
reader converts `\n` back to LF **only for p49 (comments)**; all other text
fields keep the literal backslash-n (defect D-03). A raw CR in a value
splits the line on reading (defect D-04).

Required:
* Writer: normalise CR LF and lone CR in values to LF, then replace each LF by
  `\n`. Do not escape anything else (keeps files readable by the original).
* Reader: convert `\n` to LF in **every** text field.
* Leading/trailing white space of text values is lost (values are trimmed on
  reading). This is accepted behaviour; writers SHOULD trim before writing so
  that a round trip is stable.

**Arrays.** Each array field has a fixed capacity (3 or 9). The original stores
element *i* of the file into slot *i*; extra elements beyond capacity abort the
load (defect D-14); missing elements keep their default (0). Required: fill
slots from the file, keep defaults for missing slots, ignore extra elements
with a warning.

### 3.4 Curve blocks and control-point lines

`p32`, `p33`, `p34` introduce the outline, bottom and deck. The block contains
one `(cp …)` line per knot, in curve order (tail to nose for these three
curves), optionally a guide-point block, and a closing `)` line.

The six reals of a knot line are, in order:

| # | Meaning |
|---|---|
| 1, 2 | E.x, E.y — end point |
| 3, 4 | P.x, P.y — previous handle |
| 5, 6 | N.x, N.y — next handle |

followed by the continuous flag and the auxiliary flag. All values are absolute
coordinates (not offsets from E).

Example (outline, from a real board):

```
p32 : (
(cp [0.0,0.0,0.0,0.0,0.0,3.05804454721486] false false)
(cp [1.396916196324043,5.7737301929790315,0.09199389838430455,4.5383224190388285,7.42679238696713,11.482388810874316] true false)
(cp [90.13362308681752,23.49499750470692,30.390000282383603,23.502663416291362,129.08124632569087,23.490000000000002] true false)
(cp [187.96,0.5,165.39140765851977,16.729079863293684,187.96215325905928,0.4806433299508672] false false)
(cp [187.96,0.0,187.96,0.02097709562634233,187.96,0.0] false false)
)
```

The original's knot-line parser is strict (all of these are required for the
original to read a line, so a writer MUST produce exactly this layout):
the line starts with `(cp`, its last character is `)`, tokens are separated by
single spaces, the bracketed number list contains no spaces, and there are
exactly four space-separated tokens (`cp`, list, flag, flag). A malformed knot
line makes the original silently stop reading that curve (defect D-10).
Required reader: accept arbitrary runs of spaces/tabs between tokens and inside
the list, trailing white space, and report a malformed knot line as an error
(with line number) rather than truncating silently.

Guide-point lines carry absolute (x, y) in the curve's plane.

### 3.5 Cross-sections block

`p35` opens the list of cross-sections. Each section starts with a line
`(p36 ` followed by the position (real, cm) and ends with its own `)` line. The
original reads the position as the text between the first and the second space
of that line (or end of line), so anything after a second space is ignored.
Inside a section: knot lines from the centre bottom outwards, over the rail, to
the centre deck; then an optional guide block.

The zero-size end sections are written as:

```
(p36 0.0
(cp [0.0,0.0,0.0,0.0,0.0,0.0] false false)
)
```
at the start, and the same with position L (e.g. `(p36 187.96`) at the end.

Example of a complete block with one inner section carrying guide points
(abridged knot values):

```
p35 : (
(p36 0.0
(cp [0.0,0.0,0.0,0.0,0.0,0.0] false false)
)
(p36 92.71
(cp [0.0,0.0,-7.27,0.0,7.27,0.0] true false)
(cp [23.29,0.14,15.04,0.0,23.85,0.33] true false)
(cp [0.0,6.09,7.0,6.09,-7.0,6.09] true false)
gps : (
(gp [20.49624252319336,-0.21735146641731262])
(gp [12.932412147521973,-0.13041087985038757])
)
)
(p36 187.96
(cp [0.0,0.0,0.0,0.0,0.0,0.0] false false)
)
)
```

### 3.6 Field table

"R" = read by the original, "W" = written by the original. "Use" says whether
the original interprets the value (beyond storing it). Type abbreviations from
[3.3](#33-value-syntax-per-field-type). Defaults are the values a new board has
and the values a reader MUST use for a field absent from the file.

| Code | Meaning | Type | Default | R | W | Use / notes |
|---|---|---|---|---|---|---|
| p01 | Board length L | real | — | no | yes | derived: L (1.1) |
| p02 | Length over the bottom curve | real | — | no | yes | derived: arc length of the bottom curve |
| p03 | Thickness | real | — | no | yes | derived: deck height − bottom height, both evaluated at x = L/2 |
| p04 | Width | real | — | no | yes | derived: 2 × outline y at x = L/2 |
| p05 | Nose rocker | real | — | no | no | legacy, never produced |
| p06 | Tail rocker | real | — | no | no | legacy, never produced |
| p07 | Format/version string | text | `V4.4` | yes | yes | stored only |
| p08 | Board name | text | empty | yes | yes | display |
| p09 | Author | text | empty | yes | **no** | defect D-01 |
| p10 | Blank file (path of the foam blank file used for machining) | text | empty | yes | **no** | CAM prefill; defect D-01 |
| p11 | Number of deck ("top") cuts | int | 0 | yes | yes | CAM prefill |
| p12 | Number of bottom cuts | int | 0 | yes | yes | CAM prefill |
| p13 | Number of rail cuts | int | 0 | yes | yes | stored |
| p14 | Cutter diameter | real | 0 | yes | yes | stored (default boards use 25.4) |
| p15 | Blank pivot | real | 0 | yes | yes | stored |
| p16 | Board pivot | real | 0 | yes | yes | stored |
| p17 | Maximum angle (deck-rail cut angle, degrees) | real | 0 | yes | yes | CAM prefill |
| p18 | Nose margin | real | 0 | yes | yes | stored |
| p19 | Nose length | real | 0 | yes | yes | stored |
| p20 | Tail length | real | 0 | yes | yes | stored |
| p21 | Delta-X nose | real | 0 | yes | yes | stored |
| p22 | Delta-X tail | real | 0 | yes | yes | stored |
| p23 | Delta-X middle | real | 0 | yes | yes | stored |
| p24 | Speed towards tail | int | 0 | yes | yes | stored |
| p25 | Stringer speed | int | 0 | yes | yes | CAM prefill |
| p26 | Regular speed | int | 0 | yes | yes | CAM prefill |
| p27 | Strut (support) 1: [position, height, third value] | real[3] | [0,0,0] | yes | yes | CAM prefill uses elements 0 and 1 |
| p28 | Strut (support) 2: same layout | real[3] | [0,0,0] | yes | yes | as p27 |
| p29 | Cutter start position (x,y,z) | real[3] | [0,0,0] | yes | yes | stored |
| p30 | Blank tail position (x,y,z) | real[3] | [0,0,0] | yes | yes | stored |
| p31 | Board start position (x,y,z) | real[3] | [0,0,0] | yes | yes | stored |
| p32 | Outline curve | block | empty | yes | yes | 3.4 |
| p33 | Bottom (rocker) curve | block | empty | yes | yes | 3.4 |
| p34 | Deck curve | block | empty | yes | yes | 3.4 |
| p35 | Cross-sections | block | empty | yes | yes | 3.5 |
| p36 | Cross-section (only inside p35) | sub-block | — | yes | yes | 3.5 |
| p38 | Display unit code | int | 0 | yes | yes | stored only; default boards contain 2 |
| p39 | Nose rocker at one foot from the nose | real | 0 | yes | **no** | defect D-01 |
| p40 | Tail rocker at one foot from the tail | real | 0 | yes | **no** | defect D-01 |
| p41 | Show original board | bool | true | yes | yes | stored |
| p42 | Stringer speed, bottom | int | 0 | yes | yes | stored |
| p43 | Machine output folder | text | `c:\machine` | yes | yes | stored |
| p44 | Top shoulder angle (degrees) | real | 0 | yes | yes | CAM prefill |
| p45 | Designer | text | empty | yes | yes | display |
| p46 | Top shoulder cuts | int | 0 | yes | yes | CAM prefill |
| p47 | Bottom rail cuts | int | 0 | yes | yes | CAM prefill |
| p48 | Surfer | text | empty | yes | yes | display |
| p49 | Comments (multi-line) | text | empty | yes | yes | only field whose `\n` is unescaped by the original |
| p50 | Fins (9 values, see below) | real[9] | all 0 | yes | yes | printing |
| p51 | Fin type | text | empty | yes | yes | printing |
| p52 | Description | text | empty | yes | **no** | defect D-01 |
| p53 | Security level | int | 0 | yes | yes | stored |
| p54 | Model | text | empty | yes | yes | display |
| p55 | Auxiliary 1 (external board-database id) | text | empty | yes | yes | stored |
| p56 | Auxiliary 2 (external model id) | text | empty | yes | yes | stored |
| p57 | Auxiliary 3 (external last-update stamp) | text | empty | yes | yes | stored |
| p99 | Tail margin | real | 0 | yes | yes | stored |

Codes not listed (p37, p58–p98) are unknown and MUST be ignored on reading.
The derived fields p01–p06 MUST NOT be used to build the model; the model's own
geometry is authoritative.

**Fins array (p50)**, all in cm in board coordinates (x along the board from
the tail, y from the centreline), except where noted:

| Index | Meaning |
|---|---|
| 0 | side fin, rear (trailing) point, x |
| 1 | side fin, rear point, y |
| 2 | side fin, front (leading) point, x |
| 3 | side fin, front point, y (toe-in = [3] − [1]) |
| 4 | centre fin, rear x |
| 5 | centre fin, front x |
| 6 | centre fin depth |
| 7 | side fin depth |
| 8 | side fin splay angle (degrees; the original's dialog passes it through its length-unit conversion, so it is only meaningful when the display unit is cm) |

The original prints fin information when [2] ≠ 0, [5] ≠ 0 or the fin type is
non-empty.

**Display unit code (p38).** Never interpreted by the original. Its internal
unit enumeration is 0 = cm, 1 = inches (fractional), 2 = mm, 3 = decimal
inches, 4 = metres; there is no evidence that p38 uses the same enumeration.
Treat as opaque; round-trip it.

### 3.7 Write order and output rules

The original writes, in this exact order, one line per field:

```
p01 p02 p03 p04 p07 p08 p45 p54 p55 p56 p57 p43 p11 p12 p13 p14 p15 p16 p17
p18 p99 p19 p20 p21 p22 p23 p24 p25 p42 p26 p44 p46 p47 p38 p53 p41 p48 p49
p51 p50 p27 p28 p29 p30 p31 p32 p33 p34 p35
```

Rules of the original writer:
* Text fields whose value is empty are omitted entirely (no line).
* Numeric, boolean and array fields are always written.
* Curve blocks are always written (even if empty: `p32 : (` then `)`).
* A guide block is written only if the curve has at least one guide point.
* Cross-sections are written in list order (sorted by position in normal use),
  including both end sections.
* File extension: whatever name is chosen, the extension is replaced by `.brd`
  (a name without a dot gets `.brd` appended).

Required writer: the same layout and rules, **plus the five fields the original
reads but never writes** (D-01). Recommended full order (order is not
significant to any reader, this one stays close to the original):

```
p01 p02 p03 p04 p07 p08 p09 p45 p54 p55 p56 p57 p52 p43 p10 p11 p12 p13 p14
p15 p16 p17 p18 p99 p19 p20 p21 p22 p23 p24 p25 p42 p26 p44 p46 p47 p38 p39
p40 p53 p41 p48 p49 p51 p50 p27 p28 p29 p30 p31 p32 p33 p34 p35
```

p05/p06 SHOULD NOT be written. Empty text fields are omitted as in the original,
except p07 and p43 (non-empty defaults): when their value is empty they MUST be
written as `p07 : ` / `p43 : ` (with the trailing space, so the value starts at
character 7) so that reading back yields the empty string instead of the
default (D-15).

### 3.8 Reader rules: tolerance

Behaviour of the original reader, line by line, and the required behaviour:

| Situation | Original | Required |
|---|---|---|
| Line shorter than 3 characters | ignored | ignored |
| Line without `:` | ignored | ignored |
| Line not starting with `p` (including leading white space, `(cp` outside a block, keys written by other software that do not start with `p`) | ignored | ignored; leading white space SHOULD be trimmed first |
| Key whose characters 2–3 are not digits (e.g. `pXY : 1`) | whole load aborts | ignore the line, warn |
| Key with three digits, e.g. `p100 : 5` | parsed as code 10 with a shifted value | ignore unknown keys (match `p` + exactly two digits followed by ` :`) |
| Key line shorter than 6 characters (`p08 :`) | whole load aborts | treat value as empty |
| Unknown code | ignored | ignored |
| Code 01–06 | ignored | ignored |
| Duplicate scalar/array/text field | last occurrence wins | same |
| Duplicate p32/p33/p34 block | knots and guide points are **appended** to the curve already read (D-07) | last block replaces earlier one; warn |
| Duplicate p35 block | sections appended | append is acceptable; then sort (below) |
| Blank or foreign line inside a curve/section block | ends the knot list; the remaining lines of the block are mis-parsed (sections after it are lost) (D-08) | skip blank lines inside blocks; any other unexpected line is an error |
| Missing field | default from 3.6 | same |
| Missing p32/p33/p34 | load fails during post-processing (board left half-loaded) | error "missing required curve"; do not modify the caller's board |
| Truncated inside a curve block | rest of curve silently dropped; later post-processing may fail | error "truncated file" |
| Truncated inside p35 | load fails with an exception | error "truncated file" |
| Cross-section positions out of order | kept in file order | sort sections by position (stable) |
| End sections missing (the original's own "scan template" board has none) | first/last real sections are then treated as the dummies | if the first section is not at position ≤ 0 with ≤ 1 knot, insert an end section at 0; if the last is not at position ≥ L with ≤ 1 knot, append one at L |

All reader failures MUST leave the caller's existing board unchanged (parse into
a fresh model, swap on success).

### 3.9 Post-load processing

After parsing, the original:
1. applies continuity normalisation in *demote-only* mode ([1.7](#17-continuity-normalisation-after-import));
2. installs editing constraints (end points of outline locked, deck and bottom
   end knots tied together, handle direction restrictions). These do not change
   any stored value.

Required: step 1 exactly (with the D-29 correction for non-degenerate knots);
step 2 is an editor concern outside this document.

### 3.10 Complete minimal example

```
p01 : 182.88
p02 : 184.9721542551
p03 : 6.096
p04 : 48.895
p07 : V4.4
p08 : Example shortboard
p43 : c:\machine
p11 : 0
p12 : 0
p13 : 0
p14 : 0.0
p15 : 0.0
p16 : 0.0
p17 : 0.0
p18 : 0.0
p99 : 0.0
p19 : 0.0
p20 : 0.0
p21 : 0.0
p22 : 0.0
p23 : 0.0
p24 : 0
p25 : 0
p42 : 0
p26 : 0
p44 : 0.0
p46 : 0
p47 : 0
p38 : 0
p53 : 0
p41 : true
p50 : [0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]
p27 : [0.0,0.0,0.0]
p28 : [0.0,0.0,0.0]
p29 : [0.0,0.0,0.0]
p30 : [0.0,0.0,0.0]
p31 : [0.0,0.0,0.0]
p32 : (
(cp [0.0,0.0,0.0,0.0,0.0,7.334250000000001] false false)
(cp [89.6112,24.4475,43.8912,24.4475,128.016,24.4475] true false)
(cp [182.88,0.0,159.10559999999998,10.26795,159.10559999999998,-10.26795] true false)
)
p33 : (
(cp [0.0,5.4,0.0,5.4,21.9456,2.43] true false)
(cp [84.12480000000001,0.0,43.891200000000005,0.0,128.01600000000002,0.0] true false)
(cp [182.88,12.8,146.304,3.5840000000000005,185.88,18.8] true false)
)
p34 : (
(cp [0.0,5.4,0.0,5.4,0.0,6.050000000000001] false false)
(cp [0.0,6.7,0.0,6.180000000000001,29.2608,6.7266] false false)
(cp [85.9536,6.098113720694958,42.0624,6.098113720694958,129.8448,6.098113720694958] true false)
(cp [182.88,14.0,146.304,8.6176,182.88,13.520000000000001] false false)
(cp [182.88,12.8,182.88,13.280000000000001,182.88,12.8] false false)
)
p35 : (
(p36 0.0
(cp [0.0,0.0,0.0,0.0,0.0,0.0] false false)
)
(p36 91.44
(cp [0.0,0.0,-7.331036995716622,0.0,7.331036995716622,0.0] true false)
(cp [24.436789985722076,1.9139850852667406,24.436789985722076,1.18484790992703,24.436789985722076,3.1778228558555726] true false)
(cp [0.0,6.076143127830923,6.842301196002182,6.076143127830923,-6.842301196002182,6.076143127830923] true false)
)
(p36 182.88
(cp [0.0,0.0,0.0,0.0,0.0,0.0] false false)
)
)
```

(p02 above is illustrative; a writer computes it.)

### 3.11 SurfCraft extensions

SurfCraft stores data BoardCAD does not have under codes BoardCAD ignores, and keeps the legacy fields
filled with their closest equivalent so the file still opens in BoardCAD. A file without these codes loads
exactly as before, and nothing new is written for it.

**p60: fin setup** (one line, written after p50, only when the board has a fin setup):

```
p60 : 1 <preset> <system>[; <system> <x> <y> <base> <toe> <cant> <depth>]...
```

* `1` is the format version. A reader ignores a p60 it cannot read, and the fins then come from p50.
* `<preset>`: `custom`, `single`, `twin`, `twin_trailer`, `thruster`, `quad`, `2+1` or `five`.
* `<system>`: `none`, `fcs1`, `fcs2`, `futures` or `usbox`. The header holds the setup's system, and each fin has its own.
* One group per fin, separated by `;`. Numbers are in Rust's shortest round-trip notation.
  * `x`: tail to the fin's trailing edge (rear of the base), cm.
  * `y`: signed offset of that point from the stringer, cm. Positive is the rider's left when standing on the deck facing the nose; 0 is a centre fin.
  * `base`: base length, cm.
  * `toe`: degrees, leading edge towards the stringer.
  * `cant`: degrees, tip towards the rail.
  * `depth`: cm.

Example (a thruster):

```
p60 : 1 thruster fcs2; fcs2 27.94 21.3 11.1 3.28 6.5 11.43; fcs2 27.94 -21.3 11.1 3.28 6.5 11.43; fcs2 8.89 0 11.1 0 0 11.43
```

p50 is written from the setup:
* The side fin is the frontmost fin on the left, or on the right if there is none on the left: its rear and front points, its depth, and its cant as the splay.
* The centre fin is the frontmost fin on the stringer: its rear x, rear x + base, and its depth.

A board read without p60 shows the setup described by p50: a pair of side fins if a side mark is set, a centre fin if a centre mark is set, and no boxes.

---

## 4. Legacy encrypted `.brd`

The original can **read** encrypted board files produced by older software; it
never writes them. Required: read support exactly as below; never write
encrypted files.

### 4.1 Recognition

The file is opened as text and its first line examined:

| First line starts with | Password |
|---|---|
| `%BRD-1.02` | `deltaXTaildeltaXMiddle` |
| `%BRD-1.01` | `deltaXTail` |

Any other first line → plain `.brd`. Required addition: a first line starting
with `%BRD-` but carrying another version MUST produce an "unsupported
encrypted format" error instead of being parsed as plain text (D-11).

The header occupies **exactly 12 bytes**, which the original skips
unconditionally before decrypting: `%BRD-1.0x`, two spaces, LF
(hex `25 42 52 44 2D 31 2E 30 3x 20 20 0A`). The ciphertext starts at byte
offset 12 and runs to the end of the file.

### 4.2 Cipher

"PBEWithMD5AndDES" as defined by PKCS #5 v1.5 (PBES1):

| Parameter | Value |
|---|---|
| Key derivation | PBKDF1 with MD5 |
| Password bytes | the ASCII bytes of the password string (no terminator) |
| Salt (8 bytes) | `C7 73 21 8C 7E C8 EE 99` |
| Iteration count | 20 |
| Derived bytes | D = MD5(password ‖ salt), then D := MD5(D) 19 more times (20 MD5 applications in total); 16 bytes |
| DES key | D[0..8] (parity bits ignored) |
| IV | D[8..16] |
| Algorithm | single DES, CBC mode |
| Padding | PKCS #5 (1–8 bytes, each equal to the pad length) |

Test vectors (hex):

| Password | DES key | IV |
|---|---|---|
| `deltaXTaildeltaXMiddle` | `8c41236a7fc96bce` | `7aff2c48277a18dd` |
| `deltaXTail` | `19cc4f0b1e99ef78` | `bb75f45c2eee1d93` |

Verified against `tests/fixtures/longboard_encrypted.brd` (3860 bytes:
12-byte `%BRD-1.02` header + 3848 bytes ciphertext): decryption gives 3841
bytes, byte-identical to `tests/fixtures/longboard.brd`; the last block carries
7 bytes of padding value 0x07.

### 4.3 Decrypted content

The plaintext is an ordinary plain-text `.brd` ([section 3](#3-brd--native-board-file)),
starting directly with the first field line (no header), decoded with the same
character-set rule as plain files. It is parsed and post-processed exactly like
a plain file. A ciphertext whose length is not a multiple of 8 or whose padding
is invalid MUST be reported as a corrupt file.

---

## 5. Partial curve files `.otl`, `.pfl`, `.crs`

These files exchange one part of a board. They use the `.brd` block syntax of
[3.4](#34-curve-blocks-and-control-point-lines)/[3.5](#35-cross-sections-block),
the same encoding and number formatting, LF line ends, and **no** scalar fields.

### 5.1 Content as written by the original

| Extension | Content |
|---|---|
| `.otl` (outline) | exactly one `p32` block (outline knots + optional outline guide block) |
| `.pfl` (profile) | a `p33` block (bottom) immediately followed by a `p34` block (deck) |
| `.crs` (cross-section) | exactly one **bare** section block `(p36 <position>` … `)` for the currently selected cross-section — no `p35` wrapper |

`.otl` example:

```
p32 : (
(cp [0.0,0.0,0.0,0.0,0.0,7.334250000000001] false false)
(cp [89.6112,24.4475,43.8912,24.4475,128.016,24.4475] true false)
(cp [182.88,0.0,159.10559999999998,10.26795,159.10559999999998,-10.26795] true false)
)
```

`.crs` example:

```
(p36 91.44
(cp [0.0,0.0,-7.331036995716622,0.0,7.331036995716622,0.0] true false)
(cp [24.436789985722076,1.9139850852667406,24.436789985722076,1.18484790992703,24.436789985722076,3.1778228558555726] true false)
(cp [0.0,6.076143127830923,6.842301196002182,6.076143127830923,-6.842301196002182,6.076143127830923] true false)
)
```

The writer replaces the chosen file name's extension by the format's extension.
Side effect of the original: after an export, the board's current file name is
set to the exported file's name, so a later plain "Save" writes `<name>.brd`
next to the exported file instead of the board's real file (D-18). Required:
exporting MUST NOT change the board's file name or dirty state.

### 5.2 Import behaviour of the original (all defective)

* **Outline import**: the board's outline is replaced by an empty curve and the
  file's knots are appended. Guide points from the file are *appended* to the
  board's existing outline guide points. The reader then never advances past the
  first line and loops forever once the file is exhausted — the application
  hangs (D-16).
* **Profile import**: bottom and deck are replaced by empty curves, then only
  the **first** block of the file is read (the `p33` block in files written by
  the original). The deck stays empty, post-processing fails, an error is shown,
  and the board is left with the new bottom and an empty deck (D-17).
* **Cross-section import**: the first line must be a `pNN : (` key; the native
  `.crs` first line `(p36 …` is not one, so every `.crs` written by the original
  fails to import with a number-format error (D-19). If a file starting with
  `p35` were given, its sections would be *appended* to the board instead of
  replacing the current one; a file starting with `p33`/`p34` would append knots
  to the bottom/deck. Even when parsing succeeded the command then failed with a
  type error.

### 5.3 Required import behaviour

General: parse into a temporary curve first; on any error leave the board
unchanged and report the error; ignore blank lines; accept any line-ending.
After a successful import apply continuity normalisation (demote-only) to the
imported curve(s), then let the model update dependent data exactly as after an
interactive edit of the same curve (see the geometry specification).

* `.otl`: the file MUST contain one `p32` block (other content outside the
  block is ignored with a warning). The board's outline knots are replaced.
  If the file contains a guide block, the outline guide points are **replaced**
  by the file's; otherwise they are kept.
* `.pfl`: the file contains a `p33` block and/or a `p34` block, in any order.
  Each present block replaces the corresponding curve (and its guide points if
  the block has a guide block). A file with neither block is an error. If only
  one is present the other curve is kept; the implementation SHOULD warn when
  the resulting deck and bottom end knots do not coincide.
* `.crs`: accept (a) a bare `(p36 …` block — the native form — and (b) for
  robustness, a `p35` block containing exactly one section. The knots (and guide
  points, if present) **replace** those of the currently selected cross-section.
  The position stored in the file is ignored; the section keeps its current
  position. Then the section is rescaled to the board at that position:
  * old width := 2 × (largest x reached by the section curve, including the
    curve's interior, not only end points); old thickness := (E.y of the last
    knot) − (E.y of the first knot); each is clamped to at least 0.1;
  * target width := 2 × outline y at the position; target thickness := deck
    height − bottom height at the position;
  * if target thickness ≤ 0.1 or target width ≤ 0.1 nothing is scaled;
  * otherwise every point (E, P, N of every knot) has x multiplied by
    |target width / old width| and y by |target thickness / old thickness|.
  Selecting no section (or an end section) makes `.crs` import unavailable.

Exporting a `.crs` and importing it back into the same section MUST reproduce
the section exactly (up to floating-point rounding of the scale, which is 1).

---

## 6. STEP (`.stp` / `.step`, "BCstep 1.0")

The original exports an ISO 10303-21 file that carries **two things at once**:
a NURBS surface model of the board (for other CAD programs) and the exact Bezier
board encoded as `BEZIER_CURVE` entities with special names (for itself). On
import it rebuilds the Bezier board **only from the Bezier curves**; the NURBS
surfaces are parsed for its separate 3-D model.

Board metadata (name, designer, comments, all p-fields) is **not** stored. A
STEP round trip keeps geometry and guide points only.

### 6.1 Overall layout as written

```
ISO-10303-21;
HEADER;
FILE_DESCRIPTION (( 'STEP AP203' ), '1' );
FILE_NAME ('<full path>','<timestamp>',( '' ),( '' ),'BCstep 1.0','BoardCAD','');
FILE_SCHEMA (( 'CONFIG_CONTROL_DESIGN' ));
ENDSEC;
DATA;
  deck surface (complex entity) + its control points
  bottom surface (complex entity) + its control points
  CARTESIAN_TRANSFORMATION_OPERATOR + 2 DIRECTION + 1 CARTESIAN_POINT
  BEZIER_CURVE 'outline' + its points + outline guide points
  BEZIER_CURVE 'bottom'  + its points + bottom guide points
  BEZIER_CURVE 'deck'    + its points + deck guide points
  for every cross-section in list order (end sections included):
      BEZIER_CURVE 'section' + its points + that section's guide points
  product / shape-representation / B-rep / style / context / unit entities
ENDSEC;
END-ISO-10303-21;
```

Each statement is on one line except the two surface entities (multi-line,
shown in 6.4). The original writes lines with the platform line separator
(CR LF on Windows, LF elsewhere).

* The FILE_NAME timestamp is written as `year-month-dayThour:minute:second` with
  no zero padding and a **zero-based month** (1 November 2026 23:29:43 is written
  `2026-10-1T23:29:43`) (D-19b). Required: write ISO 8601 `2026-11-01T23:29:43`.
  Nothing reads the timestamp.
* The strings `BCstep 1.0` and `BoardCAD` in the FILE_NAME line are significant
  to the original reader (6.8). The file name inside FILE_NAME is the full path;
  a writer MUST escape an apostrophe in it as two apostrophes (STEP rule) and
  SHOULD write only the base name.

### 6.2 Coordinate mapping (Bezier part)

STEP space for everything the original writes: **millimetres**,
X = board length axis (from the tail), Y = vertical (height), Z = lateral
(half-width direction). Every Bezier coordinate is multiplied by 10.

| Source | STEP point (X, Y, Z) |
|---|---|
| outline point (x, y) | (10x, 0.0, 10y) |
| bottom or deck point (x, y) | (10x, 10y, 0.0) |
| cross-section point (x, y) at position s | (10s, 10y, 10x) |
| outline guide point (x, y) | (10x, 0.0, 10y) |
| bottom / deck guide point (x, y) | (10x, 10y, 0.0) |
| guide point (x, y) of section number n | (0.0, 10y, 10x) — X is written as 0.0, not the position (D-21); readers ignore X |

The declared length unit (6.7) is `SI_UNIT(.MILLI.,.METRE.)`. The original
reader assumes millimetres without looking at the unit. Required reader:
honour the length unit of the representation context (`.MILLI.` → ×0.1 to get
cm, `.CENTI.` → ×1, none/metre → ×100); assume millimetres if absent.

### 6.3 Bezier curves

For a curve of k knots the writer emits

```
#c = BEZIER_CURVE( '<label>',3,(#c+1,#c+2,...,#c+3k-2),.UNSPECIFIED.,.F.,.F.);
```

followed immediately by the 3k−2 referenced `CARTESIAN_POINT` lines, numbered
c+1 … c+3k−2 in the same order as referenced. The control-point order is the
natural piecewise-cubic order:

```
E0, N0,  P1, E1, N1,  P2, E2, N2,  ...,  P(k-1), E(k-1)
```

i.e. the first knot contributes E and N, every inner knot P, E, N, the last
knot P and E. The first knot's P and the last knot's N are **not written** (the
format has no place for them). A one-knot curve (the end sections) is written
as a BEZIER_CURVE with a single reference.

Point names (the first string argument of CARTESIAN_POINT) carry metadata:

* an **end point** (E) is named `'<continuous> <auxiliary>'`, e.g.
  `'true false'`, `'false false'`;
* a **handle** (P or N) is named `'NONE'`.

Labels: `'outline'`, `'bottom'`, `'deck'`, `'section'`. Cross-sections are
written in list order, which includes the zero-size end sections first and last.

Note that the continuity flags exported are those of the in-memory board, which
after a `.brd` load have been normalised (1.7); e.g. a `.brd` knot flagged
`true` whose handles are not collinear is exported as `'false false'`.

Example (outline, 4 knots → 10 points; then a section with 5 knots):

```
#493 = BEZIER_CURVE( 'outline',3,(#494,#495,#496,#497,#498,#499,#500,#501,#502,#503),.UNSPECIFIED.,.F.,.F.);
#494 = CARTESIAN_POINT ( 'false false',  ( 0.0, 0.0, 0.0) );
#495 = CARTESIAN_POINT ( 'NONE',  ( 0.0, 0.0, 73.3425) );
#496 = CARTESIAN_POINT ( 'NONE',  ( 6.400000000000001, 0.0, 120.01500000000001) );
#497 = CARTESIAN_POINT ( 'false false',  ( 16.0, 0.0, 133.35000000000002) );
#498 = CARTESIAN_POINT ( 'NONE',  ( 308.608, 0.0, 222.25) );
#499 = CARTESIAN_POINT ( 'NONE',  ( 438.912, 0.0, 244.47500000000002) );
#500 = CARTESIAN_POINT ( 'true false',  ( 896.112, 0.0, 244.47500000000002) );
#501 = CARTESIAN_POINT ( 'NONE',  ( 1280.1599999999999, 0.0, 244.47500000000002) );
#502 = CARTESIAN_POINT ( 'NONE',  ( 1591.0559999999998, 0.0, 102.6795) );
#503 = CARTESIAN_POINT ( 'true false',  ( 1828.8, 0.0, 0.0) );
...
#529 = BEZIER_CURVE( 'section',3,(#530),.UNSPECIFIED.,.F.,.F.);
#530 = CARTESIAN_POINT ( 'false false',  ( 0.0, 0.0, 0.0) );
```

Guide points follow their curve's points as one CARTESIAN_POINT each, named

| Curve | Name |
|---|---|
| outline | `'outline guide point'` |
| bottom | `'bottom guide point'` |
| deck | `'deck guide point'` |
| section number n (0-based index in the written section list, end sections counted) | `'cross section n guide point'`, e.g. `'cross section 3 guide point'` |

Numbers use the formatting of [section 2](#2-number-formatting-shared-by-the-text-formats).
The literal layout `#id = CARTESIAN_POINT ( 'name',  ( x, y, z) );` (two spaces
after the name's comma) is what the original writes; the original reader also
accepts other spacing, see 6.8.

### 6.4 NURBS surfaces

Two surfaces, deck first, then bottom. Each is one complex (multi-type) entity:

```
#200=(
BOUNDED_SURFACE()
B_SPLINE_SURFACE(3,3,(
(#201, #202, ..., #211), 
(#212, ..., #222), 
...
(#333, ..., #343)), 
.UNSPECIFIED.,.F.,.F.,.F.)
B_SPLINE_SURFACE_WITH_KNOTS(
( 4, 1, 1, 1, 1, 1, 1, 1, 1, 1, 4),
( 4, 1, 1, 1, 1, 1, 1, 1, 4),
( 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0 ),
( 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0 ),
.UNSPECIFIED.)
GEOMETRIC_REPRESENTATION_ITEM()
RATIONAL_B_SPLINE_SURFACE((
(1.0, 1.0, ..., 1.0), 
...
(1.0, ..., 1.0))) 
REPRESENTATION_ITEM('deck')
SURFACE()
);
```

followed by the control points `#201 … ` as `CARTESIAN_POINT ( 'NONE', ( x, y, z) )`,
row by row. The bottom surface is identical in form with
`REPRESENTATION_ITEM('bottom')`.

Structure:

* Bicubic (degrees 3, 3), non-rational in practice (all weights 1.0).
* u direction = rows = along the board, from tail (first row) to nose (last
  row); v direction = columns = across the board from one rail edge to the
  other. Let R = rows written, C = columns written.
* u knot multiplicities: `4`, then R−4 times `1`, then `4` (R−2 entries); u knot
  values: 0, 1, …, R−3 (as reals). Likewise for v with C.
* Coordinates in millimetres, X along the board, Y vertical, **Z lateral with
  its sign inverted** relative to the internal 3-D model (the original flips Z
  before writing and flips it back after reading files marked `BCstep 1.0`).
* In the sample file both surfaces are 13 × 11; the first and last rows are
  degenerate (all points equal: tail and nose points) and the first/last three
  columns of each row coincide (tucked-under edge).
* Internally the original's 3-D model has two extra rows at each end and two
  extra columns at each side that are not written; the reader recreates them by
  duplication (6.8).

The surfaces are an *approximation* of the Bezier board produced by the
original's NURBS fitting routine. Observed in the sample: the surface rows end
at X = 1800.0 mm while the Bezier board is 1828.8 mm long, i.e. the surfaces do
not necessarily cover the full length (see [13](#13-undetermined-points)). The
exact fitting algorithm is outside the scope of this document; any reasonable
bicubic B-spline fit of deck and bottom satisfies the format. Requirements for
the original to accept the file are in 6.9.

### 6.5 Transformation operator

```
#489 = CARTESIAN_TRANSFORMATION_OPERATOR( '','','',#490, #491, #492,1.0);
#490 = DIRECTION ( 'x axis',  ( 0.0, 0.0, 0.0) );
#491 = DIRECTION ( 'y axis',  ( 0.0, 0.0, 0.0) );
#492 = CARTESIAN_POINT ( 'local origin',  ( 0.0, 0.0, 0.0) );
```

It is meant to relate the NURBS model's frame to the Bezier board. The original
always writes zero vectors for both directions and the origin (D-20), because
its export never copies the real values. Required writer: write
x axis (1.0, 0.0, 0.0), y axis (0.0, 1.0, 0.0), origin (0.0, 0.0, 0.0). The
Bezier reconstruction never uses these entities.

### 6.6 Trailing entities

After the last section's points the original writes (ids consecutive):

```
#610 = PRODUCT_DEFINITION_SHAPE('','',#611);
#611 = PRODUCT_DEFINITION('design','',#612);
#612 = PRODUCT_DEFINITION_FORMATION('','',#613);
#613 = PRODUCT('Board name', 'Generated from BoardCAD','');
#614 =SHAPE_DEFINITION_REPRESENTATION(#610,#636);
#615 = ADVANCED_FACE('',(#616),#200,.F.);
#616 = FACE_BOUND('',#617,.T.);
#617 = VERTEX_LOOP('',#618);
#618 = VERTEX_POINT('',#619);
#619 = CARTESIAN_POINT('',(0.0,0.0,0.0));
#620 = ADVANCED_FACE('',(#621),#345,.F.);
#621 = FACE_BOUND('',#622,.T.);
#622 = VERTEX_LOOP('',#623);
#623 = VERTEX_POINT('',#624);
#624 = CARTESIAN_POINT('',(0.0,0.0,0.0));
#625=CLOSED_SHELL('',(#615,#620));
#626=MANIFOLD_SOLID_BREP('',#625);
#627=DRAUGHTING_PRE_DEFINED_COLOUR('white');
#628=FILL_AREA_STYLE_COLOUR('Plastic (White)',#627);
#629=FILL_AREA_STYLE('Plastic (White)',(#628));
#630=SURFACE_STYLE_FILL_AREA(#629);
#631=SURFACE_SIDE_STYLE('Plastic (White)',(#630));
#632=SURFACE_STYLE_USAGE(.BOTH.,#631);
#633=PRESENTATION_STYLE_ASSIGNMENT((#632));
#634=STYLED_ITEM('',(#633),#626);
#635=MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION('',(#634),#637);
#636=ADVANCED_BREP_SHAPE_REPRESENTATION('ABSR',(#626),#637);
#637 = ( GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#641)) GLOBAL_UNIT_ASSIGNED_CONTEXT((#638,#639,#640)) REPRESENTATION_CONTEXT('Context #1', '3D Context with UNIT and UNCERTAINTY') );
#638 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );
#639 = ( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) );
#640 = ( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() );
#641 = UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.01),#638,'distance_accuracy_value','confusion accuracy');
```

(ids from the sample file). The two ADVANCED_FACE entities reference the deck
and bottom surface entities. The product name is the literal `'Board name'`;
a required writer SHOULD put the board's name there (STEP-escaped). The B-rep is
not a valid closed solid by the STEP standard (faces bounded by vertex loops);
this is accepted as-is.

### 6.7 Entity numbering of the original

Not required for compatibility, documented for byte-level comparison:

* Deck surface entity id 200; its R×C points 201 … 200+R·C.
* One id is skipped; bottom surface entity id 202+R·C; its points follow.
* The transformation operator takes the next id after the last bottom point,
  then its three dependents.
* Each BEZIER_CURVE takes the next free id, its points follow consecutively,
  then its guide points; **one id is skipped after every curve** (before the
  next BEZIER_CURVE) — in the sample, #504, #513, #528, #531 … are unused.
* After the last section, one id is skipped and the trailing block (6.6)
  starts.

### 6.8 How the original reads a STEP file

The reader is line-oriented and recognises entities by substring search. What
it extracts:

1. **Every line containing `CARTESIAN_POINT` or `DIRECTION` at a position other
   than the very start of the line** is parsed as a point: the name is the text
   between the first and second `'` of the line; the id is the number between
   the first `#` and the first `=`; after the `=`, the text up to and including
   the first `,` is skipped; X is the number between the next `(` and `,`; Y up
   to the next `,`; Z up to the next `)`. Points are stored in a table indexed
   by id; the table has 10 000 slots (ids ≥ 10 000 crash the reader, D-22).
2. A line containing `FILE_NAME` marks the file as a BoardCAD file if it also
   contains `BoardCAD`, and as version 1.0 if it contains `BCstep 1.0`.
3. A line containing `BOUNDED_SURFACE` starts a surface: subsequent lines are
   collected until a line containing `;` (that last line is **not** included);
   if the collected text contains `deck` it is the deck surface, if it contains
   `bottom` the bottom surface.
4. A line containing `CARTESIAN_TRANSFORMATION_OPERATOR`: the three `#` ids
   after the first `#` are the x-axis, y-axis and origin points.
5. A line containing `BEZIER_CURVE`: if the line contains no `;`, further lines
   are appended until a line containing `;` — which is again **not** included,
   so a curve spanning several lines loses its last line (D-22). Then: contains
   `outline` → outline curve; `bottom` → bottom curve; `deck` → deck curve;
   `section` → appended to the list of section curves (at most 100). A later
   curve with the same label replaces an earlier one.
6. Reading stops at a line containing `END-ISO-10303-21`. If the file has no
   such line the reader crashes at end of file (D-22).

**Rebuilding the Bezier board** (only if an `outline` curve was found;
otherwise the target board is left untouched, D-23):

* The board is reset to defaults (all metadata default, see 3.6).
* For each curve, the referenced ids are taken in order. With m references
  (m = 3k−2) the knots are:
  * first knot: E = point 1, N = point 2, P := E;
  * inner knot j (j = 1 … k−2): P = point 3j, E = point 3j+1, N = point 3j+2;
  * last knot: P = point m−1, E = point m, N := E.
  * **End-handle repair:** the first knot's P and the last knot's N, which the
    format cannot carry, are set equal to the knot's own end point.
* 2-D coordinates from 3-D points (divide by 10 to get cm):
  outline (X, Z); bottom and deck (X, Y); section (Z, Y).
* Flags of each knot come from the name of its **end point**: name contains
  `false false` → (false, false); `false true` → (false, true);
  `true false` → (true, false); anything else → (true, true).
* A curve with exactly one reference (the end sections) yields one knot whose
  E, P, N all equal that point, mapped as (Z, Y) / 10 regardless of the curve
  type (only correct for sections; irrelevant in practice).
* Section position := X of the section curve's **first referenced point** / 10.
  Sections are added in file order (no sorting).
* Guide points: every stored point whose name equals (case-insensitively)
  `outline guide point` → outline guide (X, Z)/10; `bottom guide point` →
  bottom guide (X, Y)/10; `deck guide point` → deck guide (X, Y)/10;
  `cross section n guide point` → guide of the n-th section curve in file order,
  (Z, Y)/10. Guide points are collected in ascending id order.
* No continuity normalisation is applied after a STEP load.

**NURBS part (only for a full open, not for the file-chooser preview):** both
surfaces are parsed from the collected text: the number of written rows R is
(number of entries in the first multiplicity list) + 2, and the internal grid
has R + 4 rows (two padding rows at each end); likewise C and C + 4 for
columns from the second list; knot values are taken from the third and fourth
lists; the written control points fill rows 2 … R+1 and columns 2 … C+1 of the
(R+4) × (C+4) grid (0-based internal indices). Then the original pads and repairs the
grid: the two extra rows at the tail copy the first written row and the two at
the nose copy the last written row; per row, the bottom surface's three
first columns all take the bottom's first written column point p, its three
last columns take p mirrored in Z, the deck's first three columns take the
mirrored point and the deck's last column takes p (the code sets the same
last deck column three times instead of three columns, D-32). If the file is
`BCstep 1.0`, Z of both surfaces is negated back. The v knot tail is filled from
the u knot array (D-22). Missing deck or bottom surface → the full open crashes
(D-22); the preview path does not need them.

### 6.9 Requirements for a new STEP writer (to be readable by the original)

A writer MUST:

* produce the header lines of 6.1 with `BCstep 1.0` and `BoardCAD` inside the
  `FILE_NAME` line (otherwise the original does not un-flip the surfaces);
* write **every `CARTESIAN_POINT`, `DIRECTION`, `BEZIER_CURVE` and
  `CARTESIAN_TRANSFORMATION_OPERATOR` statement on a single line**, with
  `#id = ` before the type name (the type name must not be at column 0);
* use point names without `,`, `(`, `)` or `'` (other than the delimiting
  quotes) — names of 6.3 satisfy this;
* keep all entity ids below 10 000 and write at most 100 section curves;
* write both surfaces (deck and bottom) as in 6.4, each surface's
  `BOUNDED_SURFACE` text containing the word `deck` resp. `bottom` and no other
  of those words, and the surfaces ending with a line `);`;
* write the Bezier curves with labels and point naming of 6.3 and the
  coordinate mapping of 6.2, all sections including end sections;
* write the curve labels `outline`, `bottom`, `deck`, `section` only in the
  corresponding BEZIER_CURVE lines (the original matches substrings);
* end with `END-ISO-10303-21;`.

A writer SHOULD reproduce the trailing entities of 6.6 and the numbering of 6.7.

### 6.10 Requirements for a new STEP reader

* Use a real ISO 10303-21 tokenizer: statements end at `;` outside strings,
  may span lines, complex entities `( … )` allowed, `''` inside strings.
* Locate `BEZIER_CURVE` entities by their name argument (exact match
  `outline`, `bottom`, `deck`, `section`); rebuild knots exactly as in 6.8,
  including the end-handle repair and the flag decoding.
* Accept files whose surfaces are missing or malformed (the Bezier board does
  not need them); report "not a BoardCAD STEP file" when no `outline`
  BEZIER_CURVE exists, without modifying the caller's board.
* Honour the length unit (6.2).
* Section positions from the first point's X; sort sections by position
  afterwards; attach `cross section n guide point` entries to the n-th section
  curve **in file order** before sorting.
* Apply continuity normalisation? **No** — to match the original exactly, do
  not. (The flags in the file already went through normalisation when the file
  was written from a loaded board.)
* Result metadata: defaults (3.6), file name = the STEP file.

### 6.11 Round-trip consequences

STEP → board → STEP is stable. Board → STEP → board loses: all metadata, the
first knot's P and last knot's N of every curve (replaced by E; e.g. a bottom
curve whose last knot had N = (185.88, 18.8) comes back with N = E), and
section-order information if sections were unsorted.

---

## 7. Shape3D design `.s3d`

Read-only (the original has no `.s3d` writer). XML. The original uses a DOM
parser and always takes **the first descendant element with a given name in
document order**; nesting depth is not checked.

### 7.1 Elements read

```
Shape3d_design
  Board
    Name                      → board "model" (p54)
    Author                    → board "designer" (p45)
    Comment                   → board "comments" (p49)
    Outline   / Bezier3d      → outline curve           (plane XY)
    Bottom    / Bezier3d      → bottom (rocker) curve   (plane XZ)
    Deck      / Bezier3d      → deck curve              (plane XZ)  [optional]
    Couples_0 / Bezier3d      → first cross-section     (plane YZ)
    Couples_1 / Bezier3d
    ...                         (read Couples_0, Couples_1, … until an index is missing)
```

Inside each `Bezier3d`:

```
Bezier3d
  Control_points / Polygone3d
      Nb_of_points            integer N
      Point3d (x, y, z) ×(N+1) — the first Point3d is a symmetry point and is skipped
  Tangents_1 / Polygone3d
      Point3d ×(N+1)          — same indexing; first skipped
  Tangents_2 / Polygone3d
      Point3d ×(N+1)
  Tangent_type_point_0 … Tangent_type_point_(N-1)   integer
  Number_of_guides            integer G   [optional, default 0]
  Guide ×G
      Point3d (x, y, z)
```

`Point3d` has child elements `x`, `y`, `z` whose text content is a real.

Illustrative minimal file:

```xml
<Shape3d_design>
  <Board>
    <Name>Model A</Name>
    <Author>Designer</Author>
    <Comment>Notes</Comment>
    <Outline>
      <Bezier3d>
        <Control_points><Polygone3d>
          <Nb_of_points>3</Nb_of_points>
          <Point3d><x>0</x><y>0</y><z>0</z></Point3d>
          <Point3d><x>0</x><y>0</y><z>0</z></Point3d>
          <Point3d><x>90</x><y>26</y><z>0</z></Point3d>
          <Point3d><x>183</x><y>0</y><z>0</z></Point3d>
        </Polygone3d></Control_points>
        <Tangents_1><Polygone3d> … 4 Point3d … </Polygone3d></Tangents_1>
        <Tangents_2><Polygone3d> … 4 Point3d … </Polygone3d></Tangents_2>
        <Tangent_type_point_0>0</Tangent_type_point_0>
        <Tangent_type_point_1>1</Tangent_type_point_1>
        <Tangent_type_point_2>1</Tangent_type_point_2>
      </Bezier3d>
    </Outline>
    <Bottom> … </Bottom>
    <Deck> … </Deck>
    <Couples_0><Bezier3d> … </Bezier3d></Couples_0>
  </Board>
</Shape3d_design>
```

### 7.2 Knot mapping

For knot i (1 ≤ i ≤ N, Point3d index i):

* E from `Control_points` Point3d i, P from `Tangents_1` Point3d i,
  N from `Tangents_2` Point3d i.
* Plane projection: outline (x, y); bottom and deck (x, z); sections (y, z).
* Continuous flag := (`Tangent_type_point_(i−1)` ≠ 0). Auxiliary flag false.
* Values are used **as is** (no unit conversion: Shape3D design files are in
  centimetres).
* Guide points: the first G `Guide` elements, each projected with the same
  plane rule.

### 7.3 Board assembly

1. **Outline** read. Then the tips are closed if Shape3D's outline stops away
   from the centreline:
   * Tail: if the first knot's E.y > 1.0 cm: that knot becomes non-continuous and
     its P.y := (2/3)·E.y (P.x unchanged); a new knot is inserted before it with
     E = P = (0, 0) and N = (0, E.y/3) (E.y of the old first knot), flags false.
   * Nose: if the last knot's E.y > 1.0 cm: that knot becomes non-continuous and
     its N := (E.x, (2/3)·E.y); a new knot is appended with E = (E.x, 0),
     P = (E.x, E.y/3), N = (E.x, 0), flags false.
2. **Bottom** read.
3. **Deck**: if a `Deck` element exists it is read. If not (files that carry a
   thickness curve instead), a placeholder deck is generated and the load
   returns a *warning* status ("thickness curve not supported"):
   * knot 1: copy of the bottom's first knot, non-continuous, E.y += 1.5,
     N := (20, E.y − 0.5) using the new E.y; P kept from the copy;
   * knot 2: E = (L/2, T), P = (L/2 − 50, T), N = (L/2 + 50, T), with
     T = ((L / 30.48) − 5) / 4 + 2.125 · 2.54 (L in cm; this mixes feet and cm and
     is only a rough placeholder);
   * knot 3: copy of the bottom's last knot, non-continuous, E.y += 1.5,
     P.y += (new E.y)/2, N.x := E.x, N.y += 0.8.
   Required: keep the warning; an implementation MAY instead derive the deck as
   bottom + thickness if it implements Shape3D's thickness curve (not specified
   here, see 13).
4. **Deck tie-in**: Shape3D's deck starts and ends at the top of the tail/nose,
   while the model's deck starts/ends at the bottom's end points. Therefore:
   * a copy of the bottom's first knot is inserted at the start of the deck
     with its N := (0, midpoint height between that knot's E.y and the old first
     deck knot's E.y); its P and flags are those of the bottom knot;
   * a copy of the bottom's last knot is appended to the deck with its
     P := (its E.x, midpoint height between its E.y and the old last deck knot's
     E.y).
5. **Cross-sections**: for each `Couples_i` (i = 0, 1, … while present):
   * position := x of Point3d index 1 of its `Control_points` (absolute board x);
   * knots read in plane (y, z);
   * Shape3D places sections at their absolute height on the rocker; the model
     wants them relative to the bottom. Therefore the first knot's E.y is
     subtracted from the y of every E, P, N of the section.
6. The first Shape3D section's position is set to 0.2 cm and the last one's to
   L − 0.2 cm (Shape3D's tip sections lie exactly on the tips).
7. Zero-size end sections (1.4) are added at 0 and at L.
8. Metadata: Name, Author, Comment as in 7.1.
9. Continuity normalisation, demote-only.

Errors: malformed XML or I/O → error. Required additions (the original crashes
on these, D-26): missing `Name`/`Author`/`Comment` → leave the field at its
default; no `Couples_0` → error "no cross-sections"; missing `Bottom` or
`Outline` → error; `Nb_of_points` and `Tangent_type_point_*` values trimmed
before parsing; a missing `Tangent_type_point_k` → treat as continuous with a
warning.

---

## 8. `.srf` binary board

Read-only. A binary board format of another surfboard CAD program. All
multi-byte values are **little-endian**; reals are IEEE-754 32-bit floats,
counts are signed 16-bit integers. Lengths in the file are **metres**; x along
the board is measured **from the nose**. The model gets
x<sub>model</sub> = L − 100·x<sub>file</sub>, y<sub>model</sub> = 100·y<sub>file</sub>,
where L = 100 × (board length field).

### 8.1 Layout

Text header (bytes, read one at a time):

| Part | Content |
|---|---|
| version | bytes up to and including the first space (0x20); discarded |
| name | bytes up to and including the first `*` (0x2A); the name is these bytes **minus the last two** (the `*` and the byte before it, normally a space) → board "model" (p54) |
| comments | bytes up to and including the first `@` (0x40); the comments are these bytes minus the `@` → board comments (p49) |

The original copies each part into a 500-byte buffer (longer parts crash, D-28)
and decodes with the platform character set; required: no length limit, decode
as Windows-1252.

Then, with offsets relative to the byte after `@`:

| Offset | Size | Type | Meaning |
|---|---|---|---|
| 0 | 11 | — | unknown, skipped |
| 11 | 4 | f32 | board length (m) |
| 15 | 16 | 4 × f32 | outline measurements O0…O3 (m) |
| 31 | 4 | f32 | wide-point position (m, from nose) |
| 35 | 16 | 4 × f32 | rocker measurements K0…K3 (m) |
| 51 | 16 | 4 × f32 | rail measurements (unused) |
| 67 | 16 | 4 × f32 | thickness measurements (unused) |
| 83 | 113 | — | unknown, skipped |
| 196 | 2 | i16 | outline knot count n<sub>o</sub> |
| 198 | 76·n<sub>o</sub> | records | outline knots (curve record) |

Then four more curves, each preceded by **1 unknown byte and an i16 count**,
followed by that many 76-byte curve records, in this order:
**rocker**, **rail** (upper rail line), **deck**, **bottom** (lower rail line).
Then deck "caves" (deck cross-section halves): 1 unknown byte, i16 count, that
many 158-byte cave records; then bottom caves: 1 unknown byte, i16 count, cave
records.

**Curve record (76 bytes)**, knots stored nose → tail:

| Offset | Size | Content |
|---|---|---|
| 0 | 12 | end point (x, y, z) f32 |
| 12 | 12 | unknown |
| 24 | 12 | handle A (x, y, z) |
| 36 | 12 | handle B (x, y, z) |
| 48 | 28 | unknown |

z is ignored. Model knot: E from the end point, P from handle A, N from
handle B (after the coordinate mapping), flags false (set later by
normalisation). The record list is reversed so that the model curve runs tail →
nose. For the outline y is the half-width; for the other curves y is height.

**Cave record (158 bytes)**:

| Offset | Size | Content |
|---|---|---|
| 0 | 6 | unknown |
| 6 | 12 | A — centreline point (x lateral, y height, z = distance from nose) |
| 18 | 12 | unknown |
| 30 | 12 | B — handle belonging to A |
| 42 | 40 | unknown |
| 82 | 12 | C — rail-side point |
| 94 | 24 | unknown |
| 118 | 12 | D — handle belonging to C |
| 130 | 28 | unknown |

Cave coordinates: lateral and height in metres (×100), height on the same
absolute datum as the rocker; station = L − 100·A.z.

### 8.2 Board assembly

1. **Guide points** from the measurements, added tail → nose at five stations
   (cm): s0 = 0, s1 = 30.48, s2 = L − 100·widepoint, s3 = L − 30.48, s4 = L:

   | Station | Outline guide y (cm) | Bottom guide y (cm) |
   |---|---|---|
   | 0 | 100·O3 | 100·K3 |
   | 30.48 | 100·O2 | 100·K2 |
   | L − 100·wide-point | 100·O1 | 0 |
   | L − 30.48 | 100·O0 | 100·K1 |
   | L | 0 | 100·K0 |

   (Whether O are half or full widths is not known; the values are used
   directly as outline y.)
2. **Outline** from the outline records (mapped, reversed). Tip closing as in
   7.3 step 1 but with threshold **0.3 cm** and with the tail knot's P set to
   (0, (2/3)·E.y) (x forced to 0).
3. **Bottom** from the rocker records.
4. **Deck** from the deck records. The original iterates over the **rocker**
   count when reading the deck records (D-27); required: use the deck count.
5. **Deck tie-in**: a copy of the bottom's first knot is inserted at the deck
   start; its N := its E + 0.3·(E<sub>d</sub> − E) where E<sub>d</sub> is the old
   first deck knot's end point; the old first deck knot's P := E<sub>d</sub> +
   0.3·(E − E<sub>d</sub>). Symmetrically at the nose with a copy of the bottom's
   last knot appended (its P := its E + 0.3·(E<sub>d</sub> − E) towards the old
   last deck knot; the old last deck knot's N := E<sub>d</sub> + 0.3·(E − E<sub>d</sub>)).
6. Upper-rail and lower-rail curves are read into temporary curves (same mapping)
   used only in step 8.
7. Zero-size end sections at 0 and L (L = the model's board length from the
   outline, 1.1).
8. **Sections from deck caves.** For each deck cave i (in file order):
   position s := L − 100·A.z clamped to [0.5, L − 0.5]; base h := bottom height
   at s. All cave-derived points below are (100·x, 100·y − h).
   * Lower half: search the bottom caves (file order) for the first whose station
     differs from s by less than 2.0 cm.
     * Found (cave j): knot 1: E = A<sub>j</sub>, N = B<sub>j</sub>, P = (0,0);
       knot 2: P = D<sub>j</sub>, E = C<sub>j</sub>, N = C<sub>j</sub> + (0, 0.2).
     * Not found: pick two bottom caves "before" and "after": scanning bottom
       caves from the last record to the first, every cave whose station is < s
       sets before = after = that cave; the first cave reached whose station is
       ≥ s sets after = that cave and ends the scan (initially both are the first
       record). r := (s − station<sub>before</sub>) / (station<sub>after</sub> −
       station<sub>before</sub>), r := 1 if not finite. Centre vector :=
       (1−r)·(B−A)<sub>before</sub> + r·(B−A)<sub>after</sub>; rail vector :=
       (1−r)·(D−C)<sub>before</sub> + r·(D−C)<sub>after</sub> (all ×100).
       Knot 1: E = P = (0, 0), N = centre vector.
       Knot 2: E = (outline y at s, lower-rail height at s − h),
       P = E + rail vector, N = E + (0, 0.2).
   * Upper half from deck cave i: knot 3: E = C, P = C − (0, 0.2), N = D;
     knot 4: P = B, E = A, N = (0, 0).
   * All four knots non-continuous; the section is inserted keeping the list
     sorted by position.
9. **Sections from bottom caves.** For each bottom cave i: s as above (clamped);
   if an existing inner section (end sections excluded) lies within 2.0 cm of s,
   skip. Otherwise h := bottom height at s, and:
   * knot 1: E = A, N = B, P = (0,0); knot 2: P = D, E = C, N = C + (0, 0.2);
   * interpolate between deck caves exactly as in step 8 (same scan rule, over
     deck caves) to get vector U := lerp of (D−C) and vector V := lerp of (B−A);
   * s' := s clamped to the x-range of the upper-rail curve;
   * knot 3: E = (knot 2's E.x, upper-rail height at s' − h), P = E − (0, 0.2),
     N = E + U;
   * knot 4: E = (0, deck height at s' − h), P = E + V, N = (0,0);
   * all non-continuous; inserted sorted.
10. Continuity normalisation **both ways** (1.7).

"Height at s" of a curve = y of the curve at x = s, evaluated on the first
segment whose x-range contains s; 0 if s is outside the curve. Any read past
the end of the data or other failure → error.

---

## 9. `.cad` legacy NURBS binary

Read and written by the original ("Save As" with extension `.cad`, and Open).
It contains **only** the NURBS 3-D model, no Bezier board, and is therefore
not a board file for the new library. Documented for completeness; support is
OPTIONAL.

Big-endian (network order). Layout:

| Field | Type |
|---|---|
| version | i32 (writer: 2) |
| rows (along the board) | i32 |
| deck columns | i32 |
| bottom columns | i32 |
| bottom control points: rows × bottom columns × (x, y, z) | f64 each |
| deck control points: rows × deck columns × (x, y, −z) | f64 each (z stored negated) |
| if version > 1: tail mode | i32 |
| if version > 1: alpha, beta | f64, f64 |

Row-major (all columns of row 0, then row 1, …). Coordinates in millimetres,
x along the board, y vertical, z lateral. On reading, if deck columns > bottom
columns, the bottom grid is widened to the deck's column count by repeating its
first column twice in front and its last column twice at the end.

---

## 10. Export-only formats

Implementers write their own exporters; only content and conventions are given.

### 10.1 2-D DXF of Bezier curves ("as DXF spline")

Three variants: profile, outline, current cross-section. Each file contains two
curves:

| Variant | Curve 1 | Curve 2 |
|---|---|---|
| Profile | bottom curve | deck curve in reverse order (knot order reversed and P/N swapped), so the two form a closed side profile |
| Outline | outline (one half) | outline reversed and mirrored (y negated): the full plan shape |
| Cross-section | current section | same section reversed and mirrored (x negated): the full section |

Content: a comment group (`999`, `Dxf export from BoardCAD`), then only an
ENTITIES section (no HEADER/TABLES), one `SPLINE` entity per Bezier segment:
layer `0`, colour 9, flag 8 (planar), degree 3, 8 knots `0,0,0,0,1,1,1,1`,
4 control points E<sub>i</sub>, N<sub>i</sub>, P<sub>i+1</sub>, E<sub>i+1</sub>
with z = 0, each entity with a handle (group 5) from a counter starting at 100
that keeps counting across exports in one session. Coordinates are cm × 0.01 =
**metres**. Plane: profile (x along, y height), outline (x along, y lateral),
section (x lateral, y height).

### 10.2 2-D DXF polyline ("as DXF polyline")

Same three variants and the same curve pairs; each Bezier segment is sampled at
t = 0, 1/100, …, 99/100 (100 points per segment); the final end point of the
last segment is never emitted, so the polyline is open by one sample (D-31).
Content: R12-style `POLYLINE` (colour 9, group 66 = 1, layer 0, linetype
CONTINUOUS) with `VERTEX` entities (x, y only, metres) and `SEQEND`.
Required exporter: emit the final point and mark the polyline closed when the
two curves form a closed shape.

### 10.3 3-D DXF (from the NURBS model)

ENTITIES-only DXF with one `3DFACE` per triangle (fourth corner repeats the
third), layer 0, millimetres, frame of the NURBS model (x along, y vertical,
z lateral). Triangles come from the tessellated deck and bottom NURBS surfaces;
degenerate triangles (two equal vertices) and triangles with undefined normal
are skipped.

### 10.4 STL (from the NURBS model)

ASCII STL, `solid board` … `endsolid board`, millimetres, NURBS frame. Same
triangles as 10.3. Each facet's listed vertex order is the reverse of the order
used to compute its normal, so normals and winding disagree (D-30). Required
exporter: consistent outward normals with counter-clockwise winding seen from
outside; binary STL MAY be offered.

### 10.5 STEP

See [section 6](#6-step-stp--step-bcstep-10).

### 10.6 Partial curve files and `.brd`

See sections [3](#3-brd--native-board-file) and [5](#5-partial-curve-files-otl-pfl-crs).

### 10.7 Machine outputs

* **G-code (`.nc`, also `deck.ngc` / `bottom.ngc` for the legacy NURBS path)**:
  toolpaths for deck, bottom, outline and profile (the last two for a hot-wire
  cutter). RS-274 subset: `G21` (mm), `G90`, `G00`/`G01` moves with
  `X… Y… Z…` at 4 decimals, `F` feed, `M3`/`M5` tool on/off, `M2` end, comments
  in parentheses. Details belong to the CAM specification.
* **Atua cores (`.atua`)**: profile or outline for a hot-wire core-cutting
  machine; plain text, one `x z` pair per line (8 decimals, separated by spaces),
  comment lines verbatim. Details belong to the CAM specification.

### 10.8 Images and printouts

* Spec sheet rendered to an image file (PNG, GIF, BMP or JPEG, chosen by
  extension, JPEG if none) at a user-chosen pixel size.
* Printed templates (outline, profile, cross-sections, spin template, sandwich
  and hollow-wood templates, spec sheet) go to the system printer, not to files.

---

## 11. File-type dispatch in the original

Open: by extension, case-insensitive: `s3d` → Shape3D; `srf` → SRF; `cad` →
NURBS only; `stp`/`step` → STEP; **anything else → `.brd` reader** (which then
detects encryption by header). Save As: `cad` → NURBS binary, `stp`/`step` →
STEP export, anything else → `.brd` with the extension forced to `.brd`.
Required: the same dispatch, but unknown extensions SHOULD be sniffed by content
(`%BRD-` header, `ISO-10303-21;`, `<` XML, `p` + two digits) instead of assuming
`.brd`.

---

## 12. Known defects of the original

| Id | Area | Original behaviour | Required behaviour |
|---|---|---|---|
| D-01 | `.brd` write | p09 author, p10 blank file, p39/p40 rocker at one foot, p52 description are read but never written: data loss on save | write them (3.7) |
| D-02 | `.brd` | p05/p06 neither read nor written | ignore on read; do not write |
| D-03 | `.brd` text | LF escaped as `\n` in all text fields, unescaped only in p49 | unescape in all text fields |
| D-04 | `.brd` text | raw CR in a value breaks the line structure | normalise CR/CRLF to LF before escaping |
| D-05 | `.brd` text | values trimmed | accepted; writers trim |
| D-06 | `.brd` keys | non-numeric key digits abort the load; 3-digit keys misparsed | skip unknown/malformed keys |
| D-07 | `.brd` | duplicate curve blocks concatenate knots | last block wins, warn |
| D-08 | `.brd` | blank line inside a block corrupts the rest of the block | skip blank lines in blocks |
| D-09 | `.brd` | sections not sorted on load | sort by position |
| D-10 | `.brd` knot line | strict single-space layout; malformed line silently truncates the curve | lenient white space; malformed line is an error |
| D-11 | encrypted | fixed 12-byte header skip; unknown `%BRD-` versions parsed as plain text | validate header; unknown version is an error |
| D-12 | encoding | platform default charset for `.brd`, `.otl`, `.pfl`, `.crs`, `.srf` strings | UTF-8 write; UTF-8 read with Windows-1252 fallback |
| D-13 | `.brd` ints | `3.0` in an int field aborts the load | accept integral reals |
| D-14 | `.brd` arrays | too many elements abort the load | ignore extras, warn |
| D-15 | `.brd` | empty p07/p43 are omitted on save and come back as the non-empty defaults | write `pNN : ` for empty fields with non-empty default |
| D-16 | `.otl` import | infinite loop (application hangs); guide points appended | 5.3 |
| D-17 | `.pfl` import | only first block read; deck lost; board left damaged | 5.3 |
| D-18 | partial export | board file name replaced by exported file name | do not change file name |
| D-19 | `.crs` | exported `.crs` (bare `(p36` block) cannot be imported; import appends instead of replacing; command fails afterwards | 5.3: import replaces current section, rescaled to the board |
| D-19b | STEP header | timestamp month zero-based, no padding | ISO 8601 |
| D-20 | STEP | transformation operator directions written as zero vectors | (1,0,0), (0,1,0), origin (0,0,0) |
| D-21 | STEP | section guide points written with X = 0 instead of the position | either is accepted by readers; writing the position is preferred |
| D-22 | STEP reader | multi-line statements lose their last line; ids ≥ 10 000 and > 100 sections crash; missing `END-ISO-10303-21` or missing surfaces crash; v-knot tail filled from u knots | proper Part 21 parsing (6.10) |
| D-23 | STEP reader | file without `outline` curve leaves the previous board in place | error, board unchanged |
| D-24 | STEP | first knot's P and last knot's N cannot be stored | inherent; reader sets them to E (6.8) |
| D-25 | STEP | no board metadata stored; product named `Board name` | MAY write the board name as product name; readers use defaults |
| D-26 | `.s3d` | missing Name/Author/Comment or no Couples crash; placeholder deck formula mixes units | 7.3 |
| D-27 | `.srf` | deck knots read with the rocker's count | use deck count |
| D-28 | `.srf` | header strings limited to 500 bytes | no limit |
| D-29 | continuity test | mirror-blind angle test | 1.7 corrected test |
| D-30 | STL | facet normal and vertex winding inconsistent | consistent |
| D-31 | DXF polyline | last point missing, polyline not closed | emit last point, close |
| D-32 | STEP reader NURBS | deck tucked-under column set three times on the same index | NURBS-only; irrelevant for the Bezier board |

---

## 13. Undetermined points

* **Auxiliary knot flag** (second boolean of every knot): no code interprets it;
  its intended meaning is unknown. Preserve it.
* **p38 display unit code**: never interpreted; whether it follows the
  original's 0–4 unit enumeration is unknown.
* **Machine fields** (p13–p16, p18–p24, p29–p31, p42, p53, p99, third element of
  p27/p28): stored only; their units and exact meaning in the software that
  originated the format are not documented in the original. Treat as opaque.
* **`.srf`**: the purpose of the skipped byte ranges, of the z components, and
  whether outline measurements are half or full widths, are unknown; handle
  A/B semantics are inferred only from the original's mapping (A → previous,
  B → next after reversal). No `.srf` sample was available to verify the layout.
* **`.s3d`**: element set inferred from the reader only; no sample file was
  available. Shape3D's thickness-curve semantics are not specified.
* **NURBS surfaces in STEP**: the fitting algorithm, the chosen resolution
  (13 × 11 in the sample), and why the sample's surfaces end at X = 1800 mm for
  a 1828.8 mm board are not specified here; they do not affect the Bezier board.
* **Fin splay unit** (p50[8]): stored through a length conversion in the
  original's dialog; presumably degrees when the display unit is cm.
