# lettering-preview

Live preview of the main-menu stencil letters in `crates/mercs2_quartermaster/data/lettering/` (one SVG per
letter, `U+XXXX.svg`, cap height 1000 units). Saving a letter reloads the page.

```sh
cd tools/lettering-preview
npm install
npm run dev
```

The page shows the text you type set in both menu states (dark fill with a light ring, bright fill with a
dark ring), the same at the menu's real size (27 px cap) magnified, and every letter.

Retail references (optional, local only): cut the player's own labels and letters into `public/retail/`,
which git ignores:

```sh
mercs2_probe label-crops "<game>/data/shell.wad" shell_MAINMENU_text tools/lettering-preview/public/retail \
  CONTINUE "NEW GAME" "JOIN GAME" OPTIONS EXTRAS
```

A typed text that is one of those labels shows retail's art under ours; each letter shows retail's beside it.
