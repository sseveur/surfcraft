# Foam blanks: built-in library and fit check

SurfCraft can check that a board fits inside a foam blank before you buy it (Board > Fit in blank…, MCP
`fit_blank` / `list_blanks`). This page lists where the built-in blank numbers come from and how the blank's
shape is rebuilt from them.

## How manufacturers specify a blank

| Maker | Published per blank | Units |
|---|---|---|
| US Blanks (California) | overall bottom length, tip-to-tip deck length, max width, max thickness, displacement, "natural rocker" as nose (N) and tail (T); a measurement drawing (image) and a rocker PDF per blank | inches with fractions, cm in brackets |
| Surfblanks Australia (Sydney) | blank "families" by length (5'7" to 10'7"); each blank has a rocker chart PDF with many rocker options (M08 is their standard rocker), a blank diagram book and a quick reference chart, published as PDF/images | ft/in |
| Burford (Currumbin, Queensland) | no public web catalogue found; shapers order by model code (e.g. 6'7" B, 6'8" P, 7'0" J) | ft/in |

Things to know when comparing:

* **Length.** US Blanks gives two lengths: the bottom length measured along the rocker and the tip-to-tip deck
  length, which is shorter (by ¼" to 1¼"). The library uses the **tip-to-tip deck length** (the straight length
  available, and the conservative one).
* **Rocker.** "Natural rocker N / T" is the height of the bottom at the nose and tail tips above a flat surface the
  blank rests on. Many blanks are available glued up with other rockers ("bent" stringers); the library has the
  natural rocker only. Australian charts list several rockers per blank: enter the one you order as a new blank.
* **Thickness** is the maximum (near the centre). Blanks thin toward the ends; how much is not published as a
  number, so SurfCraft assumes a taper (below).
* **Widths 12" from the tips and the wide point position** are only on the measurement drawings (images), not in
  the page text, so the library leaves them out and SurfCraft estimates them (below). Enter them for a new blank
  when you have them.
* A few US Blanks pages give cm values that do not match their inch values exactly (6'4" EA, 6'5" R, 8'0" H,
  10'2" B, differences up to 0.7 cm). The library uses the inch values.

## Library

Lengths are the tip-to-tip deck length. All values as published (inches); retrieved November 2026.

| Library name | Length | Width | Thickness | Nose rocker | Tail rocker | Source |
|---|---|---|---|---|---|---|
| 5'10" fish (US Blanks 5'10" RP) | 5' 10 3/4" | 24 7/8" | 2 15/16" | 4 5/8" | 1 15/16" | https://usblanks.com/catalog/fish-eggs/510rp/ |
| 6'0" shortboard (US Blanks 6'0" R) | 5' 11 3/4" | 21 3/16" | 3 1/16" | 4 7/16" | 2 1/8" | https://usblanks.com/catalog/shortboard/60r/ |
| 6'2" shortboard (US Blanks 6'2" P) | 6' 1 7/8" | 20 1/2" | 2 1/2" | 4 7/16" | 2 1/4" | https://usblanks.com/catalog/shortboard/62p/ |
| 6'4" shortboard (US Blanks 6'4" EA) | 6' 3 1/2" | 20 5/8" | 2 3/4" | 5 1/2" | 2 1/16" | https://usblanks.com/catalog/shortboard/64ea/ |
| 6'5" shortboard (US Blanks 6'5" R) | 6' 4 3/4" | 21 3/4" | 3 1/4" | 4 11/16" | 2 1/4" | https://usblanks.com/shortboard/65r/ |
| 6'8" fish / hybrid (US Blanks 6'8" RP) | 6' 8 3/4" | 25 1/8" | 3" | 5" | 2 1/8" | https://usblanks.com/fish-eggs/68rp/ |
| 7'5" funboard (US Blanks 7'5" A) | 7' 3 7/16" | 21 7/8" | 3" | 6" | 2 1/8" | https://usblanks.com/fun-boards/75a/ |
| 7'8" step-up (US Blanks 7'8" EA) | 7' 8 3/16" | 21 7/8" | 3" | 6 3/16" | 2 5/8" | https://usblanks.com/guns/78ea/ |
| 8'0" mid-length (US Blanks 8'0" H) | 7' 11 7/8" | 23 5/8" | 3 5/16" | 4 5/8" | 1 15/16" | https://usblanks.com/fish-eggs/80h/ |
| 9'2" gun (US Blanks 9'2" A) | 9' 2" | 22 1/4" | 3 1/2" | 7 5/8" | 2 3/4" | https://usblanks.com/catalog/guns/92a/ |
| 9'4" longboard (US Blanks 9'4" B) | 9' 3 1/16" | 25 1/16" | 3 5/16" | 4 3/4" | 3 1/2" | https://usblanks.com/longboards/94b/ |
| 10'2" longboard (US Blanks 10'2" B) | 10' 1 1/4" | 25 3/16" | 3 1/2" | 4 13/16" | 3 7/16" | https://usblanks.com/longboards/102b/ |

Generic presets, clearly **not** manufacturer numbers: values interpolated between the catalogue blanks above, for
a first check when your blank is not in the list.

| Library name | Length | Width | Thickness | Nose rocker | Tail rocker |
|---|---|---|---|---|---|
| Generic 5'8" shortboard (estimate) | 5' 9" | 21" | 2 3/4" | 4 1/4" | 2" |
| Generic 7'0" step-up (estimate) | 7' 1/2" | 21 1/4" | 3" | 5 3/4" | 2 3/8" |
| Generic 8'6" mid-length (estimate) | 8' 6" | 23 1/2" | 3 3/8" | 5" | 2 1/2" |
| Generic 9'8" longboard (estimate) | 9' 8" | 25" | 3 3/8" | 4 3/4" | 3 1/2" |

Australian blanks (Surfblanks Australia, Burford) are not in the built-in list: their dimensions are published as
PDF charts and drawings that could not be read as text, and no number is included without a source. Add the blank
you order with Fit in blank > New blank…, typing the values from the chart in mm, cm or inches; user blanks are
saved with the settings.

## How the blank's shape is rebuilt

From the few published numbers SurfCraft builds an approximate outline, bottom and deck (`surfcraft-core`
`blank.rs`):

* **Outline**: from the wide point each side narrows as a power of the distance, with the exponent that passes
  through the width 12" from the tip. Without published 12" widths they are estimated as 62 % (nose) and 74 %
  (tail) of the widest width. The nose closes to a point; the tail ends at a width extrapolated from the 12"
  station, with rounded corners. The wide point is at the middle unless given.
* **Bottom (rocker)**: zero at a flat point, rising to the published nose and tail rocker as a power 2.5 of the
  distance (or the exponent through the innermost published 12"/24" rocker station; published stations are
  matched exactly). The flat point sits where both sides have the same curvature scale (a little behind the
  middle on most blanks).
* **Deck**: bottom plus a thickness that is the published thickness at the middle and thins toward the tips to
  50 % of it (power 2.5 of the distance from the middle), unless the thickness 12" from a tip is given.

The fit check compares, every centimetre along the board, the board's outline (per side) with the blank's, and
the board's deck and bottom on the stringer with the blank's, with the board shifted along the blank, tilted in
the profile plane about its middle, and centred in height (equal smallest clearance above and below). The board
must also be shorter than the blank at both ends. Auto fit searches the shift and the tilt (±6°) for the largest
smallest clearance. Because the blank shape is an estimate, keep a few millimetres of margin, especially near the
tips and for the deck where the real taper of the mould is unknown.
