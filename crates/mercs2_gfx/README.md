# mercs2_gfx

The player for the game's Scaleform GFx 2.0.48 movies, shared by `mercs2_game` and `mercs2_workshop`.

It runs a movie (timeline, display list, ActionScript 2) and hands back a draw list. It owns no GPU
state: `mercs2_engine` draws the list. The tag stream is decoded by `mercs2_formats::gfx_def`.

Spec: `docs/reverse_engineer/gfx_menu_replay_scoping.md`, `scaleform_gfx_class_map.md`.
