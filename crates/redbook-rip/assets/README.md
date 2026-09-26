# redbook-rip assets

- `icon.svg` - vector source for the app icon. This is the original; edit this file.
- `icon.ico` - multi-resolution Windows icon generated from the SVG.
  Generate it locally after editing the SVG (not committed by default).

## Regenerating `icon.ico`

Using ImageMagick:

```sh
magick icon.svg -define icon:auto-resize=256,128,64,48,32,24,16 icon.ico
```

Using Inkscape + ImageMagick (for precise rasterization):

```sh
for size in 16 24 32 48 64 128 256; do
  inkscape icon.svg -w $size -h $size -o icon-$size.png
done
magick icon-16.png icon-24.png icon-32.png icon-48.png icon-64.png icon-128.png icon-256.png icon.ico
```

The icon is embedded into the `rip` binary on Windows targets by `build.rs`
(via the `winresource` crate). If `icon.ico` is missing the build succeeds
without an icon and emits a cargo warning.
