# Fin setups and fin boxes: dimensions and sources

The fin presets and box footprints in SurfCraft (`crates/surfcraft-core/src/fins.rs`) use the numbers below.
Each number has a source. Where sources disagree, the table lists them all and says which one is used.

**The manufacturers do not publish most box footprint dimensions.** FCS and Futures expect boxes to be routed
with their own jigs, aligned on the fin's trailing edge and centre line. SurfCraft draws the box outlines to
show where the boxes go. On the printed template, the + mark (the trailing edge) and the centre line are the
references to set a jig on. Measure your own boxes before routing to the printed outlines. Values marked
**approx.** are forum measurements or estimates, not specifications.

Last researched November 2026. 1 in = 25.4 mm.

## Conventions used in SurfCraft

| Quantity | Definition |
|---|---|
| From tail | Tail to the fin's trailing edge (rear end of the fin base), along the stringer. FCS II jigs are set on this point, the "shaper's dot" ([FCS II install guide](https://www.cfsnet.co.uk/content/downloads/FCSII_Installation_Instructions.pdf), pp. 12–13). Futures guides also measure to the fin's trailing edge, not the box flange ([Swaylocks](https://forum.swaylocks.com/t/fin-boxes-with-futures/51256)). |
| From rail | Rail to the trailing edge, square to the stringer. Lundquist measures it at the rear of the fin ([Lundquist fin placement guide](https://lundquistsurfboards.com/build-guide/fin-placement-guide)). |
| Toe | How much closer to the stringer the leading edge is than the trailing edge. Guides give it as a distance (e.g. 1/4"). SurfCraft stores it as an angle: angle = asin(distance / base length). |
| Cant | The fin tip tilted out towards the rail, in degrees. |
| Box position | Each footprint is centred on the middle of the fin base and lies along the fin. A fin in a US box sits in the middle of the box, so it can slide either way. |
| US box presets | Distances are given to the rear of the box ([FGH chart](http://fiberglasshawaii.com/How%20To_DIY/FGHFinPlacementChart042020.pdf)). The fin's trailing edge is then (box length − fin base) / 2 further forward. |

## Box systems

### FCS I (original / X-2: two round plugs per fin)

| Dimension | Used | Source / notes |
|---|---|---|
| Hole diameter | 1 1/8" (28.6 mm) | Original plug: 1 1/8" hole saw (Suncure instructions quoted on [Swaylocks](https://swaylocks.com/t/5458); FCS kit setting on [Swaylocks](https://swaylocks.com/t/18222)); some shapers use 1 1/4". Plug outer diameter about 26 mm, height 20 mm (measured drawing, [Swaylocks](https://swaylocks.com/t/2196)). X-2 plugs use a 30 mm saw ([viral-surf, retailer](https://www.viral-surf.com/en/2907-fcs-fins-plugs-and-tools)). SurfCraft draws the 28.6 mm hole. |
| Hole depth | 7/8" (22.2 mm) | Original plug: plug height plus about 1/8" ([Swaylocks](https://swaylocks.com/t/18222)). X-2 (set after the seal coat): 5/8" (15.9 mm) ([Greenlight](https://greenlightsurfsupply.com/pages/how-to-install-factory-and-fcs-x-2-fin-plugs)). |
| Plug centre to centre | 53 mm, **forum measurements** | Two independent measurements agree: plugs 53 mm apart, 75 mm overall ([Swaylocks](https://swaylocks.com/t/2196)); fin tabs 20 + 33 + 20 mm ([Swaylocks](https://swaylocks.com/t/21399)). Not published by FCS: set the plugs with FCS dummy fins or a fin, as FCS instructs. |
| Plug position along the fin | Symmetric about the middle of the base, **approx.** | Not published. |
| Fin base | 4.37" (111 mm) | FCS M5 ([Fiberglass Supply, FCS specs](https://fiberglasssupply.com/fcs/)). Other bases: M3 4.28", M7 4.52", GL 3.4", GX 3.86". |
| Cant | in the plug | X-2 rail plugs 4° ([Fiberglass Supply](https://fiberglasssupply.com/fcs-rail-plug/)). Fusion plugs 0°, 5°, 9° ([Fiberglass Supply](https://fiberglasssupply.com/fcs/)). |

### FCS II

| Dimension | Used | Source / notes |
|---|---|---|
| Plug length × width | 110 × 37.5 mm | FCS figures for the maximum plug size, quoted by [Sanded Australia](https://www.sanded.com.au/blogs/news/q-a-about-the-fcs2-fin-system). The plug has two lobes with a waist of about 25–26.5 mm ([Swaylocks](https://forum.swaylocks.com/t/fcs-ii-plug-dimensions/33543); a router jig modelled on the plug, [Gecko Surfboards](https://www.geckosurfboards.com/surfboard-building-router-jigs/), gives 114.4 × 37.2 mm with a 25.2 mm waist, including end clearance). SurfCraft draws the 110 × 37.5 mm envelope with round ends, which encloses the plug. |
| Routing depth | not given as a number | FCS: set the cutter 0.5–1 mm above the plug's top flange, and seat the plug 0.5 mm below the foam ([FCS II install guide](https://www.cfsnet.co.uk/content/downloads/FCSII_Installation_Instructions.pdf), pp. 10–11). |
| Jig reference | trailing edge | The jig crosshair goes on the trailing-edge dot. A 5" (125 mm) line toward the nose sets the toe (same guide, pp. 12–13). For fins with bases over 117 mm, a separate "router dot" offset applies ([FCS router-dot sheet](https://www.cfsnet.co.uk/content/downloads/FCSII_ROUTER_DOT_Keels_and_Twins_NEW.pdf)). |
| Plug cants | 0° (centre), 3°, 5°, 9° | FCS II install guide, p. 5. Thrusters: rails 9°, centre 0°. Quads: fronts 9°, rears 3° or 5° (pp. 6–8). |
| Fin base | 4.37" (111 mm) | FCS II Performer M ([Fiberglass Supply](https://fiberglasssupply.com/fcs/)). Other bases: Performer S 4.25", Carver M 4.47", Carver L 4.58", Performer M quad rear 4.09". |

### Futures

| Dimension | Used | Source / notes |
|---|---|---|
| Box footprint | 5 7/8" × 1 1/4" (149.8 × 31.8 mm), round ends (R 5/8") | The current Futures box is the ILT box; its routed flange is the same for the 1/2" and 3/4" boxes ([Cleanline](https://cleanlinesurf.com/products/futures-fins-ilt-fin-box)). The Gecko router jig for the box ([Gecko Surfboards](https://www.geckosurfboards.com/surfboard-building-router-jigs/)) gives 149.8 × 31.8 mm after removing the 16 mm guide bushing offset for a 1/4" bit (±1 mm), and the [FGH chart](http://fiberglasshawaii.com/How%20To_DIY/FGHFinPlacementChart042020.pdf) shows the round ends. Inner slot (deeper pass) about 130 × 12.8 mm (jig, single source). An earlier version of SurfCraft used 4 11/16" × 1", which was wrong. |
| Corner radius | 5/8" | Fully round ends. |
| Depth | not drawn | Futures boxes come in 1/2" (centre and small quad rears) and 3/4" (side fins) depths. These are fin-tab depths ([Fiberglass Supply](https://fiberglasssupply.com/futures-3-4-ilt-white-box-f/)). Routing depth for the 3/4" box: 7/8" (22 mm), single source ([Swaylocks](https://swaylocks.com/t/51461)); not found for the 1/2" box. Fin tab length 113.5 mm ([Swaylocks](https://swaylocks.com/t/2196)). |
| Cant | in the fin | Futures boxes are 0°; the cant is moulded into the fin tab ([Swaylocks](https://forum.swaylocks.com/t/futures-fins-built-in-cant/37217)). |
| Fin base | 4.5" | Retailer listings give 4.5" to 4.6". |

### US box (single fin, longboard centre)

| Dimension | Used | Source / notes |
|---|---|---|
| Length × width × depth | 26.6 × 2.5 × 2.5 cm | Fins Unlimited 10.5" box ([Cleanline](https://www.cleanlinesurf.com/products/fins-unlimited-10-5-center-fin-box)). A forum measurement agrees: width 1", inside depth 15/16", slot 11/32" ([Swaylocks](https://forum.swaylocks.com/t/longboard-fin-box-dimensions/51618)). Routing for a 10" box: 1 1/8" × 10 1/8" ([Swaylocks](https://forum.swaylocks.com/t/installing-fin-boxes/7777)). |
| Corner radius | 1/8" | From a 1/4" bit. |
| Fin base | 163 mm | 9" longboard fin (FCS II Connect 9", [surffcs.com.au](https://www.surffcs.com.au/products/fcs-ii-connect-sf-longboard-fin)). |
| Other boxes | not modelled | FCS longboard box 10.5" ([hartbeach.nl](https://hartbeach.nl/en/products/fcs-longboard-box-10-5)); Futures 1-Shot 10.75" ([Fiberglass Supply](https://fiberglasssupply.com/surf-sail-components/futures-fins/futures-boxes/)). Their widths and depths are **unconfirmed**. |

## Placement by setup

Sources:
- **L**: [Lundquist fin placement guide](https://lundquistsurfboards.com/build-guide/fin-placement-guide), including the McKee table.
- **FGH**: [Fiberglass Hawaii Futures placement chart](http://fiberglasshawaii.com/How%20To_DIY/FGHFinPlacementChart042020.pdf), measured to the rear of the box.
- **FCS**: the [FCS II install guide](https://www.cfsnet.co.uk/content/downloads/FCSII_Installation_Instructions.pdf).

Reference boards: 6'0" for the shortboard setups and 9'0" for single and 2+1. All distances are in inches.

| Setup | Fin | From tail | From rail | Toe | Cant | Sources and choice |
|---|---|---|---|---|---|---|
| Thruster | sides | 11 | 1 1/4 | 1/4 | 6.5° | L: 11, 1 1/4, 1/4. FGH: 11–11.5, 1/4 toe. Greenlight (cited by L): 11–12 and 1 1/8–1 1/4. Cant: L says the FCS/Futures default is about 6.5–7°; FCS II rail plugs are labelled 9°. 6.5° is used. |
| | centre | 3 1/2 | — | — | 0° | L: 3.5. FGH: 3–3.5. |
| Quad | fronts | 11 | 1 1/4 | 1/4 | 6.5° | L; FGH 11–11.5. |
| | rears | 5 1/2 | 1 1/4 | 3/16 | 4° | **Sources disagree**: FGH 5, Timmy tool 5.5, L 6. 5.5 is used. Toe 3/16 (L; FGH 1/8). Cant: FCS II rear plugs are 3° or 5°, so 4° is used. |
| Five fins | all | — | — | — | — | Thruster fronts and centre plus the quad rears (L: fronts 11, rears 6, centre 3.5). The quad's 5.5 rears are used so that switching between quad and thruster uses the same boxes. |
| Twin | pair | 8 | 1 1/4 | 3/16 | 6.5° | **Sources disagree**: FGH 7–8 "depending on tail width"; L 10 for performance twins and 5–7 for wide-tail fish. The fish/twin value 8 is used. Cant is **not sourced** for twins; the thruster value is used. |
| Twin + trailer | pair | 10 1/2 | 1 1/4 | 3/16 | 6.5° | L. |
| | trailer | 4 | — | — | 0° | L. |
| 2+1 (9'0") | sides | 15 1/2 | 1 1/4 | 3/16 | 6.5° | L: 15.5, 1 1/4, 3/16 for 9'0"–9'4". FGH: 15–16.5, 1/4 toe. |
| | centre box rear | 5 1/2 | — | — | 0° | L: 5.5 (10.5" box). FGH: 5–6. Always a US box. |
| Single (9'0") | box rear | 5 | — | — | 0° | FGH: 5–6. L: 4.5 (10.5" box), about 6–7 for a raked pintail single, about 3 for a glass-on. 5 is used. |

### Scaling with board length

- **Shortboard setups** follow the McKee table quoted by L. Front fins are at 10 1/4" on a 4'9", 11" on a 6'0" and 12 1/4" on a 7'0", with the rail distance unchanged. SurfCraft multiplies every distance from the tail by front-fin distance / 11. The factor is interpolated linearly between those three lengths, extended past them with the end slopes, and limited to 0.6–1.6. The centre fin then comes out at 3.26" on a 4'9" and 3.9" on a 7'0"; the table gives about 3" and 4".
- **Single and 2+1** scale in proportion to length / 9'0". No source gives a rule; this is SurfCraft's choice.
- **Side fins** are always placed 1 1/4" from the outline at their own position.
- **Pintails**: L adds 3/16" to the quad rear distance. SurfCraft does not apply this.

### Fin depths

The fin depths in the presets are used only to fill BoardCAD's legacy fin depth fields and for drawing:
- 4.5" for thruster sides and centre.
- 4" for quad rears.
- 5" for twins.
- 9" for a longboard single.
- 7" for a 2+1 centre and 3 1/2" for its sides.

These depths are not placement rules and have no source.

## Not found

The following could not be confirmed from a reliable source:
- an official FCS I plug spacing or outer diameter (forum measurements are used);
- the FCS II plug depth and exact outline;
- the routing depth of the 1/2" Futures box;
- a Futures "V2" or thin box;
- the widths and depths of the 8" US box, the FCS longboard box and the Futures 1-Shot box;
- where the fin's trailing edge sits relative to the box or plugs, for any system (footprints are centred on the middle of the fin base);
- twin fin cant.

futuresfins.com and surffcs.com could not be read during the research (rate limited or 404). Their technical pages are
the place to check first.
