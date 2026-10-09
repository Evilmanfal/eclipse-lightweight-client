# Validation — Eclipse Native 0.8

Validated October 8, 2026, Windows x64, Rust 1.99.0, optimized release with thin LTO, egui/OpenGL and static CRT. Executable: 16,208,384 bytes. SHA-256: `7655a6365750cf7efb7053d706063ce62c89f55fb2912637c78876ff1d69795f`.

## Checks performed

- Application: **71 passed, 0 failed**. Server sidebar tests also verify the removed search/redundant label, the raised first category and retained DM search. New input tests exercise Ctrl+wheel direction, bounds, plain-wheel behavior, consumed scroll and reset; emoji/GIF anchors, repositioning, screen bounds, outside-click/Escape dismissal and button toggling; Shift message controls, working edit/delete hit regions, permissions and retained delete confirmation; smaller server-only online counts, guild scoping and offline/unknown exclusion. Timestamp tests cover same-day/prior-day/year boundaries, RFC3339 offsets and fractions, leap-date validation and local midnight conversion. Existing messaging, settings dismissal, hotkeys, sounds, caches, permission, identity and media tests remain green. Full output: validation/application-tests.txt.
- **41 native render-and-exit checks passed**: 24 normal-window scenarios and 17 at 1080 × 680. Screens include emoji/GIF popups, Shift actions, server hover, 75%/150% scale, zoomed account/server settings, DM moon inversion and count visibility. The offline hover diagnostics inject pointer/Shift input into the normal UI path. Each report contains the executable SHA-256. Captures contain Eclipse's own frame only.
- CLI diagnostics retained 60 frames from the real APNG/GIF fixtures and decoded a 1024-pixel JPEG. Source cadence and bounded resolution/cache behavior remain intact. Reports: validation/quality-*.json.
- PE inspection and Windows metadata identify Eclipse 0.8.0 / Eclipse.exe, WinMM feedback and Windows system DLL dependencies. The embedded orange/red eclipse icon was extracted and checked. No Electron or WebView.
- No dependency versions were upgraded; existing windows-sys bindings enable native timezone/time functions. The underlying native media engine is unchanged; its previous 63-pass, 3-ignored result is retained rather than claimed as a fresh run. Existing licensed artwork/fonts/icon remain intact.

## Memory samples

Fresh offline processes sampled after about four seconds. MiB = 1,048,576 bytes. These are short samples without a hard RAM cap or live-account/call latency benchmark.

| Offline scenario | Physical working set MiB | Private committed MiB |
|---|---:|---:|
| timestamps | 143.7 | 164.9 |
| preview | 203.1 | 220.1 |
| dms | 142.2 | 162.6 |
| emoji | 149.3 | 167.2 |
| gifs | 145.1 | 165.9 |
| quick-actions | 145.6 | 167.4 |
| server-hover | 144.4 | 164.4 |
| zoom-in | 146.3 | 171.1 |
| zoom-out | 145.3 | 165.0 |
| settings | 144.8 | 166.5 |
| settings-zoom | 147.2 | 172.0 |
| server-settings | 146.1 | 167.2 |
| server-zoom | 146.8 | 169.9 |
| roles | 145.4 | 167.3 |
| voice | 145.5 | 166.4 |
| dm-picker | 143.4 | 167.1 |
| profile | 146.2 | 165.9 |
| nitro | 143.6 | 164.7 |
| shop | 145.1 | 168.0 |
| quests | 145.9 | 167.8 |
| voice-sidebar | 145.4 | 165.9 |
| stress | 269.1 | 305.1 |
| signin | 130.7 | 148.2 |
| call | 141.6 | 160.7 |

Stress contains 200 messages and 46,202,880 decoded image bytes. The 48 MiB / 128-texture decoded cache and 12 MiB per-animation bounds remain. Three bounded loader workers prioritize icons. Existing session-only navigation cache limits remain unchanged. Decoder allocations, heap metadata and GPU resources add overhead.

## Behavior and practical limits

Ctrl+wheel zoom adjusts the same persisted 75–150% setting as Appearance. Ctrl+0 resets to 100%; Ctrl+Plus/Minus also work. At high zoom the member panel temporarily hides to preserve chat width. The server rail uses 44-point logical icons (previously 48), approximately four points of spacing, and a two-point hover expansion with a 120 ms transition; reduced motion removes the transition. Layout/hitboxes do not move during hover. Server text/voice channels use 30-point rows with three-point spacing; server member rows use 30-point avatars, four-point padding and tighter gaps. Server search and its redundant Conversations heading are removed; the channel viewport starts below the server header and uses the recovered height. DM conversation search remains.

Both composer pickers anchor above the selected button, track its position on resize/zoom, and close on selection, outside click, Escape, ×, a repeated button click, settings or leaving the conversation. GIF search still requires a connected account; offline renders validate presentation only.

Shift-hover edit is restricted to one's own messages. Delete respects ownership/server permissions and opens the existing confirmation. Deleted message snapshots do not expose quick edit/delete. Navigation tooltips were removed from shared settings rows.

Message timestamps use local Windows timezone/DST conversion. Today shows HH:MM; an earlier calendar day appends a date. A 256-label cache avoids repeated conversion during every repaint, clears on local day rollover and refreshes within a minute for timezone changes.

The smaller count is shown only in server member panels. Discord's online_count is preferred when supplied for the selected guild. Otherwise the count uses available online, idle and Do Not Disturb statuses; offline and unknown statuses are excluded. Its tooltip explains the fallback's partial scope. This cannot establish a complete live server count when Discord does not provide it.

No sign-in, messages to others, calls, hardware activation or audible playback was used during validation. Live Discord compatibility, catalog/quest availability, audio/video/screenshare and global hotkeys remain unverified. README.md records broader implementation limits.

The same portable Eclipse.exe and ZIP were updated. Existing eclipse-settings.json was preserved byte for byte during publishing, and the ZIP excludes personal preferences. Installed Discord was untouched.
