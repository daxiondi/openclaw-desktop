# Bundle Resources

`npm run prepare:openclaw-bundle` will populate this directory with:

- `openclaw-bundle/openclaw.tgz`
- `openclaw-bundle/prefix/`
- `openclaw-bundle/npm-cache/`
- `openclaw-bundle/node/`
- `openclaw-bundle/npm/`
- `openclaw-bundle/manifest.json`

This placeholder keeps Tauri's `bundle.resources` glob valid during local builds
before the offline payload is prepared.
