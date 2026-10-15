<script lang="ts">
  import { goto } from "$app/navigation";
</script>

<header class="hero">
  <button
    type="button"
    class="back"
    onclick={() => (history.length > 1 ? history.back() : goto("/"))}
  >
    ← Back
  </button>
  <h1>About this app</h1>
  <p class="tagline">
    What pf2e-companion is, where its content came from, and how to read
    the source citations.
  </p>
</header>

<article class="prose">
  <p>
    <strong>pf2e-companion</strong> is a cross-platform reference and
    worldbuilding companion for the Pathfinder Second Edition Remaster
    (2023–2024) with a Christian Biblical worldview reskin layered on
    top. It runs offline; your campaign vault lives on your filesystem
    as plain markdown.
  </p>

  <h2 id="content">Where the content comes from</h2>
  <p>
    Every entry in this app traces back to a research wiki: 78 sources
    ingested across two topic wikis (<code>pf2e-worldbuilding-tool</code>
    and <code>pf2e-biblical-reskin</code>) — Paizo's Player Core, GM Core
    and <em>Lost Omens: Divine Mysteries</em>; the Foundry-VTT pf2e
    schema; primary patristic and magisterial sources (Aquinas's <em>Summa</em>
    I.108 and II-II qq.92–96, the Westminster and Heidelberg
    confessions, the Catechism, the seven ecumenical councils);
    Tolkien's Letters 131 and 155 and Lewis's <em>De Descriptione Temporum</em>;
    1 Enoch's Book of the Watchers (R.H. Charles 1917) for the
    forbidden-arts catalogue.
  </p>
  <p>
    From that body, four parallel agents authored the four
    denominational lens packs — Catholic, Reformed, Pentecostal,
    Orthodox — using the per-lens decision article in the wiki as a
    brief. The Lewisian "mere Christianity" pack is the v1 default.
    Every entry has a <code>sources:</code> array citing the wiki
    article that grounds it plus a Bible reference or doctrinal
    primary source.
  </p>

  <h2 id="lenses">The five lenses</h2>
  <ul>
    <li>
      <strong>Lewisian</strong> (default) — aims at the consensus across
      historic Christian traditions; deferred denominational
      specifics; pairs with the Charism + Lewisian magic-theology hybrid.
    </li>
    <li>
      <strong>Catholic</strong> — 73-book canon with the deuterocanon;
      full nine-choir Pseudo-Dionysian angelic hierarchy; saint-attached
      Champion patrons; sacramental Cleric; Thaumaturge implements as
      relics; purgatory as a transitional plane.
    </li>
    <li>
      <strong>Reformed</strong> — 66-book canon; cessationist tilt;
      covenant theology scaffold; Word-as-sword imagery; Champion
      causes attach to abstract virtues, not patron saints.
    </li>
    <li>
      <strong>Pentecostal</strong> — strong continuationism; spiritual-
      warfare cosmology; the nine charisms of 1 Corinthians 12 mapped
      directly to PF2e Divine spells.
    </li>
    <li>
      <strong>Orthodox</strong> — Septuagint+ canon; the Synaxis of
      Seven archangels; theosis as a level-20 capstone; iconographer-
      hesychast Monk archetype; the Aerial Toll Houses as a level-15+
      post-mortem mega-dungeon.
    </li>
  </ul>

  <h2 id="citations">What the citation marks mean</h2>
  <p>Every entry's <em>Sources</em> sidebar uses six conventions:</p>
  <dl class="citation-key">
    <dt><span class="ico">📖</span> Bible reference</dt>
    <dd>
      Standard book + chapter/verse (e.g. <code>Mt 14:25</code>, <code
        >1 Cor 12:8-10</code
      >). Click to open the passage on BibleGateway in the NRSV-UE.
      1 Enoch refs link to the Sefaria translation.
    </dd>
    <dt><span class="ico">🔗</span> URL</dt>
    <dd>
      A bare <code>https://…</code> link opens the source in your default
      browser.
    </dd>
    <dt><span class="ico">⛪</span> Magisterial / confessional</dt>
    <dd>
      Catechism of the Catholic Church, Westminster Confession of Faith,
      Heidelberg Catechism, Council of Trent, Vatican documents, the
      Schleitheim Confession, and so on. Rendered as styled text — these
      are church documents that don't have a single authoritative
      hosted text we link to.
    </dd>
    <dt><span class="ico">✒️</span> Theologian / patristic / Inkling</dt>
    <dd>
      Aquinas, Augustine, Athanasius, Gregory of Nyssa, Palamas, John of
      Damascus, Calvin, Luther, Lewis, Tolkien. Same principle as
      magisterial citations.
    </dd>
    <dt><span class="ico">🗂</span> Design-wiki reference</dt>
    <dd>
      A pointer of the form <code>wiki:topic/article § section</code> into
      the local research wiki the content was authored from. The wiki
      itself isn't bundled with the app — these are bookkeeping
      footnotes for contributors.
    </dd>
    <dt><span class="ico">•</span> Plain</dt>
    <dd>
      Anything we couldn't classify (e.g. <em>The Way of a Pilgrim</em>,
      named scholarly works, traditional folk-citations). Visible but
      unstyled.
    </dd>
  </dl>

  <h2 id="license">License badges</h2>
  <p>
    Every entity carries a <code>license</code> badge that tells you
    where the content sits in Paizo's licensing stack and the
    associated redistribution rules:
  </p>
  <table class="license-table">
    <thead>
      <tr><th>Badge</th><th>What it means</th></tr>
    </thead>
    <tbody>
      <tr>
        <td><code>orc</code></td>
        <td>
          Open RPG Creative License (LoC TX 9-307-067). Mechanics and
          game systems — irrevocable, perpetual, royalty-free. Required
          attribution + downstream re-grant.
        </td>
      </tr>
      <tr>
        <td><code>community-use</code></td>
        <td>
          Paizo Community Use Policy. Covers Golarion-flavored content
          (proper nouns, deities, places). Free distribution only —
          no paywalls. This pack ships zero Community-Use content
          today; the badge is here for plugins.
        </td>
      </tr>
      <tr>
        <td><code>homebrew</code></td>
        <td>
          Authored for this app, derived from primary biblical and
          historical-Christian sources. Ours to ship; ours to relicense.
          The Lewisian / Catholic / Reformed / Pentecostal / Orthodox
          packs are all homebrew.
        </td>
      </tr>
      <tr>
        <td><code>proprietary</code></td>
        <td>
          A plugin or import has been flagged as restricted. The vault
          editor accepts the badge but it disables export by default.
        </td>
      </tr>
    </tbody>
  </table>

  <h2 id="art-provenance">Art provenance</h2>
  <p>
    Any imagery bundled with the packs is AI-generated, human-directed,
    and human-curated: prompts are versioned in the repo
    (<code>tools/imagery/</code>), every image passes a human checklist
    against its lens's doctrinal do-not list before promotion, and its
    frontmatter records prompt lineage plus alt text. Pure AI output
    carries no copyright; no Paizo (or other third-party) art is used.
    Missing art silently falls back to the text UI — images never gate
    content.
  </p>

  <h2 id="discipline">The design discipline</h2>
  <p>
    Forty years of Christian RPGs taught one durable lesson — what the
    research wiki calls <em>the DragonRaid trap</em>, after the 1984
    Adventures-for-Christ game in which players had to recite Bible
    verses to cast spells. Mechanics that require player piety as input
    alienate gamers (it feels coercive) and devout Christians (it feels
    like the sacred is being used as a game token).
  </p>
  <p>
    So: <strong>nothing in this app requires real-world prayer,
    recitation, or piety to function</strong>. The Jesus Prayer in the
    Orthodox Monk's Inner-Prayer Focus spell triggers on a spell action,
    not on the player saying it aloud. Cleric edicts and anathema are
    flavor and roleplay, not casting prerequisites. Theology is
    content, not a rules input.
  </p>
  <p>
    The same posture governs the optional LLM-assisted chat at
    <a href="/chat">/chat</a>. The PF2e community is generally fine
    with prose-drafting AI but wary of rules-touching AI, so the
    feature is <strong>off by default, BYO key</strong>, hidden behind
    <a href="/settings/llm">/settings/llm</a>, and never mentioned in
    the first-launch flow. Local models (Ollama) keep everything on
    your machine; cloud models (Anthropic) send prompts and the active
    lens's content to the provider. Either way, verify rules-touching
    outputs against canonical PF2e Remaster sources before bringing
    them to the table.
  </p>

  <h2 id="repo">Project</h2>
  <p>
    Open source at
    <a
      href="https://github.com/average-gary/pf2e-companion"
      target="_blank"
      rel="noopener noreferrer">github.com/average-gary/pf2e-companion</a
    >. The research wiki it was authored from lives at
    <code>~/wiki/topics/pf2e-biblical-reskin/</code> and
    <code>~/wiki/topics/pf2e-worldbuilding-tool/</code> on the
    maintainer's machine; PRs to the content packs welcome.
  </p>
</article>

<style>
  .hero {
    margin-bottom: 1rem;
  }
  .back {
    align-self: flex-start;
    font: inherit;
    background: transparent;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0.25rem 0;
    margin-bottom: 0.4rem;
    font-size: 0.85rem;
  }
  .back:hover {
    color: inherit;
  }
  h1 {
    margin: 0.1rem 0 0;
    font-size: 1.4rem;
    font-weight: 600;
  }
  .tagline {
    margin: 0.2rem 0 0;
    color: var(--muted);
    font-size: 0.85rem;
  }

  .prose {
    line-height: 1.55;
    font-size: 0.95rem;
  }
  .prose h2 {
    margin-top: 1.6rem;
    font-size: 1.05rem;
    border-bottom: 1px solid var(--line);
    padding-bottom: 0.2rem;
    scroll-margin-top: 4rem;
  }
  .prose p,
  .prose ul {
    margin-block: 0.7rem;
  }
  .prose code {
    background: var(--bg-soft);
    padding: 0.05rem 0.3rem;
    border-radius: 4px;
    font-size: 0.85em;
    font-family: "SF Mono", Menlo, monospace;
  }
  .prose a {
    color: inherit;
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, currentColor 30%, transparent);
    text-underline-offset: 0.15em;
  }
  .prose a:hover {
    text-decoration-color: currentColor;
  }

  dl.citation-key {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.4rem 1rem;
    margin: 0.6rem 0;
  }
  dl.citation-key dt {
    font-weight: 500;
    white-space: nowrap;
  }
  dl.citation-key dd {
    margin: 0;
    color: color-mix(in srgb, currentColor 85%, transparent);
  }
  dl.citation-key .ico {
    margin-right: 0.3rem;
  }

  table.license-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.88rem;
    margin: 0.6rem 0;
  }
  table.license-table th,
  table.license-table td {
    border-bottom: 1px solid var(--line);
    padding: 0.45rem 0.55rem;
    text-align: left;
    vertical-align: top;
  }
  table.license-table th {
    font-weight: 500;
    color: var(--muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
  table.license-table td:first-child {
    width: 9rem;
    white-space: nowrap;
  }

  @media (max-width: 540px) {
    dl.citation-key {
      grid-template-columns: 1fr;
    }
    dl.citation-key dt {
      margin-top: 0.4rem;
    }
  }
</style>
