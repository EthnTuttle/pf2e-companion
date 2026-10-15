# UI Review Protocol for pf2e-companion

This document describes how to run the UI screenshot harness and review the generated screenshots for visual regressions, layout issues, and missing elements.

## Steps

1. **Generate screenshots**  
   Run the following command to generate 80 PNG screenshots (desktop/mobile, light/dark themes) for all routes:
   ```bash
   pnpm e2e:shots
   ```
   This will:
   - Build the app (if `build/` is stale)
   - Start a static preview server on `http://localhost:4173`
   - Launch Chromium with the Tauri stub and fixture maps
   - Capture full-page screenshots for each route/viewport/theme combination
   - Write `shots/manifest.json` and organize PNGs in `shots/<route-slug>/`

   On first run, you may need to install Chromium:
   ```bash
   pnpm exec playwright install chromium
   ```
   The `shots/` directory is gitignored.

2. **Inspect the manifest**  
   Read `shots/manifest.json` to understand the generated set. It contains:
   - `generatedBy`, `node`, `playwright` versions
   - An array of objects, each with:
     - `route` (e.g., `/`, `/settings`)
     - `variant` (e.g., `default`, `wizardFresh`, `chatConfigured`)
     - `viewport` (`desktop` or `mobile`)
     - `theme` (`light` or `dark`)
     - `file` (relative path to PNG)
     - `setup` (any special setup notes)
     - `readySelector` (the DOM element waited for)
     - `notes` (e.g., `"stubbed-backend: llm:token replay"` or `"fixture-snapshot"`)

3. **Review each screenshot**  
   For each entry in the manifest:
   - Use the `Read` tool to view the PNG at `shots/<file>`.
   - Evaluate against the invariant checklist below.
   - Note any defects (severity: low/medium/high) and suggested fixes.

4. **Write findings**  
   For each route with defects, create a file:
   ```
   docs/ui-reviews/YYYY-MM-DD-<route-slug>.md
   ```
   where `<route-slug>` is the route with leading slash replaced and internal slashes replaced by dashes (e.g., `/settings/llm` → `settings-llm`).  
   If multiple defect files are needed for the same route on the same day, append `-<n>` (e.g., `2026-10-14-settings-llm-2.md`).

   Each findings file should follow this template:

   ```markdown
   # UI Review Findings: <route> (<viewport>, <theme>)
   **Date:** YYYY-MM-DD
   **Screenshot:** `shots/<file>`

   ## Defects

   ### <Severity>: <Short description>
   - **Location:** (e.g., "bottom tabbar", "entity editor title field")
   - **Observation:** (what you see wrong)
   - **Expected:** (what should be seen)
   - **Suggested fix:** (optional)

   ## Checklist Status
   - [ ] Text overflow/clipping: OK
   - [ ] Empty states: handled appropriately
   - [ ] Bottom-tab reachability on mobile: all tabs/pills accessible
   - [ ] Contrast in both themes: meets WCAG AA
   - [ ] Icons/images: none broken or missing
   - [ ] Unlabeled states: all interactive elements have accessible names
   - [ ] GM-only elements: hidden for player variant, visible for GM
   - [ ] Wizard screens: correct step header, focused input, reachable buttons
   - [ ] Chat-reply variant: assistant message complete, tool traces visible
   - [ ] Other: ________________________________________________
   ```

## Invariant Checklist

Check each screenshot for the following:

- **Text overflow/clipping**: No text is cut off or overflows its container.
- **Empty states**: Lists, charts, etc. show helpful placeholders when empty.
- **Bottom-tab reachability on mobile (390px width)**: All 6 tabbar tabs plus About/Chat/Settings pills are tappable without horizontal scroll; nothing is obscured by the fixed tabbar.
- **Contrast in both themes**: Text and UI elements meet WCAG AA contrast ratios (use `color-mix` tokens as defined in the theme).
- **Icons/images**: No missing broken-image icons; emoji render as expected.
- **Unlabeled states**: Every button, input, and interactive control has an accessible label (aria-label, placeholder, or associated label element).
- **GM-only elements**: In `/table/play` GM variant, GM-exclusive tabs (Encounter, Initiative, etc.) are visible; in player variant, they are hidden.
- **Wizard screens**: On `/onboarding`, the step header matches the current step (e.g., "Step 1 of 3 · Vault"), the relevant input is focused, and Back/Next buttons are reachable on mobile.
- **Chat-reply variant**: The assistant message includes the full reply and any tool trace sections; no orphaned empty assistant bubbles.
- **Other**: Any additional observations (e.g., animation frame, unexpected whitespace).

## Honestly Stubbed Surfaces

The screenshots do **not** prove the following; they are stubbed or require manual verification:

- **Native OS dialogs** (`plugin:dialog|save`/`open`): The stub returns a fixed path or `null`; real dialog behavior is not tested.
- **Live P2P sync / LAN QR/ticket flow**: Multi-client presence, reconnect banners, and join flows are not represented; the play surface shows a fixture snapshot only.
- **Pinch/zoom and touch token-drag gestures**: Static PNGs cannot prove gesture handling; manual testing is required.
- **Real WebKit/WebView2 rendering**: Shots use Chromium; cross-engine drift is accepted (see plan §5).
- **Real LLM backend behavior**: The chat-reply variant replays fixed token events through the real `llm:token` listen path, but the LLM backend is canned (labeled `stubbed-backend`).
- **Wizard restart card + vault-move semantics**: The `wizard_complete` fixture stubs `restart_required: false`; no actual restart occurs.
- **Fixed bottom tabbar overlapping mid-image content on tall routes**: `fullPage: true` captures paint fixed elements at their scroll-0 viewport coordinates, so on routes taller than one viewport the tabbar appears mid-image over live content even though real viewports clear it (main's 5rem bottom padding). Verify overlap claims at a real viewport before filing — see `2026-10-15-f1-fix.md` § harness note for measurements.

## Reviewer Subagent Prompt Template

You are a UI reviewer for pf2e-companion. Run `pnpm e2e:shots`, read `shots/manifest.json`, Read every PNG listed, evaluate each against the invariant checklist in this document, then write one findings file per route with defects ranked by severity. Do not modify app code.

Example invocation:
```
pnpm e2e:shots
# Then for each PNG, use Read tool to inspect and note defects.
# Finally, write findings to docs/ui-reviews/YYYY-MM-DD-<route>.md
```

---
*This protocol is part of the Tier 1 UI screenshot harness. See `docs/plans/PLAN-TIER1-ui-shots.md` for full context.*