---
id: lewisian:adventure-widows-lamp
title: "The Widow's Lamp"
type: adventure
lens: lewisian
license_provenance: homebrew
sources:
  - "original work — github.com/average-gary/pf2e-companion"
level_range: [1, 2]
scenes:
  - id: scene-1
    title: "The Lamplit Street"
    read_aloud: "Rain slicks the cobbles of Anchor Row. A single lamp still burns in the widow's window, though she was buried three days ago."
  - id: scene-2
    title: "The Debt Collector"
    encounter:
      difficulty: moderate
      npcs: [debt-collector]
  - id: scene-3
    title: "The Widow's Confession"
    read_aloud: "Mara folds her hands around the lamp as though it were a child."
handouts:
  - id: letter
    title: "A Collector's Letter"
    visible_to: all
    body: "The debt is owed. The lamp is collateral. Bring both."
  - id: ledger
    title: "The Household Ledger"
    visible_to: gm
    body: "Every entry in Mara's hand — except the last, which is not."
npcs: [lamplighter, debt-collector, widow-mara]
---

# The Widow's Lamp

## Run notes

A one-session mystery for levels 1–2. The lamp is a reliquary; the debt
is older than the marriage. If the party speaks with the dead, the widow
answers only in quotations from her ledger.
