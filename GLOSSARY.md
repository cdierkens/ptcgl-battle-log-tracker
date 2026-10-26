# PTCGL Tracker

A desktop companion to Pokémon TCG Live (PTCG Live). It notices when a match ends
and helps the player keep that match's battle log, without ever playing the game
for them.

## Language

### The game

**PTCG Live**:
Pokémon TCG Live, the official client the app watches. The app is never part of it.
_Avoid_: PTCGL (except in names), the client, TCGL

**Battle log**:
The per-match play-by-play text PTCG Live puts on the clipboard when the player clicks
Export on the results screen.
_Avoid_: replay, match history, game log

**Results screen**:
The screen PTCG Live shows when a match ends. It holds the Battle Log tab and the
Continue button.
_Avoid_: end screen, post-game screen

**Player name**:
The player's in-game name, exactly as PTCG Live shows it. It tells the player's side of a
battle log from the opponent's.
_Avoid_: username, account, profile

### Capture

**Capture**:
Keeping a match's battle log after the player has exported it themselves. The app may
read the screen and the clipboard. It never clicks, types, or otherwise acts in the game.
_Avoid_: scraping, auto-export, automation

**Results detection**:
Recognising, by reading the screen, that PTCG Live is showing the results screen.
_Avoid_: trigger, hook

**Game picture**:
The 16:9 area of a PTCG Live capture that the game actually draws, without the title bar
or the black bars macOS adds in fullscreen. Results detection measures its regions from it.
_Avoid_: viewport, canvas, game area

**Warm-up**:
The startup period, up to about a minute after a fresh install or update, while the OS
text recognizer loads. Results detection and clipboard auto-save are off; the capture
drawer says "Starting up…".
_Avoid_: loading, boot

**Capture drawer**:
The app's in-game hub. It slides out from the PTCG Live window while the game is active,
and it holds saving this match's log, past matches, and settings.
_Avoid_: capture tab, widget, popup, overlay (on its own)

**Continue shield**:
The overlay that covers the results screen's Continue button until the player saves a
battle log or skips it.
_Avoid_: blocker, lock

**Clipboard auto-save**:
An opt-in setting. While the results screen is up and the game is focused, it saves a
battle log the player has just exported to the clipboard. Each auto-save can be undone.
_Avoid_: clipboard sync, clipboard listener

**Saved log**:
A battle log written to the log folder. It is the app's record of a match and the
source for everything derived from it.
_Avoid_: capture (as a noun), file, entry

**Log folder**:
The folder that saved logs go into. The player chooses it, and it lives outside the app's
own data.
_Avoid_: save folder (UI label only), output dir, app data

**Match**:
One saved log as the player sees it in the list: result, prizes, and opponent, when the
log can be analysed.
_Avoid_: game (ambiguous with PTCG Live), row

### Decks

**Deck library**:
The player-supplied decklists the app keeps in its own app data. The app never reads a
decklist from PTCG Live.
_Avoid_: collection, inventory, deck box

**Deck version**:
One immutable row in the deck library. Editing a deck adds a version instead of changing
an old one.
_Avoid_: revision, edit, history entry

**Active deck**:
The one deck that new saved logs snapshot. The player can switch it or choose **No deck**.
_Avoid_: current deck, selected deck, default deck

**Deck snapshot**:
The immutable copy of a deck version attached to one saved log. Later edits do not change
it.
_Avoid_: assignment, deck link, match deck (internal only)

**Portable decklist**:
The PTCGL-rendered text of a deck snapshot or deck version. Destinations receive it; the
original pasted text stays local.
_Avoid_: export, list text, deck string

**Archetype**:
A player-confirmed label plus zero to two Pokémon species. The label is what people
read; the species are the identities Training Court renders. Catalog archetypes carry
a stable id; custom ones do not.
_Avoid_: deck type, deck name

**Custom archetype**:
An archetype whose label or Pokémon were edited away from a catalog entry. It has no
catalog id.
_Avoid_: manual archetype, user archetype

**Metadata-only deck**:
A deck version with no decklist, identified by a format and optionally an archetype.
Used when the player wants match identity before pasting a list.
_Avoid_: empty deck, listless deck, header-only deck

**Deck source**:
How a deck version was supplied: a paste, a typed list, or metadata only. It is
provenance, not something parsing depends on.
_Avoid_: origin, input mode

### Platform

**Platform port**:
One OS capability the capture engine needs, such as finding the game window, capturing a
frame, recognising text, or presenting overlays. Each OS supplies its own implementation.
_Avoid_: driver, adapter (on its own), backend

### Appearance

**Theme**:
A complete set of colour and shape tokens that restyles the whole app without changing
its behaviour.
_Avoid_: skin, palette, brand

### Sync

**Sync**:
Sending saved logs to the player's destinations. One way only: nothing comes back from a
destination into the app.
_Avoid_: upload, push, backup

**Destination**:
A site account that saved logs are synced to, set up from a destination config.
_Avoid_: integration, webhook, target, endpoint (on its own)

**Destination config**:
The settings a site publishes for the player to paste into the app once: where to send
logs and how to authenticate.
_Avoid_: integration file, connection string

**Envelope**:
The one request body every destination receives for a saved log.
_Avoid_: payload, body mapping

**Delivery**:
Syncing one saved log to one destination, including its retries.
_Avoid_: upload (as a noun), push

**Needs attention**:
A destination paused because it rejected its token. Its deliveries wait until the player
pastes a new token or retries.
_Avoid_: broken, disconnected, auth error
