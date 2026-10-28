# mercs2_profd

The profiler's capture service. The profiler ASI (`shipments/mercs2-profiler`) sends it `START <session>`
and `STOP` on its hotkey; for each session it runs one `tracy-capture` against the game's Tracy client and
writes a new `<session>.tracy` to the captures directory. The protocol is `mercs2_formats::profd`.

It runs in Docker with the pinned `tracy-capture` (`tools/tracy/`):

```sh
MERCS2_CAPTURES_DIR=/path/to/captures docker compose -f tools/tracy/compose.yaml up --build
```

Configuration is by flag or environment variable, all required except the timeouts:

| flag | variable | meaning |
|---|---|---|
| `--listen` | `PROFD_LISTEN` | address it listens on |
| `--game-host` | `PROFD_GAME_HOST` | host the game's Tracy client is reached at |
| `--game-port` | `PROFD_GAME_PORT` | port of the game's Tracy client |
| `--captures` | `PROFD_CAPTURES` | directory the `.tracy` files are written to |
| `--tracy-capture` | `PROFD_TRACY_CAPTURE` | the `tracy-capture` executable |
| `--connect-timeout` | `PROFD_CONNECT_TIMEOUT` | seconds a START waits for the game (15) |
| `--save-timeout` | `PROFD_SAVE_TIMEOUT` | seconds a STOP waits for the save (120) |

A session never overwrites an existing capture, and only one runs at a time.
