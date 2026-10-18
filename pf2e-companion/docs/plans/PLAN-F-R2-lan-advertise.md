# PLAN — F-R2: LAN advertise-address selection (real-device evening)

Status: IMPLEMENTED on branch `fix/r2-lan-advertise-iface-enum`. Source of
truth: `docs/ui-reviews/2026-10-18-real-device.md` § F-R2 (SHIPPING Tier 1
follow-up, friends-beta blocker).

## 1. Problem

`local_lan_ip()` (`src-tauri/src/lan.rs:105` @ 684bc3c) discovered the
"LAN IP" with a no-packet UDP trick: `UdpSocket::connect("8.8.8.8:80")`
then `local_addr()`. That follows the host's **policy routing**. On the
Mullvad-VPN'd host at the real-device evening the probe went down the
`wg0-mullvad` tunnel, so the lobby advertised
`ws://10.146.185.179:4780/…` — the tunnel's **/32 host address**,
unreachable from any phone on Wi-Fi. The phone joined only after the URL
was hand-corrected to `ws://192.168.1.172:4780/…`. The same wrong IP fed
"Copy invite link", so every `pf2e://join` deep link minted while hosting
would have misdirected every player.

### Evidence

- `docs/ui-reviews/2026-10-18-real-device.md` § F-R2: observed URL,
  `ip route` capture (VPN hijacks the 8.8.8.8 route regardless of the
  dock metric 100 / Wi-Fi metric 600 split), `46a-enter-table-before.png`,
  desktop `host-events.jsonl`.
- Root-cause line: pre-fix `lan.rs` `local_lan_ip()` — the only address
  source for `advertised_addr()`, consumed by
  `session/service.rs::start_lan` → `SessionInfo.lan_url` → lobby. The
  frontend only *reads* `info.lan_url` (`src/routes/table/+page.svelte`
  lines 78/120/147), so the fix is entirely backend-side.

## 2. Approach

Enumerate, rank, and only then fallback — in `lan.rs`, keeping the
`advertised_addr(port, session_id) -> Result<String>` signature and the
`lan_url` field shape (zero frontend or IPC shape changes).

- **Enumeration**: `if-addrs = "0.14"` (optional, feature `lan`). Crate
  pick rationale: smallest single-purpose synchronous enumerator — deps
  are just `libc` (unix) / `windows-sys` (windows, version already in
  our tree via `iroh`); it exposes `name`, v4 `ip`, `prefixlen`, and
  `oper_status` (`is_oper_up()`), which `network-interface` does not.
- **Pure ranking** over a synthetic `LanCandidate { name, ip, prefixlen,
  is_up }` so tests never touch real interfaces:
  - Excludes: down interfaces; `/32` (and `/0`) host masks — the exact
    Mullvad shape; loopback; link-local 169.254/16; CGNAT 100.64/10
    (Tailscale tailnets, cellular); unspecified/multicast/experimental;
    interface names matching tunnel/virtual prefixes case-insensitively
    (`tun* tap* wg* utun* gif* stf* ipsec* ppp* vpn* tailscale* ts-*
    zt* nebula* outline* docker* veth* br-* virbr* podman* cni*
    flannel*`) — explicitly a heuristic since names are per-OS.
  - Ranks: 192.168/16 → 172.16/12 → 10/8 → any other public IPv4.
    Home Wi-Fi is overwhelmingly 192.168/16 and VPN nets that slip the
    name filter are overwhelmingly 10/8 or CGNAT, so private classes
    rank in that order; any RFC1918 beats public (a phone can't dial
    the host's WAN IP through NAT). Deterministic tiebreak: name, then
    address.
- **Fallback (never regress non-VPN hosts)**: if enumeration fails or
  ranks nothing, log a `tracing::warn!` (the repo's logging facade —
  there is no `log` crate in the tree) and run the **legacy 8.8.8.8 UDP
  probe verbatim**; if that also fails, keep the historical
  `127.0.0.1` last resort.
- **IPv6**: out of scope by construction. Enumeration records only v4;
  a v6-only host yields zero candidates → warn → probe (v4-only) →
  loopback. No crash, no v6 literal in a `ws://` URL. Home Wi-Fi is
  dual-stack; true v6-only LAN play is a non-goal.

## 3. Tests

Unit tests in `src-tauri/src/lan.rs` (`#[cfg(test)]`, run by
`cargo test --all-features` since `lan` is a default feature):

1. `wifi_beats_tunnel_slash32` — the F-R2 capture: `wlp2s0`
   192.168.1.172/24 vs `wg0-mullvad` 10.146.185.179/32 → Wi-Fi wins.
2. `tunnel_names_excluded_case_insensitive` — `WG0`/`Tailscale0`/`UTUN9`
   with non-/32 RFC1918 subnets all filtered; `enp3s0` wins.
3. `tunnel_only_falls_back_to_probe` — ranking returns `None`, the
   injected probe closure is used; probe `None` → `127.0.0.1`.
4. `link_local_excluded` — lone 169.254/16 never advertises.
5. `ipv6_only_degrades_to_loopback_via_probe` — documented v6-only
   behavior: empty candidate list → probe → loopback, no crash.
6. `down_interface_excluded` — `is_up = false` never wins.
7. `private_class_ordering_and_cgnat` — CGNAT excluded; class order
   192.168/16 → 172.16/12 → 10/8 verified stepwise.

Gates: `cargo test --all-features`, `corepack pnpm check`,
`corepack pnpm test`, `corepack pnpm build`. No frontend surface changed
(lobby reads the same `lan_url` string) → e2e lanes not required.

## 4. Acceptance criteria

- A Mullvad-connected host with Wi-Fi up advertises
  `ws://192.168.x.x:4780/…`, verifiable in the host lobby and in
  `host-events.jsonl` on the next real-device evening.
- A VPN-free host advertises exactly what it did before (enumeration
  agrees with, or outranks, the old probe result).
- A tunnel-only box degrades to the old probe behavior with a warning
  in the log.

## 5. Risks / follow-ups

- The tunnel-name list is a heuristic. Escape hatches if a setup still
  advertises wrongly: the rank preference (192.168 first) usually saves
  the day, and the review-doc "Direction" ideas — showing ALL candidate
  URLs with the preferred one first, plus a manual override field —
  remain available as a lobby-side enhancement (frontend + `lan_url`
  payload shape change; deliberately not in this slice to keep sibling
  fix branches conflict-free).
- `if-addrs` oper-status on POSIX maps `IFF_RUNNING` (`LOWER_UP`) to
  `Up` and everything else to `Unknown`; `is_oper_up()` is strictly-`Up`.
  WireGuard-style tunnels report `LOWER_UP` once configured, so "up" is
  NOT a tunnel discriminator — name/mask exclusions carry that. A
  down-but-configured NIC (`NO-CARRIER`) correctly reads as not-up. A
  host whose legit interface somehow reports `Unknown` degrades to the
  legacy probe, never worse than today.
