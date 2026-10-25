# Rebirth 0.46.7 — Graphics Settings Labels

## Change

The Graphics page uses a shared two-column setting row: a readable setting name on the left and its bound value/control on the right. Menu pickers explicitly render the selected value, including saved migrated values. Switches display On/Off. Rows wrap vertically and retain touch-sized controls. Graphics rows have extra bottom scroll space at accessibility text sizes.

All 15 graphics settings use the same row component. The existing settings model, saved keys, preset application, engine mappings, resolution/FPS values, runtime, game assets and save data are unchanged.

## Build and delivery

- Rebirth version/build: **0.46.7 / 53**.
- Source revision embedded in the IPA: `12dedc0084cff19e80408c0d00e463e1d3d74949`.
- Local IPA: `outputs/GTA-Native-Prototype/GTA-Rebirth-0.46.7-12dedc0084cf-unsigned.ipa`.
- IPA size: 63,244,063 bytes.
- SHA-256: `5be7e8e43afbfe763c1611c91788590913f5342f189baa16b2b81a8b417b2003`.
- Package-only CI run 40224770057 succeeded. The IPA's ad-hoc signature and package metadata were checked by the packaging script; it still requires signing for installation.
- The encrypted IPA is also available as GitHub Actions artifact `rebirth-private-ipa` (artifact 12610842463; 14-day retention).

## Verification limits

Per request, the package-only run skipped tests. Earlier simulator runs showed labels and values in launcher previews, but the full graphics UI suite did not pass: it reached scroll-position failures and ultimately timed out. Therefore, this IPA is built and packaged, but the label fix is **not fully UI-test-validated**. No physical iPhone verification was performed.

Install on the iPhone and inspect every Graphics row in landscape, including long values and larger text sizes. Check that labels and values remain visible while scrolling and that settings still save and apply as expected.

