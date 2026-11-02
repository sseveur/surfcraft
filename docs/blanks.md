# Foam blanks: built-in library and fit check

SurfCraft can check that a board fits inside a foam blank before you buy it (Board > Fit in blank…, MCP
`fit_blank` / `list_blanks`). This page lists where the built-in blank numbers come from and how the blank's
shape is rebuilt from them.

## How manufacturers specify a blank

| Maker | Published per blank | Units |
|---|---|---|
| US Blanks (California) | page text: overall bottom length, tip-to-tip deck length, max width, max thickness, displacement, "natural rocker" nose (N) / tail (T). A "with Measurements" drawing per blank gives width, stringer thickness and rocker at the tips and 6", 12", 18", 24" (and 36", 48" on longer blanks) from each tip, and at the centre. A PDF lists the other rockers the blank is glued up with. | inches with fractions, cm in brackets |
| Surfblanks Australia (Brookvale, NSW) | a drawing per blank (the product image): max length, max width, max thickness, volume, and width / stringer thickness / rocker at stations labelled by their distance from the tail (12", 24", the wide point, 12" and 24" from the nose, sometimes 6" and 48"), the position of the lowest rocker point, and the natural (moulded) rocker; glued-in rockers (M08 is the most popular) are in a separate rocker chart | ft/in |
| Burford (Currumbin, Queensland) | no public web catalogue found; shapers order by model code (e.g. 6'7" B, 6'8" P, 7'0" J) | ft/in |

Things to know when comparing (Australian vs US catalogues):

* **Stations.** US Blanks measures every station from its own tip (0", 6", 12", 18", 24", 36", 48" from the nose
  and from the tail). Surfblanks Australia labels every station by its distance from the **tail** along the
  stringer; its nose stations fall 12" and 24" from the nose (e.g. 5'4.8" and 4'4.8" on the 6'4" SB, whose tip is
  then at 6'4.8"), so they are measured along the curved bottom, while "Max length" (6'4") is the straight length.
  The library uses the published max length and puts the nose stations 12" / 24" from the nose.
* **Rocker.** Surfblanks Australia also gives where the lowest rocker point is (38.4" from the tail on the 6'7" SB);
  US Blanks does not. Both show the natural (moulded) rocker; other rockers are glued in to order.
* **Width.** For the same nominal length the Australian shortboard blanks are wider here (6'4" SB: 22 1/2" vs US
  Blanks 6'4" EA: 20 5/8"), and each drawing lists the blank's volume (51.6 L for the 6'4" SB).
* **Units.** Both publish inches; nothing metric to convert.

* **Length.** US Blanks gives the bottom length measured along the rocker and the tip-to-tip deck length, which
  is shorter (by ¼" to 1¼"). The library uses the **tip-to-tip deck length**: the straight length available, the
  conservative choice.
* **Rocker** is the height of the bottom above its lowest point (the blank resting on a flat table) at the tip and
  at each station. The library has each blank's natural (standard) rocker; most blanks can be ordered glued up
  with another rocker. Australian charts list several rockers per blank: enter the one you order as a new blank.
* **Thickness** at the stations is on the stringer; the middle value is the maximum.
* **Stations** are measured from each tip. The widths at "0"" are at the very end of the blank (a blunt nose
  4"-8" wide, a tail 6"-14" wide); round-nosed blanks (longboards, 8'0" H) have no width at the nose tip.
* **Inconsistencies in the source pages** (the library takes the conservative or the self-consistent value):
  * 6'0" R: page max width 21 3/16", drawing centre width 21 5/8"; page thickness 3 1/16", drawing 3". Library:
    21 3/16" and 3".
  * 8'0" H: page rocker "4 5/8" N / 1 15/16" T" (the same text as the 5'10" RP page), drawing 3 9/16" N /
    3 1/8" T with matching stations. Library: the drawing.
  * 9'2" A: drawing centre width label "21 1/4"" but 56.52 cm and the page say 22 1/4". Library: 22 1/4". Its
    18" nose thickness reads "2 5/8"" with 5.78 cm (2 9/32"): left out.
  * 7'5" A: 12" tail rocker reads "1 3/16"" with 2.06 cm (13/16"); the library takes 1 3/16" (monotone with
    its neighbours).
  * 5'10" RP: tail tip thickness reads "1 15/16"" with 3.33 cm (1 5/16"); library 1 5/16".
  * A few pages give cm values that do not match their inches (6'4" EA, 6'5" R, 8'0" H, 10'2" B lengths,
    differences up to 0.7 cm). The library uses the inches.

## Library

Main dimensions (tip-to-tip length, widest width, thickness at the middle, natural rocker). Every US Blanks entry
also carries its full station table (widths, thicknesses, rockers) from the measurement drawing linked in the
last column. Retrieved November 2026.

| Library name | Length | Width | Thickness | Nose rocker | Tail rocker | Page | Drawing |
|---|---|---|---|---|---|---|---|
| 5'10" fish (US Blanks 5'10" RP) | 5' 10 3/4" | 24 7/8" | 2 15/16" | 4 5/8" | 1 15/16" | https://usblanks.com/catalog/fish-eggs/510rp/ | https://usblanks.com/wp-content/uploads/2022/02/510RP-with-Measurements.png |
| 6'0" shortboard (US Blanks 6'0" R) | 5' 11 3/4" | 21 3/16" | 3" | 4 7/16" | 2 1/8" | https://usblanks.com/catalog/shortboard/60r/ | https://usblanks.com/wp-content/uploads/2022/02/60R-with-Measurements.png |
| 6'2" shortboard (US Blanks 6'2" P) | 6' 1 7/8" | 20 1/2" | 2 1/2" | 4 7/16" | 2 1/4" | https://usblanks.com/catalog/shortboard/62p/ | https://usblanks.com/wp-content/uploads/2022/02/62P-with-Measurements.png |
| 6'4" shortboard (US Blanks 6'4" EA) | 6' 3 1/2" | 20 5/8" | 2 3/4" | 5 1/2" | 2 1/16" | https://usblanks.com/catalog/shortboard/64ea/ | https://usblanks.com/wp-content/uploads/2022/02/64EA-with-Measurements.png |
| 6'5" shortboard (US Blanks 6'5" R) | 6' 4 3/4" | 21 3/4" | 3 1/4" | 4 11/16" | 2 1/4" | https://usblanks.com/shortboard/65r/ | https://usblanks.com/wp-content/uploads/2022/02/65R-with-Measurements.png |
| 6'8" fish / hybrid (US Blanks 6'8" RP) | 6' 8 3/4" | 25 1/8" | 3" | 5" | 2 1/8" | https://usblanks.com/fish-eggs/68rp/ | https://usblanks.com/wp-content/uploads/2022/02/68RP-with-Measurements.png |
| 7'5" funboard (US Blanks 7'5" A) | 7' 3 7/16" | 21 7/8" | 3" | 6" | 2 1/8" | https://usblanks.com/fun-boards/75a/ | https://usblanks.com/wp-content/uploads/2022/03/75A-with-Measurements.png |
| 7'8" step-up (US Blanks 7'8" EA) | 7' 8 3/16" | 21 7/8" | 3" | 6 3/16" | 2 5/8" | https://usblanks.com/guns/78ea/ | https://usblanks.com/wp-content/uploads/2022/03/78EA-with-Measurements.png |
| 8'0" mid-length (US Blanks 8'0" H) | 7' 11 7/8" | 23 5/8" | 3 5/16" | 3 9/16" | 3 1/8" | https://usblanks.com/fish-eggs/80h/ | https://usblanks.com/wp-content/uploads/2022/04/80H-with-Measurements.png |
| 9'2" gun (US Blanks 9'2" A) | 9' 2" | 22 1/4" | 3 1/2" | 7 5/8" | 2 3/4" | https://usblanks.com/catalog/guns/92a/ | https://usblanks.com/wp-content/uploads/2022/03/92A-with-Measurements.png |
| 9'4" longboard (US Blanks 9'4" B) | 9' 3 1/16" | 25 1/16" (48" from the nose) | 3 5/16" | 4 3/4" | 3 1/2" | https://usblanks.com/longboards/94b/ | https://usblanks.com/wp-content/uploads/2022/03/94B-with-Measurements.png |
| 10'2" longboard (US Blanks 10'2" B) | 10' 1 1/4" | 25 3/16" (48" from the nose) | 3 1/2" | 4 13/16" | 3 7/16" | https://usblanks.com/longboards/102b/ | https://usblanks.com/wp-content/uploads/2022/03/102B-with-Measurements.png |

Surfblanks Australia, from each blank's drawing (the product image on its page), retrieved November 2026:

| Library name | Max length | Max width (from the tail) | Max thickness | Nose rocker | Tail rocker | Page | Drawing |
|---|---|---|---|---|---|---|---|
| 6'4" shortboard (Surfblanks Australia 6'4" SB) | 6' 4" | 22 1/2" (38 3/8") | 2 3/4" | 4 7/8" | 1 3/4" | https://surfblanksaustralia.com/products/6-4-shortboard-rocker-chart | https://surfblanksaustralia.com/cdn/shop/products/0604SB_1024x1024.jpg |
| 6'5" fat fish (Surfblanks Australia 6'5" FF) | 6' 6" | 25" (41 3/16") | 3 3/4" | 4 1/2" | 2 1/2" | https://surfblanksaustralia.com/products/6-5-fat-fish-rocker-chart | https://surfblanksaustralia.com/cdn/shop/products/0605FF_1024x1024.gif |
| 6'7" shortboard (Surfblanks Australia 6'7" SB) | 6' 8" | 20 3/4" (38 7/8") | 2 13/16" | 5 3/16" | 2 1/2" | https://surfblanksaustralia.com/products/6-7-shortboard-rocker-chart | https://surfblanksaustralia.com/cdn/shop/products/0607SB_1024x1024.gif |
| 8'3" mid-length (Surfblanks Australia 8'3" LB) | 8' 3 1/2" | 24 1/2" (53 9/16") | 3 9/16" | 5 3/8" | 3 3/16" | https://surfblanksaustralia.com/products/8-3-longboard-rocker-chart | https://surfblanksaustralia.com/cdn/shop/products/0803LB_1024x1024.gif |
| 9'2" longboard (Surfblanks Australia 9'2" LB) | 9' 2 1/2" | 24 5/16" (48" from the nose) | 3 3/8" | 4" | 3 1/2" | https://surfblanksaustralia.com/products/9-2-longboard-rocker-chart | https://surfblanksaustralia.com/cdn/shop/products/0902LB_1024x1024.gif |

Left out of the Australian entries: the 8'3" LB nose-tip width ("12 15/16"" printed at the apex of a round nose)
and the 9'2" LB nose-tip width ("0"", a point); both noses are rebuilt as round ends from the 12" and 24" widths.
The 6'7" SB centre width (20 11/16" at 38 7/8") is just under its max width and is not used.

Generic presets, clearly **not** manufacturer numbers: values interpolated between the catalogue blanks above, for
a first check when your blank is not in the list. Their stations come from the typical proportions below.

| Library name | Length | Width | Thickness | Nose rocker | Tail rocker |
|---|---|---|---|---|---|
| Generic 5'8" shortboard (estimate) | 5' 9" | 21" | 2 3/4" | 4 1/4" | 2" |
| Generic 7'0" step-up (estimate) | 7' 1/2" | 21 1/4" | 3" | 5 3/4" | 2 3/8" |
| Generic 8'6" mid-length (estimate) | 8' 6" | 23 1/2" | 3 3/8" | 4 3/4" | 3 1/4" |
| Generic 9'8" longboard (estimate) | 9' 8" | 25" | 3 3/8" | 4 3/4" | 3 1/2" |

Not in the built-in list: the other Surfblanks Australia sizes (their pages limit automated requests; the drawings
are on each blank's page), Surfblanks' glued-in rockers (the rocker chart PDF link returned "not found"), and
Burford blanks (no public catalogue). Add the blank you order with Fit in blank > New blank…, typing the values
from the drawing in inches, cm or mm; user blanks are saved with the settings.

## How the blank's shape is rebuilt

From the published numbers SurfCraft builds an approximate outline, bottom and deck (`surfcraft-core`
`blank.rs`). Every curve is a monotone cubic through the published points (it never overshoots them):

* **Outline** through the wide point and the station widths. The tips close with small rounded corners when
  the tip width is published, or with a round end when it is extrapolated from the two stations nearest the tip.
* **Bottom (rocker)** through the tip rockers and the station rockers, zero at a flat point between the
  innermost stations (nearer the lower one).
* **Deck**: bottom plus the thickness through the middle thickness and the station thicknesses.

Where a side has no station data (generic presets, user blanks with only the main numbers), typical proportions
measured on the US Blanks drawings above are used:

| | Nose | Tail |
|---|---|---|
| Width 12" from the tip | 70 % of the widest | 74 % |
| Width at the tip / 24" | 33 % of the 12" width / 70 % of the way from the 12" width to the widest | 54 % / 70 % |
| Thickness at the tip, 12", 24" | 37 %, 67 %, 88 % of the middle | 43 %, 69 %, 87 % |
| Rocker | power 2.7 of the distance from the middle | power 2.2 |

The fit check compares, every centimetre along the board, the board's outline (per side) with the blank's, and
the board's deck and bottom on the stringer with the blank's, with the board shifted along the blank, tilted in
the profile plane about its middle, and centred in height (equal smallest clearance above and below). The board
must also be shorter than the blank at both ends. Auto fit searches the shift and the tilt (±6°) for the largest
smallest clearance. The blank between stations is an interpolation and blanks vary from one pour to the next:
keep a few millimetres of margin. Off the stringer the check assumes the blank's full thickness reaches the board's
rail; blanks have rounded rails, so a very full rail right at the blank's edge needs a look on the real blank.
