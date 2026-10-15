---
id: lewisian:adventure-widows-lamp
title: "The Widow's Lamp"
type: adventure
lens: lewisian
license_provenance: homebrew
sources:
  - "original work — github.com/average-gary/pf2e-companion"
  - "wiki:pf2e-biblical-reskin/widows-lamp-scenario"
level_range: [1, 2]
scenes:
  - { id: scene-1, title: "The Lamp in the Window", read_aloud: "Rain hammers the cobblestones of Oakhaven's Dock Ward. Ahead, a narrow three-story tenement leans into the wind, its lowest window glowing with a steady, amber light — oil-fed, unhurried, defiant against the storm. The sign above the door swings on one chain: THE WIDOW'S LAMP. No chantry bell tolls, no patrol lanterns swing this far out. Just the light, and the sound of something heavy being dragged across floorboards upstairs." }
  - { id: scene-2, title: "The Debt Collector's Due", encounter: { difficulty: "moderate", npcs: ["debt-collector", "thug", "thug"] } }
  - { id: scene-3, title: "The Light Unveiled", read_aloud: "The rain eases. Through the broken shutters, dawn bleeds pale gold across the alley. The lamp in the window still burns — but now you see its wick is no ordinary fiber. It is a thread of pale gold, unburning, anchored in a brass font that bears the maker's mark of the Temple Smiths' Guild. The widow, Mara, sets a cup of tea before you with hands that tremble only slightly. 'They come for the light,' she says, 'because they cannot make their own. But the Light was never theirs to take — only to tend.'" }
handouts:
  - { id: letter, image: art/adventures/widows-lamp/handout-letter.webp, image_alt: "Storybook ink-and-wash still life: a folded vellum letter sealed with black wax, a plain coin pressed into the seal instead of a signet, an iron-grey quill beside it on a dark oak table.", title: "The Collector's Letter", visible_to: "gm", body: "A folded vellum sealed with black wax — a coin pressed into the seal, not a signet. The script is precise, legalistic:\n\n> *To the Agent of the Iron Ledger, Dock Ward, Oakhaven.*\n> *The asset designated 'Lamp of the First Watch' remains uncollected. The debtor, Mara of House Varel, has defaulted on the Covenant of Sustenance (Clause 7, Sub-clause 3). You are authorized to seize, bind, and convey the asset and any associated vessels to the Vault of Ledgers by the Feast of Chains. Resistance constitutes forfeiture of soul-title under the Old Compact.*\n>\n> *— Keeper Vorn, Ninth Quill of the Iron Ledger*\n\nThe 'Old Compact' and 'Vault of Ledgers' are not known in any legitimate temple archive. This is infernal bureaucracy masquerading as law." }
  - { id: journal, image: art/adventures/widows-lamp/handout-journal.webp, image_alt: "Storybook ink-and-wash still life: a slim water-stained notebook with warped pages on a rain-spotted windowsill, beside a small brass oil lamp burning with one steady pale-gold flame.", title: "Mara's Journal", visible_to: "all", body: "A slim, water-stained notebook. The earlier entries are ledger-precise: rents, tithes, the price of oil. The later entries grow strange.\n\n*14th of Rainsfall* — The lamp burned without oil for three nights. I dreamed of a city where every window held such a light, and no storm could reach them. A voice: 'Keep it. Tend it. The night is long but the Morning is certain.'\n\n*17th of Rainsfall* — Men in iron masks asked after 'the asset.' I said I knew nothing. They left a letter. The lamp flickered when I read it — not with fear, but with recognition.\n\n*20th of Rainsfall* — The adventurers came. The lamp burned brighter when they crossed the threshold. Perhaps the voice was right: the light calls to those who carry it." }
npcs: [lamplighter, debt-collector, widow-mara, thug]
image: art/adventures/widows-lamp/cover.webp
image_alt: "Storybook ink-and-wash plate: a rain-swept dockside street at dusk, crooked timber tenements leaning into the wind, and one narrow three-story tenement whose single ground-floor window glows with steady amber lamplight."
image_provenance:
  generator: ppq
  model: nano-banana-2-1
  style_block: styles/lewisian.md@1
  client_request_id: pf2e-lewisian-cover-widows-lamp-v1
  generated_at: 2026-10-14
  curated: true
  promoted_at: 2026-10-14
---

# The Widow's Lamp — GM Notes

**Level Range**: 1–2 (optimized for 4 PCs of level 1)
**Themes**: Providence in darkness, the cost of tending light, bureaucratic evil vs. living faith
**Tone**: Mere-Christianity posture — no denominational distinctives required. The "Light" is the Light of John 1:5; the "Old Compact" parodies infernal legalism (cf. 2 Cor 3:6). The adventure works whether the PCs are believers, seekers, or skeptics.

## Overview

A widow tends a lamp that burns without oil in a tenement at the edge of the world. Infernal debt collectors — lawyers of the Iron Ledger — come to seize it as a defaulted asset. The PCs intervene. The lamp is a minor relic of the First Watch (the angels who kept vigil before the Fall); its light reveals truth and disrupts infernal contracts.

## Running the Adventure

### Scene 1: The Lamp in the Window
**Social/Exploration**. The PCs arrive at the Widow's Lamp tenement. Mara (widow-mara) is terrified but hospitable. The debt-collector and thugs arrive at the end of the scene, demanding "the asset." Combat begins in Scene 2.

**Read-aloud**: Provided in frontmatter.

**Key NPCs**: Mara (widow-mara) — fragile but sharp. She knows the lamp is special but not *what* it is.
**Clues**:
- The lamp's flame casts no shadow from Mara's hands.
- The letter (handout: letter) is on the table if PCs search.
- Mara's journal (handout: journal) is in her coat pocket.

### Scene 2: The Debt Collector's Due
**Combat (Moderate)**. The debt-collector (elite) + 2 thugs (standard) burst in. They fight to subdue and seize the lamp, not to kill. The debt-collector uses *Legalistic Bind* (reaction) to restrain PCs who damage the lamp or Mara.

**Tactics**:
- Debt-collector targets the lamp-bearer.
- Thugs flank and grapple.
- If reduced to ≤15 HP, debt-collector attempts to flee with the lamp.

**Environment**: Tight tenement hallway (difficult terrain), oil lamps (fire hazard), rain-slicked stairs to upper floors.

### Scene 3: The Light Unveiled
**Resolution**. After combat, the lamp's true nature reveals itself. Mara offers the journal (handout: journal) and a reward: the lamp becomes a *minor relic* (see below) for the party.

**Read-aloud**: Provided in frontmatter.

**Relic — The Widow's Lamp** (minor, slotless, light Bulk)
- **Activate** (Interact, divine, light): The lamp sheds bright light in a 20-ft radius (dim to 40 ft) for 1 hour. Fiends and undead in the light take a –1 status penalty to attack rolls and saves. Infernal contracts and magical compulsions are suppressed in the light (counteract level 1, modifier +5).
- **Activate** (Envision, divine, revelation): Once per day, the bearer gains *true seeing* for 1 round.
- **Craft Requirements**: Initial creator must have the *light* cantrip and a sincere vow to tend light in darkness.

## NPCs

See statblock sidecars:
- `widows-lamp.lamplighter.statblock.json` — The mysterious caretaker of such lamps (ally/patron)
- `widows-lamp.debt-collector.statblock.json` — Infernal bureaucrat-enforcer (boss)
- `widows-lamp.widow-mara.statblock.json` — The widow herself (non-combatant, but with a secret)
- `widows-lamp.thug.statblock.json` — Debtor-enforcer (mook, appears ×2 in Scene 2)

## Scaling

**Weak Party (3 PCs or level 1)**: Reduce thugs to 1; debt-collector HP –10.
**Strong Party (5–6 PCs or level 2)**: Add 1 thug; debt-collector gains *Infernal Warrant* (1/day, dominate person, DC 18).
**High Level (3+)**: The lamp attracts a *Contract Devil* (level 5) as the true collector.

## Theological Notes (Lewisian Lens)

The adventure avoids sectarian markers. The "Light" is *phōs* (John 1:4–5); the "Old Compact" is the *letter that kills* (2 Cor 3:6); the "Morning" is the *Dayspring* (Luke 1:78). The Iron Ledger parodies the reduction of covenant to contract — a temptation in every tradition. The widow's vocation (tending a light she did not make, did not earn, but *keeps*) mirrors the call to *stewardship* over *ownership* (1 Cor 4:7).

No mechanical benefit requires profession of faith. The relic responds to *vow* — a deliberate commitment — not to creedal assent. This mirrors the Lewisian conviction that the hallway (mere Christianity) is where the doors to the rooms (traditions) open; the adventure plays in the hallway.