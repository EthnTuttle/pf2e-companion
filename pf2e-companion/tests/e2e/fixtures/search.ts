// Fixtures for the search + entity-detail surfaces (PLAN §3.3, §4 S01–S02).
// Typed against $lib/ipc so a seam drift breaks `pnpm check`.
import type { EntityDetail, SearchHit } from "$lib/ipc";

/** S01: three hits, one per type chip family, all FTS-sourced. */
export const fixtureSearchHits: SearchHit[] = [
  {
    id: "lewisian:deity-hosts",
    title: "YHWH of Hosts",
    type: "deity",
    snippet: "The God of Abraham, Isaac, and Jacob — commander of the <em>hosts</em> of heaven.",
    score: 0.912,
    source: "fts",
  },
  {
    id: "lewisian:spell-force-barrage",
    title: "Force Barrage",
    type: "spell",
    snippet: "Loose darts of <em>force</em> that unerringly strike your foes.",
    score: 0.804,
    source: "fts",
  },
  {
    id: "lewisian:creature-cherub",
    title: "Cherub Sentinel",
    type: "creature",
    snippet: "A many-winged <em>guardian</em> of sacred thresholds.",
    score: 0.766,
    source: "fts",
  },
];

/** S02 hit side: `/entity/lewisian%3Adeity-hosts` renders a full deity page. */
export const fixtureEntityWithStatblock: EntityDetail = {
  id: "lewisian:deity-hosts",
  title: "YHWH of Hosts",
  type: "deity",
  lens: "lewisian",
  license_provenance: "homebrew",
  source: "reference",
  frontmatter: {
    sources: [
      "Exodus 3:14",
      "wiki:pf2e-biblical-reskin/divine-statblocks § YHWH",
    ],
  },
  body: [
    "## The Name",
    "",
    "I AM WHO I AM. The covenant God of Israel, enthroned above the cherubim,",
    "commander of the hosts of heaven.",
    "",
    "Clerics of the Name often learn [[lewisian:spell-force-barrage]] early.",
  ].join("\n"),
  statblock: {
    name: "YHWH of Hosts",
    title: "The God of Abraham, Isaac, and Jacob",
    edicts: ["Worship no other gods", "Keep the Sabbath"],
    anathema: ["Idolatry", "Take the Name in vain"],
    areas_of_concern: ["covenant", "creation", "justice"],
    divine_attribute: ["wisdom", "charisma"],
    divine_font: "heal",
    sanctification: "holy",
    divine_skill: "Religion",
    favored_weapon: "longsword",
    domains: ["creation", "fire", "glory", "healing"],
  },
};

/** S02 miss side: `/entity/missing%3Aid` renders the not-found state. */
export const fixtureMissingEntityId = "missing:id";
