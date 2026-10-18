# Real-device evening — 2026-10-18 (SHIPPING Tier 1 item 1)

Method: test-and-document pass on physical hardware, no source changes. Phone:
Pixel 7a `lynx`, Android 16 (GrapheneOS; default browser **Vanadium**, not
Chrome). Desktop: Linux laptop on the same Wi-Fi (192.168.1.0/24). Evidence
PNGs + logs live in `device-lab/` (gitignored this session); filenames below
reference it.

Three lanes were driven:

- **Lane A — phone browser over LAN**: static frontend via
  `corepack pnpm preview --host` (note: `pnpm preview -- --host` does NOT
  bind 0.0.0.0) + the e2e `e2e_host` recipe (`tests/e2e/server/liveHost.ts`,
  PIN `A7K9M2`) reached two ways: (a) a python TCP relay bound to
  `0.0.0.0:4781` → loopback host (real Wi-Fi path of the frames), and (b)
  `adb reverse` loopback with **phone Wi-Fi disabled** to prove the USB-bridged
  lane and the origin-security contrast. The relay validated the frame
  protocol over real Wi-Fi but is documented as a relay, not app binding.
- **Lane B — debug APK + `pf2e://` deep links** on the phone itself
  (`corepack pnpm tauri android build --apk --debug`, `adb install`,
  `am start -d 'pf2e://…'`).
- **Lane C — real desktop app ↔ real phone over true Wi-Fi** (the capstone):
  `corepack pnpm tauri dev` under Xvfb (requires
  `WEBKIT_DISABLE_DMABUF_RENDERER=1 LIBGL_ALWAYS_SOFTWARE=1 GDK_BACKEND=x11`),
  campaign `evening-table`, hosted session "Widow Lamp Night"
  (id `21aaa0f9-83b2-4512-bf9f-be87e7b5d7ee`, PIN `KSVXQQ`, listening on
  `0.0.0.0:4780`), phone joined as "Gabriel".

## Verdicts at a glance

| # | Item | Verdict |
|---|------|---------|
| A1 | Preview reachable from phone over LAN | PASS (with friction, F-R4) |
| A2 | Lobby URL prefill (`?join=1&url=…`) | PASS |
| A3 | Browser Join button → live session | **FAIL** (F-R1) |
| A4 | Direct `/table/play?…` join from phone browser | PASS |
| A5 | GM→phone chat over Wi-Fi | PASS |
| A6 | Phone→GM chat | dead end (known seam, F-R7) |
| A7 | Secret roll redaction on player device | PASS |
| A8 | Adventure badge + party roster on phone | PASS (nit F-R8) |
| A9 | Wrong-PIN rejection + lobby redirect | PASS |
| A10 | USB-loopback lane (Wi-Fi off) + secure-context contrast | PASS |
| B1 | Debug APK build | PASS, first attempt |
| B2 | Install + first-launch wizard (3 steps) | PASS |
| B3 | Deep-link manifest injection + permissions | PASS |
| B4 | Hot deep link (app running) → prefilled lobby | PASS |
| B5 | Cold-start deep link (pinned) → prefilled, wizard skipped | PASS |
| B6 | Unpinned deep link: PIN empty on cold start | PASS (nit F-R10) |
| B7 | Malformed deep links (`pf2e://bogus`, `http` url, bad PIN) | PASS — safe no-op |
| B8 | Native Join button from prefilled lobby | **FAIL** (F-R1) |
| B9 | Native lobby layout at real viewport | overlap (F-R6); play surface clear (F1 holds) |
| C1 | Real desktop host, phone joins over Wi-Fi | PASS |
| C2 | Advertised LAN URL correctness | **FAIL** (F-R2, major) |
| C3 | GM "Enter Table →" on desktop | **ANOMALY** (F-R3) |
| C4 | Camera/avatar/permission audit | N/A by design (F-R5) |

## Major

### F-R1 — Browser-join and native deep-link join both dead: `session_join_lan` does not exist
Exact strings observed:
- Browser (Vanadium, lobby Join button):
  `TypeError: Cannot read properties of undefined (reading 'invoke')`
- Native (Pixel 7a, deep-link-prefilled lobby, tap Join):
  `Command session_join_lan not found`

Root cause: `src/routes/table/+page.svelte:115` calls
`invoke("session_join_lan", …)` but no such command is registered in
`src-tauri/src/lib.rs`'s invoke_handler — the command only ever existed as an
e2e mock (`e2e/fixtures.ts:388`). This is the R1 gap, now confirmed on a real
device via TWO entry paths (typed URL prefill in a browser, and a genuine
`pf2e://join?url=…&pin=…` tap-through in the installed app). Workaround proven
today: paste `/table/play?name=…&host=…&pin=…` directly — that path is fully
functional.
Evidence: `04-join-error.png`, `24-join-error-native.png`.
Direction: implement `session_join_lan` in Rust — spawn a WS client actor to
the host's `ws://ip:port/session_id`, authenticate with the PIN, and bridge
frames onto the existing `session:patch` / `session:presence` /
`session:error` Tauri events so `ipcTransport()` works unchanged. Bonus:
lobby should catch the missing-command error and degrade to a readable
message rather than a raw TypeError for plain-browser users.

### F-R2 — Advertised LAN URL shows the VPN tunnel IP, not the Wi-Fi IP
The host lobby advertised `ws://10.146.185.179:4780/21aaa0f9-…`. That address
is this laptop's **`wg0-mullvad` tunnel address (/32)** — unreachable from the
phone. The phone joined only after manually correcting the URL to
`ws://192.168.1.172:4780/…`.
Root cause: `local_lan_ip()` in `src-tauri/src/lan.rs:105` UDP-"connects" to
`8.8.8.8:80` and reads `local_addr()`. On a Mullvad-VPN machine, policy
routing sends that probe down the tunnel (default route metric 100 is the
USB-ethernet dock; metric 600 is Wi-Fi — neither wins for 8.8.8.8 once the VPN
hijacks the route), so the "LAN IP" is the tunnel's.
Why it matters: the single most prominent string on the host's screen was
wrong on a very common friends-beta setup (VPN on the GM's laptop). The
"Copy invite link" flow embeds the same wrong IP, so deep links minted while
hosting would misdirect every player.
Evidence: `46a-enter-table-before.png`, desktop `host-events.jsonl`,
`ip route` capture.
Direction: enumerate candidate addresses instead of one probe — collect
non-loopback IPv4 addrs on UP interfaces that are NOT `wg*`/`tun*`/`utun*`,
prefer the subnet of the physical default route, and (cheap win) show ALL
candidate URLs in the lobby with the preferred one first, plus a manual
override field. A unit test can stub the interface list.

### F-R3 — GM "Enter Table →" anchor does not navigate (WebKitGTK dev build)
On the real desktop build (Xvfb, WebKitGTK), with hosting active:
1. Pointer click on the `Enter Table →` anchor: **no navigation** (verified
   across multiple attempts at its exact bounds).
2. Keyboard path: focus moved cleanly (Tab from "Copy invite link" puts a
   visible focus ring on the anchor — real focus, correct target) → Return:
   **still no navigation**, no error toast, no transient play-page flash at
   +0.5 s.
3. Same-page `<button>` interactivity works at the same coordinates (the
   JOIN panel's "LAN URL" tab flips correctly on click).
4. The play route itself is fine — the phone browser renders
   `/table/play?…` fully (badges, chat, roster, redaction).
5. `Ctrl+Shift+I` did not open devtools in this session, so the webview
   console is unread. The anchor is a plain `<a class="btn"
   href="/table/play">` (`+page.svelte:313`); no `beforeNavigate`/global
   click interceptors exist in `src/`.
Undiagnosed. Possibly dev-build/WebKit-specific (the e2e GM path runs in
Chromium and passes 4/4). Evidence: `46a`–`46f` (focus-ring shot `46d` is
the money shot).
Direction: reproduce with devtools attached
(`WEBKIT_INSPECTOR_SERVER=127.0.0.1:9223` before launch on webkit2gtk), and
if the console shows a navigation-canceling error, file it; also consider
converting the anchor to a `goto("/table/play")` button to sidestep the
whole class.

## Minor

### F-R4 — Vanadium HTTPS-upgrade interstitial on http:// LAN origins
First visit to `http://192.168.1.172:4173/` on GrapheneOS Vanadium shows a
full-screen interstitial: "192.168.1.172 doesn't support a secure
connection with HTTPS" (HTTPS-First mode), requiring an explicit tap-through.
Expected Chromium-family behavior, not an app bug — but it is the FIRST thing
every friends-beta phone user will hit if they join via browser.
Direction: document the tap-through in the join docs; long-term the native
deep-link path (once F-R1 is fixed) avoids it entirely.

### F-R5 — Origin-security contrast is real and visible
- LAN origin (`http://192.168.1.172`): omnibox shows a red "not secure"
  chip; `window.isSecureContext`-class features are unavailable.
- USB-loopback origin (`http://127.0.0.1:4173` with phone Wi-Fi OFF):
  plain ⓘ, potentially-trustworthy context.
Camera/mic audit: **no camera, avatar-capture, or `getUserMedia` usage exists
anywhere in `src/`** (grep-verified), so there is currently nothing to be
blocked by this. If avatar capture is ever added, it must treat plain-LAN
origins as insecure — the loopback lane is the only browser context where it
would work. Evidence: `13-usb-loopback-joined.png`.

### F-R6 — Native lobby: "Join Session" button partially occluded by fixed tabbar
On the phone's installed app (`24-join-error-native.png`, real 390-CSS-px
viewport), the JOIN panel's "Join Session" button renders with its bottom
~40% painted UNDER the fixed bottom tabbar. Still tappable (the F-R1 error
toast was triggered through it), but this is an F1-class overlap on a
non-play route, on native. Counterpoint, positively verified: the F1 fix
(commit `426a1ed`) DOES hold on the play surface — `10-play-top.png` shows
composer, dice input, and Secret checkbox fully clear of the tabbar on the
same phone. Direction: apply the play-surface padding audit to the lobby and
other tall routes at real viewports (the harness fullPage note in
`ui-reviews/README.md` explains why the static shots couldn't separate these).

## Nits

- **F-R7** Phone→GM chat is a known dead end: phone sends, nothing reaches the
  host log (matches the header note in `tests/e2e/live-lan.spec.ts`). GM→phone
  works. Evidence: `07-chat-phone-dead-end.png`.
- **F-R8** On the phone the play-surface section tabs render icon-only
  (💬⚔👥🗺📜, labels dropped) with no accessible fallback visible; confirm
  aria-labels survive the collapse. The global bottom bar's labels
  (Search/Encounter/…) DO still show. Evidence: `09-party.png`, `11-party.png`.
- **F-R9** Lens picker text differs by surface: browser shows plain
  "Lewisian", native shows "Lewisian (mere Christianity)". Cosmetic, likely
  truncation/width difference; worth one glance.
- **F-R10** Unpinned warm deep link leaves a previously-typed PIN in the field
  ("sticks"); cold unpinned start correctly clears it. Decide whether
  stickiness is a feature (re-join convenience) or a leak (shared device) —
  either way, one-line doc note.
- **F-R11** Debug APK is **1.62 GB** (`app-universal-debug.apk`): 4 ABIs,
  unstripped, `.so` payloads ~370–420 MB each. Expected for a universal debug
  build; just remember installs are slow (~2 min) and to build per-ABI for any
  friend install. Manifest check: `INTERNET` + `usesCleartextTraffic=true`
  only — no camera/mic permissions; deep-link plugin injected
  `<data android:scheme="pf2e"/>` as designed. applicationId is
  `io.github.gkrause.pf2e_companion` — note the UNDERSCORE (matters for any
  future App-Links/assetlinks.json).

## Environment + harness notes (NOT app bugs)

- `corepack pnpm tauri android build --apk --debug` succeeded on the FIRST
  attempt with only env: `ANDROID_HOME=~/Android/Sdk`,
  `NDK_HOME=$ANDROID_HOME/ndk/27.0.12077973`, Java 17, and the four android
  rust targets preinstalled. No system installs were needed.
- adb quoting: `&` inside `-d` URLs is eaten by the *device* shell unless the
  whole intent is single-quoted from the host:
  `adb shell "am start … -d 'pf2e://join?url=…&pin=…'"`.
- Vanadium/uiautomator quirks: input labels surface as `hint` (not `text`),
  tab icons carry no text, and bounds degenerate while the keyboard is open —
  drive by screenshot coordinates; device px = screenshot px × 1.2.
- Xvfb pane is `:100` here (check `/tmp/.X11-unix`, several stale locks); the
  webview renders ONLY with the three env vars listed above. The plain
  `cargo build --release` binary points at devUrl localhost:1420 — use
  `xvfb-run -a corepack pnpm tauri dev`.
- Killing harness processes: match with bracket guards
  (`pgrep -f "pf2e[-]companion"`); a plain `pkill -f preview` self-matched and
  killed its own driver once today.
- Phone housekeeping: `svc power stayon` restored to `false`, all
  `adb reverse` mappings removed, app left installed. No personal screens
  were captured (two accidental frames containing non-app surfaces were
  deleted on the spot).

## Recommended next dispatch order

1. **F-R1** `session_join_lan` (unblocks browser-join UX AND the whole
   deep-link value proposition — highest leverage).
2. **F-R2** LAN IP selection (friends-beta blocker on any VPN'd host).
3. **F-R3** GM Enter Table anomaly (may be dev-only, but verify before
   shipping the desktop bundle).
4. F-R6 lobby tabbar overlap + F-R7 phone→GM seam, then the nits.
