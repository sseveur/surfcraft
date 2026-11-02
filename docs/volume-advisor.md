# Volume advisor

Board > Volume advisor…, the "Suggest volume" section of File > New…, and the MCP tool `suggest_volume` suggest a
board volume range for a rider. The result is a **starting point**: shapers and surfers adjust it for the
rider's style, local waves and the board's shape (rail volume, foil, width and rocker all change how a given
volume paddles and floats).

## Method

Most online surfboard volume calculators use the **Guild factor**, published by John Whitney Guild (a former pro
surfer): volume in litres = rider weight in kg × a factor that falls with skill. The factors are for a performance
shortboard ridden in average waves.

SurfCraft uses these factors, consolidated from the published tables listed under Sources (they agree on the order
of magnitude and the trend but vary by a few hundredths):

| Skill | Factor (L/kg) | 75 kg rider |
|---|---|---|
| Beginner | 0.50 – 0.60 | 37.5 – 45 L |
| Intermediate | 0.40 – 0.45 | 30 – 33.8 L |
| Advanced | 0.36 – 0.40 | 27 – 30 L |
| Pro | 0.33 – 0.36 | 24.8 – 27 L |

Adjustments, applied to both ends of the range. The calculators below mention them but most publish no numbers, so
these values are SurfCraft's estimates within the ranges that are published:

| Input | Values |
|---|---|
| Fitness | low × 1.10, average × 1.00, high × 0.95 |
| Typical waves | small / weak × 1.08, average × 1.00, powerful / hollow × 0.93 (published: ± 5–10 %) |
| Age (optional) | under 35 × 1.00, 35–49 × 1.04, 50–59 × 1.08, 60+ × 1.12 (Surfer Today puts older surfers one factor band higher) |
| Board type | shortboard + 0 L, fish / hybrid + 4 L, mid-length / funboard + 10 L, longboard + 20 L (published: hybrid + 3–5 L, funboard + 8–12 L, longboard + 15–25 L) |

The code is `surfcraft_core::rider` (pure functions with unit tests). The middle of the range is what "Use … L" puts
into the New board dialog's target volume, and what "Set this board to … L" reaches by changing the thickness.

## Sources

- Surfer Today, "The surfboard volume calculator": Guild factor formula; beginner 0.50+, intermediate (and older
  surfers) 0.38–0.42, advanced/pro 0.34–0.36; formula best for PU shortboards and funboards.
  <https://www.surfertoday.com/surfing/the-surfboard-volume-calculator>
- Jungle Surf volume calculator: novice 0.50, casual 0.43–0.49, intermediate 0.42, advanced 0.36.
  <https://www.jungle-surf.com/pages/calculator>
- BreakFinder surfboard volume calculator: beginner 0.45–0.55, intermediate 0.38–0.45, advanced 0.32–0.38; small
  waves + 5–10 %, powerful waves − 5–10 %; hybrid + 3–5 L, funboard + 8–12 L, longboard + 15–25 L; fitness
  adjusts volume. <https://breakfinder.surf/tools/surfboard-volume-calculator>
- Ridershack surfboard volume guide: Guild-factor based ranges; adjust volume up for soft waves, age, crowded
  line-ups and low fitness. <https://ridershack.com/pages/surfboard-volume-guide>
