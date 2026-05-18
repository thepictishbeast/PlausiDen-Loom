# Asset licenses

Every asset shipped under `loom-assets/` carries its license in
its `AssetRegistry` entry. Possible classes ([`LicenseClass`]):

| Class           | Attribution | Share-alike | Commercial use |
|-----------------|-------------|-------------|----------------|
| `mit`           | yes         | no          | yes            |
| `apache-2`      | yes         | no          | yes            |
| `isc`           | yes         | no          | yes            |
| `cc0`           | no          | no          | yes            |
| `cc-by-4`       | yes         | no          | yes            |
| `cc-by-sa-4`    | yes         | yes         | yes            |
| `ofl-1-1`       | yes         | yes         | yes (rebrand)  |
| `unsplash`      | no (kind)   | no          | yes            |
| `pexels`        | no (kind)   | no          | yes            |
| `pixabay`       | no          | no          | yes            |
| `platform`      | per terms   | per terms   | per terms      |

## Where attribution renders

When `LicenseClass::requires_attribution()` returns `true`,
Forge's render pipeline emits a credits block in the page
footer linking back to the original source + author. The
attribution string is mechanically derived from each
`Asset::source` + `Asset::author` so manual maintenance isn't
needed.

## Audit

`forge audit-log verify` cross-checks every used asset
against the registry, refusing builds that reference an
asset with an unknown or untracked license.

## Adding new asset sources

1. Source must publish under one of the classes above.
2. Add a row to `SOURCES.md` describing where it came from
   + any provider-specific requirements (e.g. Pexels'
   "include credit when reasonably possible").
3. Open a PR adding the asset entries to `seeds/*.json`
   (one file per source).
4. Run `cargo test -p loom-assets`. The test suite checks
   slug uniqueness, license validity, and tag normalization.
