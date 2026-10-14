# UI review — 2026-10-14 first pass (12-shot curated subset)

Method: 12/80 shots from `shots/manifest.json` reviewed by the orchestrator directly
(subagent vision unavailable — provider 500s across all tff model groups at review
time). Shots: root (d/m/l/dk), settings, onboarding s1+s2, table-play-gm (desktop-light,
mobile-dark), chat-reply, encounter, entity-aboleth (dark), vault-aboleth (light).
Known-stubbed surfaces excluded per harness notes (chat-reply tokens, table-play
snapshot data, onboarding dialog).

## Major

### F1 — Fixed bottom tab bar overlaps scroll content AND the table-play composer
Evidence: `encounter/desktop-light` (creature-XP rows +3/+4 unreadable behind the bar),
`vault-aboleth/desktop-light` (frontmatter JSON textarea slides under the bar),
`table-play-gm/desktop-light` (composer sits below the bar), and worst of all
`table-play-gm/mobile-dark` (composer input half-hidden BEHIND the tab icons —
the tab bar renders on top of the message input at 390px).
Why it matters: the one control players use most during a session is the message
input; obscured/tappable-through-tabs on phones is a table-blocking papercut.
Direction: composer pins above the tab bar (its fixed offset must account for tabbar
height); main scroll container needs `padding-bottom >= tabbar height + composer
height` on play, `>= tabbar height` elsewhere. Verify the plan's noted
"`main` padding-bottom 5rem" is actually effective at every viewport.

## Minor

### F2 — Desktop shows duplicate navigation (topbar pills + bottom tabs)
Evidence: all desktop shots show both. README calls bottom tabs "mobile".
Either intentional (document in /about) or gate tabs to <768px.

### F3 — In-card links have no affordance
Evidence: search hit titles (`root/*`) and settings index links (`settings/*`) render
as plain-black text, no underline/color — indistinguishable from static text,
while the onboarding "Skip wizard" link IS underlined (inconsistent).
Direction: underline or accent-color real links everywhere; audit `:focus-visible`.

### F4 — Mobile topbar wraps to ~230px tall
Evidence: `onboarding-step*/mobile-*`: brand "pf2e-companion" wraps mid-word,
Campaign/Lens selects take two more rows.
Direction: hide brand text <480px; consider collapsing the two selects into one row.

### F5 — /table/play mobile header piles up
Evidence: `table-play-gm/mobile-dark`: title wraps 2 lines, GM badge + peer id
float oddly, adventure pill ("widows-lamp") squeezes into a rounded blob.
Direction: compact header row <480px (title truncated, meta into one chip row).

## Nits

- **F6** `chat-reply/*`: "Using ollama · qwen3:4b . Change" — stray space before period.
- **F7** `vault-aboleth/*`: label case inconsistency ("BODY", "EXTRA FRONTMATTER (JSON)" vs
  "Title", "Type", "Campaign").
- **F8** `onboarding-step2/*`: Lewisian description reads "Charism+Lewisian
  magic-theology hybrid" — confirm "Charism+" is intended copy (likely "Charismatic").
- **F9** Fonts/emoji are host fallbacks (no bundled webfont) — deterministic per
  machine, not across OSes; acceptable for beta.

## Harness/fixture notes (NOT app bugs)

- `entity-aboleth/*`: `[[c-runes:location/...]]` wikilinks render raw — target entities
  aren't in the fixture corpus. Before filing an app bug, check the wikilink rewrite
  rule on missing targets; likely expected degrade.
- Composer/tabbar overlap (F1) shows in static shots exactly as a user sees it —
  not a capture artifact (the ready selector waits past hydration).
