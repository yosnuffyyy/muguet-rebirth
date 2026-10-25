# Rebirth 0.46.7 — Graphics Settings Labels

## Change

The Graphics page uses a shared two-column setting row: a readable setting name on the left and its bound value/control on the right. Menu pickers explicitly render the selected value, including saved migrated values. Switches display On/Off. Rows wrap vertically and retain touch-sized controls. Graphics rows have extra bottom scroll space at accessibility text sizes.

All 15 graphics settings use the same row component. The existing settings model, saved keys, preset application, engine mappings, resolution/FPS values, runtime, game assets and save data are unchanged.

## Verification

- Host/settings and upstream test suites passed in the checked CI runs.
- Swift launcher simulator previews compiled and produced screenshots with visible labels and selected values (workflow run 40221721343).
- The compiler-free iOS ARM64 build and dependency checks passed.
- The end-to-end graphics UI suite did not pass within the latest run: its 900-second test command timed out. Earlier runs exposed the compact accessibility test reaching a partially clipped scroll position; its bounded scroll allowance was expanded and bottom scroll space added. These changes have not yet completed a passing full UI run.
- No physical iPhone UI verification was performed.

Because the required UI suite did not pass, the workflow correctly did not run its IPA packaging step. There is no validated IPA or IPA hash for this revision.

## Revision and next check

Native development branch commit: 14a18f909e6d99e0bb1a6b3e3588024787224113. Candidate app identity remains 0.46.7 / build 53.

After the UI suite passes, install the produced build over the existing app and inspect every Graphics row in landscape, including long values and larger text sizes. Confirm saved values, preset behavior and controls; simulator evidence does not establish physical-device behavior.

