# Eclipse 0.8.4 — October 9, 2026

- Fixed a stream getting stuck on "Connecting to stream…" when watched again after stopping: Eclipse now tells Discord it left the stream.
- Friends get the green speaking outline under the voice channel while they talk, not just you.
- 1080p streams keep up too: large pictures are color-converted on four CPU threads (about 2.9 ms per 1080p picture instead of 14.7 ms).
- Faster stream decoding on Windows: video is decoded on the CPU instead of waiting on a per-picture GPU round trip, with a quicker color conversion and one less copy per picture, so 720p60 streams can keep up.
- Fixed watched streams showing only about 2 pictures a second: a late frame tore down the video decoder, and rebuilding it made every following frame late too, so only keyframes were ever shown.
- Smoother video on real connections: lost stream and camera packets are now requested again from Discord (NACK) and briefly waited for, instead of freezing the picture until the next keyframe. Constant keyframe requests were also what made streams look blocky.
- Sharper, smoother screen shares: watched streams are shown at their own resolution (up to 1080p) instead of being shrunk to 640x360 on the CPU for every frame.
- Enlarge a stream you're watching: hover it for Enlarge (hides the participant strip) and Full screen (double-click also works; Esc leaves).
- Picture in picture: clicking a text channel while watching keeps the stream in a small movable window in the corner; click it to return to the stream.
- The Watch Stream buttons are smaller rounded pills with a screen icon.
- Mentions read as names: <@id> shows as @name, role mentions as @role and channel links as #channel, in messages and reply previews.
- Typing @ in the message box lists who you can mention (members, plus @everyone and @here where you're allowed), filtered as you type. Arrow keys or the mouse choose, Tab or Enter inserts; the name is sent as a real mention.
- Reply puts the cursor straight into the message box.
- Watch screen shares like Discord: a live friend's call tile shows LIVE and a Watch Stream button, including friends who were already live when you joined. Hovering a live friend under a voice channel also offers Watch Stream (joining the channel first if needed). The watched stream fills the stage with Stop Watching and a volume control, and everyone else sits in a strip below.
- Sharing your screen opens a Discord-style Screen Share dialog with Applications and Screens tabs, resolution and frame-rate choices, sound sharing and Go Live, instead of a separate window.
- Shift-hover quick delete removes the message at once, without a confirmation (the right-click Delete still asks).
- Editing a message happens in place, like Discord: the message turns into an outlined, already-focused edit box with "escape to cancel • enter to save" under it. The separate edit popup is gone.
- Update prompt at launch: when GitHub has a newer Eclipse release, Eclipse asks whether to update. Yes downloads it, checks it against the release checksum, installs it and restarts; No keeps a green update button in the top right to update later.
- Links in messages and embeds are clickable and shown in link blue (right-click to copy).
- Link previews are one Discord-style card: colored side bar, site name, linked title, description and fields, with the thumbnail inside the card instead of a large separate picture. Click a preview's picture to open it in the larger view.
- Click a picture in chat to open it like Discord's viewer: shown at its own size (shrunk only to fit) over the dimmed app, with the sender and time at the top left and zoom, open in browser, copy link and close at the top right. Esc or a click outside closes it.
- Save pictures from the viewer's download button or the right-click menu (Save image…); Eclipse asks where to save and confirms when it's done.
- The call screen now shows everyone in the voice channel, including people who were already there when you joined.
- With push to talk on, your green speaking outline follows your push-to-talk key instead of microphone activity.

# Eclipse 0.8.3 — October 9, 2026

- Scrolls move twice as fast. Consecutive messages from the same author share an avatar and heading, with each message retaining its actions.
- Direct messages sort by newest message, update with incoming/outgoing activity, and show 25 conversations initially with further batches revealed when scrolling down.
- Removed the Enter/Shift+Enter instruction below the composer; the keyboard shortcuts still work.

# Eclipse 0.8.2 — October 9, 2026

- The selected server's icon, nameplates, avatar decorations and display-name effects keep animating; other avatars, icons and artwork still animate on hover.
- Fixed Shop and Quests constantly reloading server icons and avatars: off-screen cards no longer request artwork, and the image cache evicts icons and avatars last.
- The Direct messages header is now a centered conversation search; the Eclipse icon and the separate search box under Quests are gone, moving the DM list up.
- The conversation search at the top reads Find A Conversation, clears while you type and returns when left empty.
- Shop cards stay still until hovered, and the image cache never evicts on-screen images (keeping a new animation still instead), fixing decorations, nameplates and Shop/Quests art reloading constantly.
- New sign-in on one screen: email/phone and password (with authenticator, backup-code and SMS 2FA) on the left and a QR code for the Discord mobile app on the right. Token entry is removed.
- Stay signed in: the session is saved in Windows Credential Manager and resumed at launch; Log out forgets it.
- Clicking your avatar or name in the bottom-left bar opens a Discord-style account menu: profile banner and bio, Edit Profile, status (Online, Idle, Do Not Disturb, Invisible), Switch Accounts (log out) and Copy User ID.

# Eclipse 0.8.1 â€” October 9, 2026

- Bottom-left account bar spans the server rail and channel list: nameplate background (theme colour without one), avatar, styled name and status, borderless activity/mute/deafen/settings controls with input and output device menus. Removed the Live and Push to Talk labels.
- Conversation header: one "Search <server>" box filters messages and members; pins and member list are small icons; refresh button removed. The member list no longer has its own search or online count.
- Pinned messages open as an anchored dropdown. Drag its bottom edge to resize (saved); click a pin to jump to it, loading surrounding history for older pins with Jump to present to return.
- Right-click â†’ Profile (or a popout avatar) opens a full profile with bio, member-since dates, roles, connections and mutual servers/friends. Profiles use theme colours as gradients with the banner fading in. Clicking the same person again closes their popout.
- Chat drops the YOU tag and presence dots on avatars.
- Friends is a compact list with round message/profile icons and an Active Now column of friends' current activities.
- Ctrl+V in the message bar attaches copied files and images (screenshots, browser images).
- Voice channels list everyone connected from any Discord client, with mute, deafen, camera and LIVE indicators.
- Push to talk gates an always-open microphone for instant start/stop, with press and release tones.
- Tighter spacing in the Direct messages navigation.
- GitHub Actions builds and tests every push on Windows; version tags publish Eclipse.exe as a release.

# Eclipse 0.8 â€” October 8, 2026

- Animated avatars, server icons, emoji, banners, profile effects, shop artwork and name effects play only while hovered and return to their first frame afterward. GIFs in chat and in the GIF picker still play automatically.
- All animations, GIFs included, pause on their current frame while the Eclipse window is not focused and continue when it regains focus.
- Emoji and GIF pickers attach above their composer buttons, follow resizing/zooming and dismiss on outside click, Escape, Ã— or toggling the button.
- Ctrl+wheel zoom with a saved 75â€“150% scale; Ctrl+0 resets. Side panels adapt at higher zoom.
- Less padding around the chat, composer and settings; tighter message spacing.
- Tighter 30-point server text/voice channel rows and compact server member rows.
- Removed server channel search and its empty header space; channel lists move up and use the freed height. DM conversation search stays.
- Local-time message timestamps include a date for earlier calendar days; today remains time-only.
- Smaller 44-pixel server icons, tighter rail spacing and gentle hover enlargement without layout movement.
- Selected DM moon switches to a black crescent on grey.
- Removed repetitive settings navigation tooltips.
- Smaller server-only online member count, using reported server totals or known online/idle/DND statuses. No count in DMs/group chats.
- Shift-hover message actions provide quick edit and permitted delete, retaining delete confirmation.
- Compact mode (Appearance or Chat): 85% interface scale, with panels joined into one connected surface and thin dividers instead of gaps and rounded cards. Buttons stay rounded, message avatars stay visible and message spacing is tighter. Replaces the earlier avatar-free compact messages.
- Updates the same Eclipse.exe and ZIP and preserves local preferences.

# Eclipse 0.7 â€” October 8, 2026

- Enter sends and Shift+Enter inserts a line break; preserves focus, draft safety and IME composition.
- Removed grey message bubbles in server chats, group chats and DMs.
- Matching user/server settings panels, outside-click/Escape/Ã— dismissal, grouped sidebars, content cards and readable settings labels. Technical IDs stay in optional developer details.
- Performance presets, language choices, friendly processing names and AFK durations.
- Server icon/name opens its anchored menu; wheel scrolling without a visible server scrollbar.
- Push-to-talk supports Ctrl, Shift and Alt alone, modifier-only chords and key/mouse combinations, saving the fullest chord on release.
- Original join/leave sounds on real voice connection transitions, duplicate suppression and a shared sound preference.
- Bounded session caches restore recent chats and server channels while refreshing; Gateway changes invalidate stale entries. Chat reads precede optional metadata, per-route limits stay separate, and icons have a dedicated loader.
- Product artwork resolves decoration hashes, profile-effect IDs and nameplates; quest images resolve matching IDs and CDN paths.
- Updates the same Eclipse.exe and ZIP; existing preferences and installed Discord are untouched.

# Eclipse 0.6 â€” October 8, 2026

- Renamed the client and project from Feather to Eclipse, with an original black/orange/red eclipse icon embedded in the Windows EXE and used for the window/taskbar. Existing preferences carry over.

- New Material Black default inspired by MaterialDiscord: black/grey surfaces and accents, rounded message bubbles, pill controls and a rounded composer; preserves other preferences during migration.

- Display/DPI-sized media decoding, sharper avatar/banner CDN requests, a bounded 48 MiB cache and 60 FPS playback/name-animation cap.
- Preserves animation cadence by reducing spatial resolution before sampling long cycles; smooth cyclic Prism gradients and native Gummy motion.
- Chat avatars each open an anchored profile, including repeated messages from the same author. Status badges render above decorations/effects.
- Read All uses a clear text label. Compact upload/emoji/GIF icons are integrated into the composer.
- Name-based DM search is an anchored dropdown. DM context menus show recipient actions directly.
- Rendered inline image/GIF links and attachment hyperlinks are hidden; open/copy actions remain on the media context menu.
- Official Discord artwork for Nitro, Shop and Quests, plus account-provided collection banners.
- Four distinct original mute/unmute/deafen/undeafen feedback sounds, with a settings toggle.
- Updates the existing portable folder and ZIP; installed Discord remains untouched.

# Eclipse 0.5 â€” October 8, 2026

- Anchored server/user profile popouts with outside-click and Escape dismissal, replacing movable profile windows.
- Discord-supplied badges, server/global banners, profile colors, avatar decorations, server tags, nameplates and thirteen licensed display-name fonts; bounded animated artwork and supported profile-effect previews.
- Cleaner typography, grouped settings categories and one Account & Profile page.
- Name-based DM picker using friends, DM recipients and loaded members; full-width result selection.
- Red inline deleted messages and edit history; removed the separate message-log counter/window.
- Read All moved directly beneath the moon icon.
- Responsive Shop/Quests card grids, featured banners and category/status tabs.
- Own profile beneath the active voice channel, actual transport speaking indicators, and a persistent voice connection card.
- Updates the single current portable folder. Installed Discord is untouched.

README documents native approximations, unavailable name-font families and live-service limitations. VALIDATION records the checks performed.
