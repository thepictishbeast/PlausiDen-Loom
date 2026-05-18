# Asset sources

A complete catalogue of upstream sources Forge accepts assets
from. Each source is vetted for license compatibility + signal-
to-noise + governance stability before we add it.

## Icons (single-color SVG)

| Source       | License | URL                                | Notes                                       |
|--------------|---------|------------------------------------|---------------------------------------------|
| Feather      | MIT     | github.com/feathericons/feather    | ~287 minimal line icons                     |
| Heroicons    | MIT     | github.com/tailwindlabs/heroicons  | 24x24 + 20x20 + 16x16 in line/solid         |
| Lucide       | ISC     | github.com/lucide-icons/lucide     | Feather fork, ~1500 icons + active dev      |
| Tabler       | MIT     | github.com/tabler/tabler-icons     | ~4000 icons, line + solid                   |
| Phosphor     | MIT     | github.com/phosphor-icons/core     | 6 weight variants per icon                  |
| Bootstrap    | MIT     | github.com/twbs/icons              | 2000+ icons                                 |

## Emoji (color SVG)

| Source       | License        | URL                              | Notes                                       |
|--------------|----------------|----------------------------------|---------------------------------------------|
| OpenMoji     | CC-BY-SA 4.0   | github.com/hfg-gmuend/openmoji   | 4000+ open-license emoji                    |
| Twemoji      | CC-BY 4.0      | github.com/twitter/twemoji       | Twitter's emoji set; license requires attribution |
| Noto Emoji   | Apache-2 / OFL | github.com/googlefonts/noto-emoji | Google's emoji set                         |

## Photos (raster)

| Source       | License             | URL              | Notes                                  |
|--------------|---------------------|------------------|----------------------------------------|
| Unsplash     | Unsplash License    | unsplash.com     | Free for any use; credit appreciated   |
| Pexels       | Pexels License      | pexels.com       | Free for any use; credit appreciated   |
| Pixabay      | Pixabay License     | pixabay.com      | Free for any use; no credit needed     |
| Wikimedia    | varies (CC0 / PD)   | commons.wikimedia.org | Filter by Public Domain / CC0     |

## Illustrations (vector)

| Source       | License        | URL              | Notes                                 |
|--------------|----------------|------------------|---------------------------------------|
| unDraw       | unDraw License | undraw.co        | Free for any use, no attribution      |
| Open Peeps   | CC0            | openpeeps.com    | CC0 hand-drawn portrait library       |
| Manypixels   | CC0            | manypixels.co    | CC0 illustrations                     |
| Humaaans     | CC BY 4.0      | humaaans.com     | Mix-and-match people                  |

## GIFs / loops

We intentionally don't ship Giphy / Tenor — their content
mostly lacks clear redistribution license. Instead:

| Source           | License | URL                          | Notes                       |
|------------------|---------|------------------------------|-----------------------------|
| CC0 Loops        | CC0     | (curated)                    | Hand-curated CC0 loops      |
| Platform-generated | platform | (generated)                | SVG keyframe loops          |

## Templates (CMS JSON)

All platform-authored — PlausiDen's reference templates for
common archetypes. License class: `platform`.

## Adding a source

To add a new source:

1. Verify the source publishes under a `LicenseClass` already in
   the enum (`loom_assets::LicenseClass`).
2. If not, extend the enum — the closed-enum design refuses
   builds that pull from un-vetted license classes.
3. Add a row to this table.
4. Open a PR with the corresponding `loom-assets/seeds/*.json`
   entries.

## Provenance audit

Every shipped asset's source URL + license appears in the per-
build SBOM (CycloneDX 1.5 via the supply-chain CI workflow).
Independently verifiable.
