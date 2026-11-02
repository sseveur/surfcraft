# Shape menu: bottom contours and rails

Both tools change every cross-section in a range at once, as one undo step, and are also available to the AI
assistant as the `apply_bottom_contour` and `set_rails` commands. Code: `crates/surfcraft-session/src/shaping.rs`
(geometry, unit tests), `crates/surfcraft-app/src/shaping_ui.rs` (windows).

Each cross-section is a half section in its own frame: bottom centre (on the stringer, at the rocker height) >
rail > deck centre. The surface model scales every section to the outline width and stringer thickness at its
station, so the tools keep the two centre points and the widest point of each section: width, stringer thickness,
rocker and deck line never change.

## Bottom contours (Shape > Bottom contours…)

A contour zone has a type, a depth, a start and end (from the tail) with fade-in and fade-out lengths, and a
*run-out*: where the contour ends across the board, either a distance in from the rail edge (default 1") or a
fraction of the local half-width. Several zones can be stacked (applied in order; a later zone blends over an
earlier one where they overlap, e.g. a double concave fading in over a single). Before shaping, interpolated
sections are inserted at the zone ends, where the fades end and at most "section spacing" apart (default 15 cm),
so the contour has sections to follow.

**What stays: rails or stringer rocker.** Each zone has a *Keep* choice:

- **Rails (cut foam, like shaping a blank)**, the default. The rail edge stays where it is in the board and foam
  is removed, as a shaper cuts a contour into a blank. A concave raises the stringer rocker inside the zone (the
  stringer rocker deepens, the deck stays, so the thickness on the stringer drops by the depth plus the old rise of
  the bottom to the run-out); a vee and channels cut towards the rails with the stringer left alone. The volume
  always goes down. The stringer rocker is raised as a fair curve: the lift follows the zone's fades
  (smoothstep), fades out over the last 5 cm at the tail and nose tips and where the board gets thin, and a zone
  that starts or ends inside the board fades over at least 10 cm. The lowest rocker point is then put back at zero,
  as after any rocker edit.

  The rocker keeps its own knots so it stays easy to edit by hand: each knot moves up by the lift at its station
  and its handles turn by the lift's slope (smooth knots stay smooth). New knots go only where the zone starts,
  reaches full depth, starts fading and ends, plus one wherever the result would still stray more than 0.3 mm
  from the intended rocker; a zone typically adds 3-4 knots (a flat zone over the whole board up to 6). Each
  section is then cut by the lift the refitted rocker really has at its station, so the rail edge stays exactly
  where it was.

**Knots on a contoured section.** The bottom from the stringer to the run-out is refitted with the fewest knots
that stay within 0.2 mm of the contour shape: usually 2 (stringer and run-out) for a single concave, vee or flat
bottom, 4 for a double concave, plus four per channel (one at each end of each channel wall).
- **Stringer rocker**. The rocker and the thickness on the stringer stay; the bottom moves near the rails instead
  (a concave lowers it there and adds a little volume, a vee raises it). This is how CAD programs that define
  the rocker on the stringer behave.

**How depth is measured.** As a shaper measures it, with a straight edge laid across the bottom between the two
run-out points. Relative to the stringer:

| Type | Shape between stringer and run-out | Run-out height |
|---|---|---|
| Flat | straight and level | at the stringer |
| Single concave | smooth dish (cosine), level at both ends | `depth` below the stringer |
| Double concave (inside a single) | the single concave plus two dishes of *inner depth* either side of the stringer (inner 80 % of the run-out width), leaving a low rounded ridge on the stringer | `depth` below the stringer |
| Vee | nearly straight panels from a ridge on the stringer, easing level into the run-out | `depth` above the stringer |
| Channels | grooves of `depth` and *channel width* cut into the existing bottom, outermost one ending at the run-out, separated by flats one channel wide (count = channels per side) | unchanged |

At a fade factor *f* the bottom is `(1 - f) * old + f * contour`, so applying the same contour twice gives the same
board (except channels, which cut into whatever bottom is there). The shape is bent slightly so it leaves the
run-out along the original bottom (no kink at the run-out). Outboard of the run-out the rail keeps its shape: when
the run-out moves relative to the rail (stringer mode, or a vee), the part below the rail apex is stretched or
compressed vertically to meet it; the apex and the deck do not move.

**Volume.** Keeping the rails (cutting foam), every contour removes volume: on the built-in 31 L shortboard the
default zones remove about 0.7 L (1/8" single concave over two thirds of the length), 0.5 L (double concave),
0.3 L (vee) and 0.2 L (channels). Keeping the stringer rocker, a concave
lowers the bottom towards the rails and *adds* volume, while a vee and channels remove volume. On the built-in
31 L shortboard, keeping the stringer: a 1/8" single concave over two thirds of the length adds about 1.1 L
(+3.6 %); a 1/4" vee in the last quarter removes about 0.3 L; two 3/16" channels per side in the last 30 % remove
about 0.2 L. Use Board > Scale (target volume) afterwards if the volume must stay. *Remove contours* (Flatten the
bottom) makes the bottom straight and level from the stringer to the run-out in a range, with the first zone's
Keep choice: cutting foam, whichever of the stringer and the run-out is higher sets the level.

Defaults when a zone is created for a board: single concave 1/8" from the tail to 65 % of the length, fading out
over the last 25 %; double concave 1/8" (inner 1/16") over the last 40 %; vee 1/4" over the last 25 %; channels
3/16" deep, 1" wide, 2 per side, over the last 30 %.

## Rails (Shape > Rails…)

The rail of each section is rebuilt from four knots between the bottom near the edge and the deck at the *rail
band* in from the edge: bottom, edge (tuck), apex (at the widest point, so the width is kept), deck. The bottom
inboard of the rail and the deck inboard of the band are untouched, so a bottom contour stays.

- **Apex height**: fraction of the rail height, measured from the bottom. The rail height is from the bottom line
  (extended to the edge) up to the deck at the rail band. 0.5 = 50/50, 0.4 = 60/40 (60 % of the rail above the
  apex), 0.3 = 70/30, 0.2 = 80/20.
- **Softness**: 0 = hard edge (the rail leaves the bottom at about 70°, a corner knot; a small release angle in the
  Section view), 1 = soft, round rail (the rail rises tangentially from the bottom; "soft rail (no release edge)").
- **Tuck**: how far the edge sits inboard of the apex (tucked-under edge). A soft rail rounds further in.
- **Rail band**: where the rail meets the deck, measured in from the edge.

Presets: 50/50 (apex 0.5, soft 0.9, tuck 3 mm), 60/40 (0.4, 0.55, 6 mm), 70/30 (0.3, 0.3, 8 mm), 80/20 (0.2,
0.1, 10 mm). With *Blend tail, middle and nose*, the tail spec applies up to 20 % of the length, the middle spec at
half, the nose spec from 80 %, blended linearly in between. The window and the Section view show the result with
the same apex and tuck-under markers the Section view uses (normal angles 90° and 175°).

Rails change the volume a little (harder, lower-apex rails remove foam: about -0.3 L for 80/20 on the shortboard).

## Typical values (research)

- Single concave: about 1/8" is common and mild; 1/4"-3/8" is deep ("anything more than 3/8" is pretty deep").
  Double concaves inside a single are shallow, about 1/8" with the stringer about 1/16" higher; vee out the tail is
  usually shallow. Concaves are often run out an inch or so from the rail. [1] [2] [3] [4]
- Rails: the ratio gives the apex position, deck side first: 50/50 has the apex in the middle; 60/40 lower; 80/20
  so low it makes a sharp edge. Hard rails have a defined edge, soft rails a rounded curve; the tucked-under edge is
  where the bottom meets the elliptical curve of the rail. Conventions vary between shapers. [5] [6] [7] [8]
- Channels: typically 4-8 grooves in total, deepest at the tail exit; older four-channel twins had channels about
  1/2" deep. [9] [10]

These are forum and magazine figures, not standards. Pages [5] and [7] were read in full; the others were consulted
through search-result excerpts (several forum pages refuse automated access).

1. Surfer Today, *The complete guide to surfboard bottom contour designs*:
   https://www.surfertoday.com/surfing/the-complete-guide-to-surfboard-bottom-contour-designs
2. Swaylocks, *Help designing winter board* (1/8" double concaves, stringer about 1/16"):
   https://forum.swaylocks.com/t/help-designing-winter-board/31211
3. Swaylocks, *Bottom contours 101*: https://forum.swaylocks.com/t/bottom-contours-101/31055 and *Double concave
   to vee out tail specifics*: https://forum.swaylocks.com/t/double-concave-to-vee-out-tail-specifics/38519
4. RealSurf forum, *Concave depth*: https://forum.realsurf.com/forums/viewtopic.php?p=397120
5. Foam Magazine, *The difference between soft rails vs. hard rails*: https://foammagazine.com/surfboard-rails/
6. Surfer Today, *The different types of surfboard rails*:
   https://www.surfertoday.com/surfing/the-different-types-of-surfboard-rails and Swaylocks, *Definition of 70/30
   etc, numeric rail descriptions*: https://forum.swaylocks.com/t/definition-of-70-30-etc-numeric-rail-descriptions/4141
7. EOS, *Tucked-under edge*: https://www.eos.surf/encyclopedia/tucked-under-edge
8. Ombe, *Surfboard rails*: https://www.ombe.co/guides/surfboard-rails
9. US patent 10974797, *Surf craft having an improved channel*: https://patents.google.com/patent/US10974797
10. Swellnet, *Which channels are better?*: https://www.swellnet.com/comment/899748
