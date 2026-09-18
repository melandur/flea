# vendor

Third-party data Flea generates code from. Nothing here is imported at runtime: each file is read
by a generator in `tools/`, which writes a library into `ui/js/`. That is the shipped artefact, and
the generated header of each one names the file it came from.

Regenerate rather than editing either side by hand:

```bash
tools/flea-filetypes-gen    # vendor/yazi-icons.toml   -> ui/js/FileTypeColors.js
tools/flea-themes-gen       # vendor/strata-themes.toml -> ui/js/Themes.js
```

| File | From | Licence |
|---|---|---|
| `yazi-icons.toml` | the `[icon]` section of `yazi-config/preset/theme-dark.toml`, [yazi](https://github.com/sxyazi/yazi) | MIT, © 2023 nvim-tree — the table is [nvim-web-devicons'](https://github.com/nvim-tree/nvim-web-devicons) work, which yazi redistributes under its own `LICENSE-ICONS`. `LICENSE-yazi-icons` |
| `strata-themes.toml` | `data/themes/catalog.toml`, [Strata](https://github.com/lgse/strata) | MIT, © 2026 LGSE Ltd. `LICENSE-strata`. Most entries are [Tinted Base16](https://github.com/tinted-theming) palettes Strata imported; its `THIRD_PARTY_LICENSES.md` records those. |
| — | [lucide-static](https://github.com/lucide-icons/lucide) 1.38.0, the geometry `ui/js/Icons.js` is cut from | ISC. `LICENSE-lucide` |

## What crosses over, and what does not

**From yazi, the colours and the keying only.** Its `text` fields are nerd-font codepoints, and
AGENTS.md "The row mark: Shape, not a generated font" rules a font glyph out of this tree, so
`ui/js/FileTypes.js` answers the mark from Flea's own lucide set instead. Its `conds` rules are
dropped too: they colour a link, an orphan, an executable and a directory, and `ui/Theme.qml`
already takes those from the Omarchy palette's own roles with a WCAG lift on each.

**From Strata, eight of fourteen tokens.** `tools/flea-themes-gen`'s header records the mapping and
why each is the one it is. The four with no Flea role are dropped the same way `applyColors`
already drops an Omarchy theme's extra keys.

**Neither project's code is used**, and no brand logo is reproduced: see AGENTS.md
"The filetype tier" and "The theme catalog".
