# nostr Integration Analysis — pf2e-companion

> Research date: 2026-10-14.
> Scope: evaluate nostr (the decentralized event/relay protocol) as an integration
> target for (1) session rendezvous, (2) player/GM identity, (3) content-pack
> distribution, (4) monetization via zaps, and (5) community/social features —
> weighed against this project's stated values (serverless-by-default, local-first,
> zero tracking) and the `docs/SHIPPING.md` tier plan.
> All NIP numbers, kind numbers, crate versions, and policy claims below were
> verified against primary sources in October 2026; see [Sources](#sources).

## Executive summary

**VERDICT: Reject nostr as a core dependency; defer a single narrow subset (signed
content-pack announcements) to Tier 3 as an *optional* transport, adopted only if
the pack ecosystem (SHIPPING.md item 16) actually materializes and playtests demand
it. Do not build session rendezvous, identity, payments, or social features on
nostr.**

Reasoning in brief:

- **The connectivity problem nostr would solve is already solved.** iroh's N0
  relay + pkarr/DNS discovery gives the app dial-by-NodeId rendezvous today, with
  QUIC encryption and a `PF2E_NET_WAN`-tested path. A nostr relay layer would add
  a *second* server dependency, a second crypto stack (secp256k1 alongside
  iroh's ed25519), and a second MSRV constraint, in exchange for a "searchable
  lobby" feature nobody has asked for and that Tier 1 item 2 (deep links) already
  covers more cheaply.
- **The identity story is a values regression.** SHIPPING.md's "the vault is the
  account" and the Tier-2 item-10 ed25519 per-peer identity + capability tokens
  are simpler, offline-capable, and curve-consistent with iroh. nostr keypairs
  (secp256k1, NIP-01) buy *portable* identity — but portability is exactly what
  "no account system" defers.
- **Zaps monetization is a poor fit on three independent axes**: audience
  (Christian-family stewardship optics around Bitcoin/Lightning), app-store
  policy (Apple forced Damus to strip in-app zaps under Guideline 3.1.1 in 2023),
  and scope (SHIPPING.md puts "marketplace payments" explicitly out of scope).
- **The one genuinely attractive idea** — signed, relay-distributable pack
  announcements as a no-server-to-run complement to the planned "pack format v2
  with semver + signatures" — is real but small: relay event-size caps (~128 KB
  default) mean packs themselves must be hosted elsewhere anyway (Blossom/NIP-96,
  Git, or a plain web server), reducing nostr to a signed announcement/index
  feed. That is a nice-to-have channel, not an architecture.

## 1. Background: what nostr is

nostr ("Notes and Other Stuff Transmitted by Relays") is a decentralized
publish/subscribe protocol ([NIP-01](https://nips.nostr.com/1)):

- Clients hold a **secp256k1 keypair**; the public key *is* the identity.
- Everything is a signed **event** (`{id, pubkey, created_at, kind, tags,
  content, sig}`) published to one or more **relays** — dumb WebSocket servers
  that store and filter events.
- **Kinds** partition the space; as of the current NIP-01, regular kinds are
  0–9999, **replaceable** kinds 10000–19999, **ephemeral** kinds 20000–29999
  (relays may drop them, not store), and **addressable** kinds 30000–39999 (one
  event retained per `(pubkey, kind, d-tag)` — formerly NIP-33, "parameterized
  replaceable", now folded into NIP-01).
- The relay list is user-chosen; anyone can run a relay
  ([nostr-rs-relay](https://github.com/scsibug/nostr-rs-relay) is the common Rust
  one). No global state, no built-in moderation.

Relevant NIPs for this analysis (all verified):

| NIP | Title | Why it matters here |
|---|---|---|
| [NIP-01](https://nips.nostr.com/1) | Basic protocol; event/kind model | everything |
| [NIP-05](https://nips.nostr.com/5) | DNS-verified identifiers (`user@domain`) | human-readable identity |
| [NIP-07](https://nips.nostr.com/7) | Browser-extension signer (`window.nostr`) | key management — **browser-only** |
| [NIP-23](https://nips.nostr.com/23) | Long-form content, kind 30023 (Markdown) | pack announcements |
| [NIP-25](https://nips.nostr.com/25) | Reactions | reviews/comments |
| [NIP-46](https://nips.nostr.com/46) | Remote signing (signer app holds keys) | key management |
| [NIP-47](https://nips.nostr.com/47) | Nostr Wallet Connect (NWC) | in-app Lightning payments |
| [NIP-57](https://nips.nostr.com/57) | Lightning Zaps (kinds 9734/9735) | monetization |
| [NIP-78](https://nips.nostr.com/78) | App-specific data, kind 30078 | arbitrary app data |
| [NIP-96](https://nips.nostr.com/96) / [NIP-b7](https://nips.nostr.com/b7) | HTTP file storage / Blossom blob servers | hosting actual pack files |
| [NIP-99](https://nips.nostr.com/99) | Classified listings | LFG-style listings |
| [NIP-11](https://nips.nostr.com/11) | Relay information document | relay limits/policies |

## 2. Session rendezvous / discovery via nostr

**Question:** could relays serve as a rendezvous layer for finding/opening Live
Table sessions (today: iroh ticket shared out-of-band, or LAN URL + 6-char PIN)?

### How it would work

The host publishes an invite event containing the iroh ticket / LAN URL + PIN:

- **Ephemeral event** (kind 20000–29999, e.g. `20050` "session invite"): relays
  accept it but don't store it — fire-and-forget lobby beacons. Players subscribe
  with a filter (`{kinds: [20050], "#t": ["pf2e-companion"]}`).
- **Addressable event** (kind 30000–39999, e.g. `30078` app data with `d` =
  session id, or a custom `30xxx` kind): the invite persists, is updatable by the
  host (kind 30078 addressable semantics: one live event per pubkey/kind/d), and
  can carry an `expiration` tag (NIP-01 `expiration` tag) so relays drop stale
  invites.
- Joiners fetch, validate the host's signature, redeem by connecting with the
  embedded ticket (the existing join path, unchanged).

### Precedent

- **Trystero** ([github.com/dmotz/trystero](https://github.com/dmotz/trystero)) is
  the best-known "nostr as signaling/rendezvous" system — WebRTC room discovery
  over nostr relays. Its history is the cautionary tale:
  [issue #91](https://github.com/dmotz/trystero/issues/91) — using *public* relays
  for room coordination got the project flagged for relay-spamming; the practical
  guidance became "run your own relay for signaling."
- **Jester** ([jesterui.github.io](https://jesterui.github.io/)) is chess over
  nostr — a self-described "proof of concept with many, many bugs." There is no
  mature nostr lobby/matchmaking ecosystem to piggyback on as of Oct 2026; a
  search for nostr tabletop-LFG found nothing nostr-native at all.

### Comparison vs the existing iroh stack

iroh already provides dial-by-NodeId with **three** discovery routes
([docs.iroh.computer — Address Lookup](https://docs.iroh.computer/concepts/address-lookup)):
the N0 relay (QUIC/UDP 443, tested via `PF2E_NET_WAN=1`), pkarr-signed DNS
publication (n0's public `PkarrPublisher` + DNS server), and local mDNS-style
lookups. nostr would add none of *connectivity* — only a **searchable lobby
directory** on top.

| Criterion | iroh ticket + deep link (planned Tier 1) | nostr rendezvous |
|---|---|---|
| Servers needed | N0 relay (already a dep, shared infra) | + one or more nostr relays |
| Privacy of invite | ticket shown only to invited peers | invite visible to relay operator + anyone subscribing |
| Spam/sybil surface | none (invite is a capability) | lobby flooding, fake lobbies, kind squatting on public relays |
| Offline/LAN play | works | broken (needs relay) |
| UX | paste link / tap deep link | discover lobby → join (extra mode) |
| New code | ~0 (already shipped) | relay client, invite schema, expiry, abuse handling |

### Assessment

**Reject.** The value proposition ("find open tables") presumes a public-lobby
social mode that this project deliberately does not have (friends-and-family
tables, invites out-of-band). The spam/moderation burden of running a public lobby
is precisely what SHIPPING.md Tier 3 item 18 tries to avoid with "optional
self-hostable rendezvous … still serverless-by-default." If lobby discovery is
ever wanted, a self-hosted rendezvous (item 18) is smaller, private, and
auditable; nostr relays would be a strictly heavier version of the same thing
with an anonymous-spam problem attached.

## 3. Identity: nostr keypairs vs planned ed25519 peer identity

**Question:** nostr keypairs (secp256k1, NIP-01) as portable player/GM identity,
vs Tier-2 item 10's per-peer ed25519 identity + capability tokens?

### The two systems side by side

| | nostr identity | Tier-2 item 10 plan |
|---|---|---|
| Curve | secp256k1 | ed25519 (iroh-native; NodeId *is* the key) |
| Portable across devices/apps | yes (that's the point) | no (per-peer, per-app) |
| Works offline | signing yes, *proof* requires relays | yes, cryptographic capability tokens |
| Key count per user | 1 master key | 1 key per peer relationship |
| Protocol weight | NIP-01 events; NIP-05/NIP-46 ecosystem | raw signatures in existing QUIC framing |
| Failure mode | key loss = identity loss everywhere | re-key = new peer, low blast radius |

**One key cannot serve both** — secp256k1 ≠ ed25519. A unification is possible
only by deriving both from a shared master seed, which multiplies key-management
complexity rather than reducing it.

### Key management in a Tauri 2 app

- **NIP-07** (extension signer) is browser-only — not available in Tauri's
  WebView for practical purposes (it requires a browser extension contract).
- **NIP-46** (remote signer) — pairing the app with a signer app (e.g. Amber on
  Android) — is a real option but adds a second app, a pairing flow, and failure
  modes that are hard to explain to a non-technical parent running a family game
  night.
- **Embedded keys** (generate in-app, store via the existing `keystore.rs`
  keyring integration) is the realistic option. That works — but it is exactly
  the "account system" posture SHIPPING.md excludes, minus the portability
  nostr users actually care about (social graph, reputation, DMs — none of which
  this app has).
- **NIP-05** (verified `name@domain` identifiers) requires running DNS for a
  domain — pure overhead for a feature with no in-app consumer.

### Assessment

**Reject as a core identity layer.** Tier-2 item 10's design (ed25519 per-peer
identity → capability tokens, kick/ban) is smaller, offline-first, and matches
iroh's threat model. nostr identity's benefits (portability, cross-app
reputation) only pay off in a public-network social context this project
deliberately avoids. The single defensible use of a nostr key is as **author
metadata**: a content-pack author may optionally attach their nostr pubkey to a
pack manifest (see §4) — an attribute, not an identity system.

## 4. Content distribution: signed packs as nostr events

**Question:** signed adventure/content-pack announcements and distribution as
nostr events, vs the planned "pack format v2 with semver + signatures"
(SHIPPING.md Tier 3 item 16)?

### What nostr offers here

- **Author keypair + signed events** — the same primitive as pack-format-v2
  signatures, so *announcement* signing is free once author keys exist.
- **Kind 30023 (NIP-23 long-form)** is Markdown-native with title/summary/image
  tags — a decent fit for a pack *announcement* (not the pack itself).
- **Kind 30078 (NIP-78)** is the generic "application-specific data" addressable
  event — the natural place for a structured pack manifest/index event
  (`d` = pack id, semver in a tag, content hash, license badge).
- **Versioning**: addressable-event semantics give you "latest event per
  (pubkey, kind, d)" — i.e., a replaceable pointer, with `expiration` tags
  available.

### The hard constraint: size

Relays cap event size — nostr-rs-relay defaults to **128 KB** per event, and
public relays commonly reject large events. The two bundled adventures are small,
but a real pack ecosystem (statblock sidecars, scene images) will exceed this
immediately. nostr itself acknowledges this: the companion
[NIP-96](https://nips.nostr.com/96) / [NIP-b7 Blossom](https://nips.nostr.com/b7)
file-storage specs exist precisely so events *reference* blobs by URL/SHA-256
instead of embedding them. So the real architecture would be:

```
nostr event (kind 30078, ~1 KB manifest + signature)
  └── pack URL + SHA-256 (hosted on Blossom server, Git, or any web server)
        └── downloaded, hash-verified, license-validated on ingest
```

At which point nostr is providing: **discovery + authenticity + update
notifications**, and nothing else. Downloads, license enforcement
(`orc | community-use | homebrew | proprietary` stays client-side on ingest, as
today), and provenance display are unchanged.

### Comparison vs the alternatives for item 16

| Channel | Server to run | Auth | Update push | Discovery | Effort |
|---|---|---|---|---|---|
| GitHub repo + releases (PR review, CI `validate_statblock` gate) | none (GH hosts) | git signatures/PRs | RSS/webhook | GitHub search | lowest — matches item 16's existing plan |
| In-app pack browser over GitHub API | none | same | poll | in-app | low |
| nostr announcements (kind 30078) + Blossom/Git blobs | none (public relays) or own relay | nostr key | live via relay subs | relay filters | medium |
| Self-hosted registry | DO droplet | custom | custom | in-app | medium-high (item 18 pattern) |

Item 16 already specifies "community-submitted adventures reviewed via PR +
`validate_statblock` CI gate" — a Git-centric flow. nostr would *additionally*
let authors push signed updates without GitHub, and let the app subscribe to an
author's events instead of polling. That is a genuine decentralization win that
is consistent with serverless-by-default (the app consumes relays; it runs
nothing).

### Assessment

**Adopt-specific-subset, deferred to Tier 3, conditionally.** Conditions:
(i) pack format v2 with its own signature scheme ships first (nostr consumes
the manifest, it must not *define* it); (ii) nostr is one *optional* channel
alongside the primary Git/download channel, never the only one; (iii) the app
connects to a small curated relay list (or the author's declared relay via
NIP-11 `relay` hints) rather than treating public relays as a pack source of
truth; (iv) all nostr-sourced packs still pass the same license-provenance and
`validate_statblock` gates as local ones.

## 5. Monetization via zaps (NIP-57)

Verified: **NIP-57** defines zaps — kind 9735 zap receipt (kind 9734 zap
request) Lightning payments attached to events. **NIP-47** defines Nostr Wallet
Connect (NWC), which lets a client pay through a remote Lightning wallet via
encrypted nostr events (the model behind Alby-style "paste your NWC URL" flows).

### Integration options, concretely

1. **Zap button on packs/authors (desktop web UI only)** — render a Lightning
   Address / LNURL; payment happens in the user's wallet outside the app. No
   NIP-57 machinery needed in-app beyond a `lightning:` link.
2. **App-development donation address** — a static Lightning Address in `/about`
   or on the website. Zero nostr dependency.
3. **In-app NWC payments (NIP-47)** — full in-app checkout: user pastes a NWC
   connection string, app requests `pay_invoice` over nostr. This is the only
   option that makes nostr a *payments* dependency.
4. **Zap-gated premium adventures** — content unlocked by verifying a zap
   receipt (kind 9735) before download. This is a paywall; it converts the app
   into a marketplace client.

### Why each fails or survives, for THIS app

- **Option 4 (zap-gating) contradicts SHIPPING.md outright**: "marketplace
  payments" is in the explicitly-out-of-scope list. It also creates a
  license-provenance mess (paid tiers vs ORC/Paizo CUP/MIT posture: ORC and
  community-use content can't be paywalled by a third party; only `homebrew`/
  `proprietary` could, which splits the ecosystem).
- **Option 3 (NWC in-app) is an app-store hazard.** Precedent: in June 2023
  Apple notified **Damus** that zaps violated Guideline 3.1.1 (payments for
  digital content outside IAP) and would be removed in 14 days; Damus survived
  by removing the in-app zap button on iOS ([The Block,
  2023](https://www.theblock.co/news/business/2023-06-13-apple-damus-app-removal-warning-234392);
  [CNBC, 2023](https://www.cnbc.com/2023/06/13/jack-dorseys-damus-may-be-thwarted-by-apples-strict-payment-rules.html)).
  The 2025 Epic contempt ruling forced Apple to allow *external purchase links*
  on the **US storefront** (guideline 3.1.1(a), upheld by the Ninth Circuit Dec
  2025) — but that is US-only, carries entitlement/link-out mechanics, and does
  not clearly cover Lightning tipping of third parties. Google Play similarly
  requires Play Billing for digital goods. A Tier-2 "public release on stores"
  goal (item 7) and in-app Lightning payments are in direct tension.
- **Option 2 (donation address) is the only consistent reading**: a tip to the
  project or an author is not a marketplace transaction (nothing is gated, no
  entitlement is purchased), and an external link keeps it out of IAP scope.
  Note Apple permits donation links to registered nonprofits and allows
  fundraising links under narrow conditions; a hobby project is neither —
  safest form is a donation link on the **website/GitHub**, not in the iOS app.
- **Audience optics**: the stated audience is Christian TTRPG families whose
  values include stewardship and "no speculation/gambling optics" (SHIPPING.md
  item 9's tone work). Bitcoin-denominated tipping as a *prominent* in-app
  feature is off-brand for that audience even where it is legal; a quiet
  "support the project" link is fine.

### Assessment

**Reject in-app zaps/NWC and any zap-gating. Donations, if ever desired, are a
plain Lightning Address or GitHub Sponsors link on the website — which requires
zero nostr integration.** There is no monetization use-case here where nostr is
the right tool rather than an external link.

## 6. Community / social features (LFG, comments, reviews)

- **LFG listings**: NIP-99 (classified listings) or addressable events could
  host "table recruiting" posts; there is **no existing nostr tabletop-LFG
  ecosystem** (nothing found in Oct 2026 searches) — the project would be
  building and moderating the category from zero.
- **Comments/reviews on packs**: NIP-25 reactions on pack-announcement events
  are trivially implementable, but see moderation reality below.
- **Moderation reality on public relays**: relay spam is an active, unsolved
  problem in 2026 — responses include pay-to-post relays and web-of-trust
  filtering (e.g. PlebOne); relay operators each set their own policy via NIP-11
  documents. Public relays also carry unfiltered NSFW content adjacent to any
  tag your app subscribes to. For an app whose differentiator includes
  table-content-filter hardening (item 9) and child-at-table safety, attaching
  the in-app experience to open public relays is a direct contradiction.
- **Does a Christian-audience app want open-relay social?** The 5-lens pack
  design deliberately avoids denominational arbitration disputes (out of scope).
  A public comment stream on religious content is a denominational flame war
  magnet that the project has explicitly declined to staff for.

**Assessment: Reject.** If community is wanted later, first-party channels
(GitHub discussions, a moderated forum/Discord) moderate cleanly and don't touch
the app binary.

## 7. Engineering reality check

### Rust crates (verified Oct 2026)

- [`nostr`](https://crates.io/crates/nostr) v0.45.5 and
  [`nostr-sdk`](https://crates.io/crates/nostr-sdk) v0.45.4 (released 2026-09-19;
  ~76 versions; ~1.7M downloads; ~740k recent) — actively maintained by
  nostrdevkit (formerly rust-nostr), MIT, MSRV **1.85.0**.
- **MSRV conflict**: this project's README states "Rust 1.77+". Adopting
  nostr-sdk forces a toolchain bump to 1.85+. Minor, but it's a real constraint
  to record (CI matrix pins would need updating).
- nostr-sdk feature flags are reasonable: default `ring` + rustls/webpki-roots
  (overlaps the existing iroh/tokio/rustls stack), and a **`local-relay`**
  feature (in-process `nostr-memory` relay) that fits this project's test
  culture perfectly — default tests run against an in-process relay, with an
  env-gated `PF2E_NET_NOSTR=1` (say) for real-relay WAN tests, mirroring the
  existing `PF2E_NET_WAN`/`PF2E_NET_TESTS` pattern.
- Bundle size: secp256k1 (C, with optional asm) plus the SDK. Expect a
  low-single-digit MB addition on desktop; negligible relative to the
  iroh/automerge/sqlite stack already carried. Not a blocker; also not free.
- **Tauri 2 mobile**: nostr-sdk is pure Rust + rustls (no OpenSSL), compiles to
  wasm and native targets; no known iOS/Android-specific issues. Less battle
  tested on iOS than on desktop/web, but no red flags found.

### Dependencies & architecture weight

- Adding nostr means: a relay-pool manager, a second key type, an event store
  (even if only a cache — nostr-sdk-sqlite exists but this project already has
  SQLite and would presumably map events into its own tables), and a new
  network-permission surface on mobile (already needed for iroh/LAN, so no new
  manifest entries — but a new *destination*).
- All of that is avoidable for every use-case except the §4 subset.

## 8. Comparison table

| Use-case | Value for this project | Effort | Values fit | Store-policy risk | Recommendation |
|---|---|---|---|---|---|
| 1. Session rendezvous | Low (solved by iroh + deep links) | Med | Poor (relay = server + metadata exposure; spam) | None | **Reject** |
| 2. Portable identity | Low–negative (contradicts "vault is the account") | Med | Poor (account system adjacent) | None | **Reject** (author pubkey as metadata OK) |
| 3. Content distribution | Medium (signed announcements, updates) | Med | Good (consumes relays, runs nothing) | None | **Adopt subset, Tier 3, conditional** |
| 4. Monetization (zaps) | Low | Med–High | Poor (optics, out-of-scope) | **High** (Apple 3.1.1, Damus precedent) | **Reject** (website donation link if anything) |
| 5. Community/LFG | Low–medium hypothetical | High | Poor (moderation, child safety) | Medium | **Reject** |

## 9. Risks (if any nostr integration proceeds)

1. **Relay trust**: relays see every subscription and can serve fabricated-but-
   unsigned-filtered views; the app must verify event signatures client-side
   (nostr-sdk does) and treat relay lists as untrusted input.
2. **Spam/sybil** on public relays (Trystero precedent) — must default to a
   curated relay list, ideally author-declared relays.
3. **Key management for non-technical users** — embedded keys in `keystore.rs`
   with no recovery path; a lost pack-author key breaks update continuity
   (addressable events are per-pubkey). Document a key-rotation story before
   shipping.
4. **Toolchain drift**: MSRV 1.85 vs the documented 1.77+; CI matrix updates.
5. **Scope creep**: each NIP pulled in (NIP-05, NIP-46, NIP-57…) drags UX and
   test surface; keep the integration to NIP-01 + NIP-78 (+NIP-11 for relay
   info) at most.
6. **App-store review ambiguity**: even non-payment nostr features can draw
   scrutiny on iOS if review perceives a social network (user-generated content
   without moderation/blocking is itself a rejection risk under Apple's UGC
   rules) — one more reason to keep nostr out of the interactive surface.

## 10. Phased adoption sketch (mapped to SHIPPING.md tiers)

Only the §4 subset survives into a plan; everything else is a no:

- **Tier 1–2: nothing.** No nostr work. Deep links (item 2) and ed25519 peer
  identity (item 10) proceed exactly as written — they are the better answers
  to the questions nostr raises.
- **Tier 3, alongside item 16 (pack ecosystem), conditional on pack format v2
  shipping with its own signatures:**
  - 16a. Define the pack manifest (semver, license badge, content SHA-256,
    optional author nostr pubkey + relay list).
  - 16b. *Optional*: nostr announcement channel — publish kind 30078 addressable
    events (`d` = pack id) from the author's key; the in-app pack browser
    (item 16's) may subscribe to a curated relay list as a *secondary* source
    next to the primary Git/download channel.
  - 16c. Tests: in-process `local-relay` feature for default `cargo test`; a
    `PF2E_NET_NOSTR=1`-gated WAN test against one real relay, mirroring
    `PF2E_NET_WAN`.
- **Item 18 (federation/rendezvous): keep as planned** — self-hostable
  rendezvous, *not* nostr relays. If lobby discovery is demanded by playtests,
  item 18's server is the private, auditable version.
- **Never**: session invites over public relays; nostr as the identity layer;
  in-app NWC/zaps; zap-gated content; open-relay social/comment surfaces.

## 11. Ranked recommendation

Ranked by value/effort **for this project specifically**:

1. **Content distribution (signed pack announcements)** — adopt-specific-subset,
   Tier 3, conditional (pack v2 signatures first; optional channel, never sole;
   curated relays; same license/validator gates). This is the only place nostr's
   design (signed, serverless-to-consume, update-push) maps onto a real gap
   (item 16's distribution story).
2. **Session rendezvous** — reject. iroh + N0 relay + pkarr DNS + deep links
   already cover it with less machinery, better privacy, and no spam surface.
3. **Identity** — reject for the app; a nostr pubkey as *author metadata* on
   packs is fine. Keep Tier-2 item 10's ed25519 capability-token design.
4. **Community/LFG** — reject for the app (moderation + child-safety); use
   first-party moderated channels if community is ever wanted.
5. **Monetization via zaps** — reject (audience, policy, and out-of-scope
   triple-fail). A website-side Lightning Address / GitHub Sponsors link is the
   only consistent tip mechanism, and it needs no nostr code.

## Sources

- NIPs (official repo / mirror, verified Oct 2026):
  [NIP-01](https://nips.nostr.com/1) (events, kinds incl. ephemeral 20000–29999
  and addressable 30000–39999; formerly NIP-16/33 content folded in),
  [NIP-05](https://nips.nostr.com/5),
  [NIP-07](https://nips.nostr.com/7),
  [NIP-23 kind 30023](https://github.com/nostr-protocol/nips/blob/master/23.md),
  [NIP-25](https://nips.nostr.com/25),
  [NIP-46](https://github.com/nostr-protocol/nips/blob/master/46.md),
  [NIP-47](https://github.com/nostr-protocol/nips/blob/master/47.md),
  [NIP-57 kinds 9734/9735](https://nips.nostr.com/57),
  [NIP-78 kind 30078](https://nips.nostr.com/78),
  [NIP-96](https://nips.nostr.com/96),
  [NIP-b7 Blossom](https://nips.nostr.com/b7),
  [NIP-99](https://nips.nostr.com/99),
  [NIP-11 relay info](https://nips.nostr.com/11).
- Rust: [nostr crate v0.45.5](https://crates.io/crates/nostr),
  [nostr-sdk v0.45.4 (2026-09-19, MSRV 1.85)](https://crates.io/crates/nostr-sdk),
  [nostrdevkit/nostr repo](https://github.com/nostrdevkit/nostr),
  [nostr-rs-relay (128 KB default event cap)](https://github.com/scsibug/nostr-rs-relay),
  [nostr-blossom crate](https://docs.rs/nostr-blossom),
  [route96 (NIP-96/Blossom server)](https://github.com/v0l/route96).
- iroh discovery: [Address Lookup docs (pkarr + n0 DNS + relay)](https://docs.iroh.computer/concepts/address-lookup),
  [iroh-dns blog](https://www.iroh.computer/blog/iroh-dns),
  [iroh_net::discovery docs](https://docs.rs/iroh-net/latest/iroh_net/discovery/index.html).
- Precedents: [Trystero (nostr signaling)](https://github.com/dmotz/trystero)
  and [relay-spam issue #91](https://github.com/dmotz/trystero/issues/91);
  [Jester chess over nostr](https://github.com/jesterui/jesterui).
- Apple/payments: [Damus removal warning (The Block, 2023)](https://www.theblock.co/news/business/2023-06-13-apple-damus-app-removal-warning-234392),
  [CNBC on Damus vs Apple IAP](https://www.cnbc.com/2023/06/13/jack-dorseys-damus-may-be-thwarted-by-apples-strict-payment-rules.html),
  [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/),
  [Apple guideline update for US external links (2025)](https://developer.apple.com/news/?id=9txfddzf),
  [Ninth Circuit Dec 2025 ruling analysis](https://grow.cleverbridge.com/blog/apple-vs-epic-a-legal-turning-point).
- Relay moderation/spam: [OpenSats — Advancements in Nostr Relays (WoT filtering)](https://opensats.org/blog/advancements-in-nostr-relays).
- Project context: `README.md`, `docs/SHIPPING.md` (this repo).
