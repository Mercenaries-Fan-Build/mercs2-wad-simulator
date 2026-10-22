# Audio — behaviour contract

Artboard: `Audio.dc.html` ("Audio · vo_Chris") of the Workshop Polish Pass design.
Capture state: `crates/mercs2_workshop/ui_states/audio.json`.

The page is Inspect's AUDIO tab. Every row is an element of the page: what the user does, what happens,
where the data comes from, what it writes, and what proves it. Nothing on the page is drawn without its
row being true.

## Sources

- **Clips**: `workshop_data/audio_manifest.json` (`audioidx::AudioIndex`), one entry per record of the
  `vo_stream.english` wavebank: 12,988 entries, the file name's leading number is the record index.
  The entry's id (`context` hashed, or `asset_id`) is the record's `+0x00` clip hash in 12,935 of
  them.
- **Routing**: the 27 `vo_*.english` soundbanks in `English.wad`. A clip is *played by* a cue when the
  cue's group (single-track) or one of its tracks' sound entries (multi-track) names
  `{wavebank m2("vo_stream"), index = the clip's record}`. Over the retail data: 12,829 clips are played
  by at least one cue; 159 by none; 1,125 by cues of more than one bank.
- **Cue names**: a cue guid is `m2(<cue name>)`; the name is known when the clip's `context` hashes to
  it or the rainbow table / `names.bin` holds it. 1,606 clips are played by exactly one single-track
  cue with a known name; 9,850 by exactly one single-track cue whose name is not known.
- **Samples**: `vostream::VoStream` over `data/Audios/vo_stream.english.pws` (IMA ADPCM decoded to PCM16).

`English.wad` or the `.pws` missing is an error banner on the page naming the file; play, export and
Add to Shipment are unavailable until it is found.

## Elements

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Header crumb | none; `INSPECTING  Audio › <bank>` with a bank open, `Audio › "<query>"` in search mode, `Audio` with neither | `inspect_crumb` | — | test: crumb per mode |
| Tab strip | as Inspect; the AUDIO count is the WAD's audio assets | `AssetIndex::count(Kind::Audio)` | — | capture |
| Search every clip | typing switches the centre to **search mode**: every clip whose bank, speaker, caption or file name contains every term, across all banks, no cap; the table gains a BANK column; clearing the field returns to the open bank | `AudioIndex::search` | — | test: hits across banks, no cap |
| Inventory line | `<N> CLIP(S) ACROSS <B> BANKS`, computed | `AudioIndex` | — | test |
| Bank list | each bank with its live clip count; click opens it in the centre, highlighted. A clip is listed under every bank whose cues play it (Q1) | routing | — | test: counts per bank |
| ADD TO THIS DOMAIN | `add_sound`, `replace_sound_bank`, `replace_sound_cue`; a click adds that kind's stub (scaffolding a Shipment when none is open) and opens it on the Shipment page. With a bank open, `replace_sound_bank` is seeded with that bank, `language: english`, and the category when every group of the bank carries the same one (otherwise empty, which M0216 reports) | `quartermaster::seeded`, routing | `manifest.yaml` | test: seeded fields |
| Centre header | bank name, `<N> clips`; **Filter this bank** narrows the table by caption, speaker or file name | `AudioIndex::by_bank` | — | test |
| Selection bar | `<N> SELECTED · <sum of their lengths> s`; **select all <N>** selects every row the table lists (bank or search hits); **clear** empties the selection | selection set | — | test |
| Table | columns: ☐, ▶, WAVE (record index), SPEAKER, CAPTION (file stem when uncaptioned), LENGTH. ☐ toggles the row in the selection; a row click makes it the current clip (inspector + verbs act on it); the playing row is gold, selected rows grey. Every row is listed (virtualised scroll, no 300-row cap) | `AudioIndex` | — | capture |
| Play · Space / ▶ | plays the current clip from the `.pws`; on the playing clip it pauses, and again resumes where it paused | `audioplay::Player` gains pause / resume / position | — | test: pause holds the position |
| Now-playing card | `CLIP <wave> · PLAYING` (or `PAUSED`), the caption, a progress bar with `<position> / <length> s` from the samples the device has consumed | `Player::position` | — | test + capture |
| Inspector: CLIP card | speaker, bank(s) that play it, the cue(s) that play it (name, or the guid when the name is not known) with their form, source file, stream file | routing, `AudioClip` | — | test |
| Select bank · Ctrl A | as **select all** | selection set | — | test: key binding |
| Add to Shipment ▸ | the current clip's routes: **Replace this line** (`replace_sound_wave`, proposal B), **Replace cue …** one item per cue that plays it, naming how many waves it plays (`replace_sound_cue`: the cue's bank, `language: english`, the group's category, and `cue` = the cue's name, or its guid when the name is not known (proposal A), plus every value the retail cue and group carry: gains in dB, pitch, positional, distances, exponent, Doppler, start limit, sound id, priority, `group_20`, `cue_16`, the record's clip hash), and **Replace its bank** (`replace_sound_bank`, seeded as in ADD TO THIS DOMAIN). The `wave` is left for the author. For a clip no soundbank plays (159), Replace this line is disabled with that reason — replacing it would change nothing, which the build refuses (M0218) | routing, `quartermaster::{seeded, ensure_shipment, upsert_contribution}` | `manifest.yaml` | test: seeded cue equals the retail values |
| Export N clips · F10 | opens the EXPORT card's export; disabled while the chosen scope is empty | as below | — | — |
| EXPORT card | scope: **N selected** · **bank · N** · **search hits · N** (the last only in search mode); format WAV 16-bit PCM; names as shipped (the manifest's file name, e.g. `01595__chris-Banter-Contract-All01-29.wav`); **Choose folder…**; Export runs in the background with the progress window and writes the WAVs plus `captions.csv` (file, wave, bank, speaker, caption, length) | `vostream::clip_pcm`, a WAV writer | `<folder>/<bank or query>/*.wav`, `captions.csv` | test: WAV header + sample count, CSV rows |
| Headless `--audio-export <bank\|query> <dir>` | the same export without a window: an exact bank name exports the bank, anything else the search hits; exit 0, or 2 with the reason (no hits, no `English.wad`, unwritable folder) | shared export fn | as above | test |
| Footer | READY; while a clip plays, `playing <wave> from vo_stream.english.pws` | `Player` | — | capture |

## Decisions

- **Q1 — bank listing: by routing.** A clip is listed under every bank whose cues play it; the 159 no
  cue plays sit in a *not played by any cue* group. (The manifest's `bank` matches the routing for
  3,694 clips and files 9,135 under `vo_misc`, whose soundbank plays 160.)

## Manifest changes (approved)

### A. Sound overrides address banks and cues by name or hash

Every other asset reference takes a name or a bare `0xHHHHHHHH` (`manifest::asset_hash`, M0130
preferring names as a warning), and most shipped audio has no known name: 9,850 clips' only cue,
and 41 of `English.wad`'s 68 soundbank entries.

- In `replace_sound_bank` and `replace_sound_cue`, `bank` and every cue `name` take a name or a bare
  hash, resolved through `manifest::asset_hash` at every site (lint, claims, build, link). A bank
  hash is the entry the game ships (`<bank>.<language>` for a localized bank).
- M0215 keeps the bare-hash refusal for `add_sound` only: its bank and cues are new, and a new
  identity's hash comes from a real name.
- M0218 is the guard: the entry must be in a carrier and the cue in the bank, so a mistyped hash is
  an error, never a silent no-op.
- M0217 and the carrier choice for a hashed bank follow where the entry is found (a language WAD, or
  `vz.wad` / `shell.wad`), not the `vo_` prefix of a name.
- M0130 offers the name when the name table knows the hash (warning).

### B. `replace_sound_wave` — replace one line wherever it plays

A clip is a wave record; cues reach it through groups. 1,431 single-track `vo_*` cues play a group
of 2–14 waves (variants the engine picks between, e.g. `vo_Fiona` `0xB5E91E91`, records 3844–3857),
and 1,367 clips are played by more than one cue. `replace_sound_cue` makes the cue play one new wave,
so on such a cue it drops the variants, and the other cues that play the line keep the old one.

```yaml
  - kind: replace_sound_wave
    wavebank: vo_stream        # name or bare hash, as the groups name it
    language: english          # a localized wavebank's copy (vo_stream.english)
    line: 0x96FFDF9F           # the record's clip hash; a name when known
    wave: src/new_line.wav
```

The build finds the record whose clip hash is `line` (none or several is an error), places the new
wave in the Shipment's override wavebank, and in every soundbank of that carrier rewrites each group
wave reference to the record to point at it, keeping the reference's weight and every other wave of
the group; each cue reaching a rewritten group gets its length recomputed. Two Shipments replacing
one line conflict; the link merges replaced lines with the other sound overrides per bank.

The record itself cannot change in place: a wavebank streams or embeds by its own `+0x0A` kind, not
per record (`FUN_0082e8b0` returns that byte; `FUN_0082e8f0` gives the `.pws` name only when it is
set), so a `vo_stream` record cannot become embedded PCM. Which banks play a line is read from the
game, so the load plan of a set with a `replace_sound_wave` needs the game folder.

With A and B, **Add to Shipment ▸** for a clip offers **Replace this line** (`replace_sound_wave`,
seeded with `vo_stream`, `english` and the clip hash) and **Replace cue …** per cue that plays it
(`replace_sound_cue`, seeded with the cue's bank, guid or name, and its retail values; the item names
how many waves the cue plays).
