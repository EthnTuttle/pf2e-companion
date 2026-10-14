---
id: lewisian:adventure-roof-of-mercy
title: "The Roof of Mercy"
type: adventure
lens: lewisian
license_provenance: homebrew
sources:
  - "original work — github.com/average-gary/pf2e-companion"
  - "wiki:pf2e-biblical-reskin/roof-of-mercy-scenario"
level_range: [3, 4]
scenes:
  - { id: scene-1, title: "The House of Hearing", read_aloud: "The narrow streets of Kestrel's Roost open onto a courtyard where a modest two-story house stands, its shutters thrown wide despite the autumn chill. Voices spill from within — a teacher's measured cadence, the murmurs of a crowd pressed shoulder-to-shoulder on the lower floor, spilling onto the stair landing and into the yard. The air hums with attention, the kind that gathers when something true is being spoken. A stretcher bears a still figure at the crowd's edge: a paralyzed man, his eyes bright with a hope that has outlasted years of closed doors." }
  - { id: scene-2, title: "The Roof Gives Way", encounter: { difficulty: "moderate", npcs: ["temple-scribe", "temple-scribe"] } }
  - { id: scene-3, title: "Take Up Your Mat", read_aloud: "Dust settles on the crowded floor. The teacher looks up from the stretcher now resting at his feet, meets the paralyzed man's eyes, and speaks the words that shatter every expectation: 'Child, your sins are forgiven.' A sharp intake of breath from the scribes in the front rank. The teacher does not flinch. 'Which is easier: to say your sins are forgiven, or to say rise and walk? But that you may know the Son of Man has authority on earth to forgive sins—' He turns to the man. 'Rise, take up your mat, and go home.' Muscles that have not moved in years obey. The man stands, rolls his mat, and walks through the parted crowd into the morning light." }
handouts:
  - { id: paralytics-prayer, title: "The Paralytic's Prayer", visible_to: "all", body: "A crumpled vellum, smoothed by years of folding and unfolding. The ink has faded to sepia, but the hand remains steady:\n\n> *Lord of the Open Door,*\n> *You who made the roof as well as the room,*\n> *You who hear the knock of those who cannot reach the threshold —* \n> *Teach us to carry one another.*\n> *Teach us to tear open the ceiling when the door is barred.*\n> *Teach us that forgiveness is not a word spoken from safety,*\n> *But a roof removed, a mat rolled up, a life returned to walking.*\n> *Amen.*\n\nThe margins bear smaller notations in a different hand — answers, perhaps, or additions from those who have prayed this prayer and found it answered." }
npcs: [wandering-teacher, paralytic-elian, temple-scribe]
---

# The Roof of Mercy — GM Notes

**Level Range**: 3–4 (optimized for 4 PCs of level 3)
**Themes**: Forgiveness given and practiced, community bearing one another's burdens, authority that heals rather than controls
**Tone**: Mere-Christianity posture — the adventure draws on Mark 2:1–12 (the paralytic lowered through the roof) as narrative architecture. The "Teacher" embodies mercy that precedes merit; the "scribes" embody the temptation to gatekeep grace. No denominational distinctives required. The adventure works whether PCs are believers, seekers, or skeptics.

## Overview

A wandering teacher draws crowds in Kestrel's Roost with words that heal as much as they teach. A paralyzed man named Elian has been carried by his four friends for days to reach this teacher — but the house is full. The friends climb onto the roof, tear it open, and lower Elian down. Before the teacher heals Elian's body, he forgives his sins. Temple scribes present object to the teacher's authority to forgive. The PCs, drawn into the scene as witnesses or participants, must navigate the confrontation and its aftermath.

## Running the Adventure

### Scene 1: The House of Hearing
**Social/Exploration**. The PCs arrive at the crowded house. The teacher (wandering-teacher) is mid-teaching. Elian (paralytic-elian) and his friends are at the edge of the crowd, unable to enter. The temple scribes (temple-scribe) sit in places of honor near the front, watching with folded arms.

**Read-aloud**: Provided in frontmatter.

**Key NPCs**:
- Wandering Teacher — calm, authoritative without demanding authority, speaks in parables and direct truths.
- Elian — paralyzed from the waist down, eyes alert, has traveled three days on a stretcher.
- Temple Scribes — officious, concerned with proper procedure and authority, not malice but rigidity.

**Clues**:
- The teacher's words seem to settle physical ailments in the listeners (subtle, deniable).
- The scribes carry scrolls marked with the seal of the local temple hierarchy.
- Elian's friends are visibly exhausted — they have carried him far.
- The roof is accessible via an external stair and has clay tiles over timber beams.

### Scene 2: The Roof Gives Way
**Combat (Moderate) / Skill Challenge Hybrid**. Elian's friends begin tearing up the roof tiles. Two temple scribes intervene to enforce procedural order. The PCs can support the friends (skill checks: Athletics to lift tiles, Acrobatics to maneuver on the roof, Diplomacy/Intimidation to stall the scribes) or engage the opponents directly.

**Opponents**: 2 Temple Scribes (standard). They fight to subdue and remove the intruders, not to kill. Each scribe uses *Procedural Objection* (reaction) to hamper skill checks and *Doctrinal Censure* to suppress unsanctioned magic.

**Tactics**:
- Scribes position on the roof edge to block access to the opening.
- Scribes target the friends lowering Elian with *Procedural Objection*.
- Scribes use *Doctrinal Censure* against any divine magic not bearing the temple's seal.
- If reduced to ≤15 HP, a scribe retreats into the house to "report the irregularity."
- Scribes prioritize maintaining order over causing harm.

**Environment**: Sloped clay-tile roof (difficult terrain, DC 15 Acrobatics to move without slipping), 15-ft drop to lower floor, crowd below (soft cover, civilians).

**Skill Challenge Alternative**: DCs 17/19 (level 3/4). 3 successes before 2 failures opens the roof. Each failed check brings the scribes closer. PCs can aid the friends directly.

### Scene 3: Take Up Your Mat
**Resolution**. The roof opens. Elian is lowered before the teacher. The teacher forgives Elian's sins first. The scribes object: "Who can forgive sins but God alone?" The teacher demonstrates authority by healing Elian's paralysis. The crowd erupts in wonder. The scribes withdraw, murmuring.

**Read-aloud**: Provided in frontmatter.

**Aftermath**:
- Elian walks out carrying his mat. His friends weep.
- The teacher invites the PCs to walk with him — the road continues.
- The scribes' scrolls contain a formal censure notice (handout: paralytics-prayer is Elian's response).
- The house becomes a landmark: "The Roof of Mercy."

## NPCs

See statblock sidecars:
- `roof-of-mercy.wandering-teacher.statblock.json` — The teacher whose words forgive and heal (ally/patron, level 5)
- `roof-of-mercy.paralytic-elian.statblock.json` — The paralyzed man carried to the roof (non-combatant, level 3)
- `roof-of-mercy.temple-scribe.statblock.json` — Temple bureaucrat enforcing doctrinal boundaries (antagonist, level 3, appears ×2 in Scene 2)

## Scaling

**Weak Party (3 PCs or level 3)**: Reduce temple scribes to 1.
**Strong Party (5–6 PCs or level 4)**: Add 1 temple scribe (total 3); one scribe gains *Divine Censure* (1/day, banishment, DC 22).
**High Level (5+)**: The teacher is revealed as an avatar of Mercy itself; the scribes are replaced by a *Doctrinal Inquisitor* (level 6) with true seeing and dispel magic.

## Theological Notes (Lewisian Lens)

The adventure follows Mark 2:1–12 as narrative scaffold, not proof-text. The "Teacher" is a Christ-figure in the Lewisian sense — *Aslan is not a tame lion, but he is good*. The forgiveness-first-then-healing sequence mirrors the text's theology: the greater miracle is the forgiveness; the healing makes the invisible visible (Mark 2:10–11).

The "roof" is the barrier every tradition erects — procedure, purity, proper channels. The friends who tear it open are the *community that carries* (Gal 6:2). The scribes are not villains; they are *guardians of the door* who have forgotten the door exists for those outside it.

The paralytic's prayer (handout) is the voice of the one carried — not a demand, but a petition that the roof itself can be grace.

No mechanical benefit requires profession of faith. The adventure rewards *carrying one another* (aid actions, skill cooperation) and *choosing mercy over procedure* (diplomacy over combat, protecting the vulnerable). This mirrors the Lewisian conviction that the hallway (mere Christianity) is where the doors to the rooms (traditions) open; the adventure plays in the hallway.

**Sources**:
- Mark 2:1–12 (primary narrative)
- Lewis, C.S. *Mere Christianity* (hallway/doors metaphor)
- Augustine, *On the Spirit and the Letter* (law vs. grace)
- WCF 11 / Heidelberg Catechism Q60 (forgiveness as declaration, not process)