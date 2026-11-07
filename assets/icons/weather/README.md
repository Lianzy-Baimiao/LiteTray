# LiteTray weather icons

These are original, locally authored SVG paths bundled with LiteTray. They are
not images downloaded from Open-Meteo, MET Norway, or another weather provider.
No icon font, bitmap download, external icon package, or browser is required.

All paths use a 24×24 viewBox, rounded joins/caps, a 1.8-unit stroke and a single
M/L/C/Z path, matching the existing native SVG path renderer. The weather renderer
sets a 1.75-physical-pixel minimum stroke for small hourly icons, samples coverage
at 8× the final physical size, and composites premultiplied alpha at 1:1. Cache
keys include weather symbol, physical size and theme color.

Symbols: sun, partly cloudy, cloud, rain, thunderstorm, snow, sleet, fog, hail,
unknown. Descriptions select these symbols locally; combined conditions take
priority over their individual words. Thunder with hail uses the thunderstorm
symbol. Unknown conditions retain an explicit question mark.
