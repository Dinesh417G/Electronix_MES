# App icons

Bundle icons are generated from a single source PNG and are **not** checked in
(they are build artifacts). Before packaging a release, generate them:

```bash
npm run tauri icon path/to/electronix-logo.png
```

This writes `icon.png`, `icon.ico`, `icon.icns`, and the platform PNG set that
`tauri.conf.json` (`bundle.icon`) references. `npm run build` (the React app)
and `tauri dev` do not require them; only `tauri build` (packaging) does.
