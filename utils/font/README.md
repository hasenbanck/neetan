# Open Source Font ROM

This font was created using the [Shinonome Gothic](https://github.com/code4fukui/shinonome-font) bitmap font (public
domain), which provides 8x16 and 16x16 glyphs for JIS X 0201 and JIS X 0208 character sets with some additional
modifications to align them with the original NEC fonts.

NEC-specific characters use glyph data from NP21W and are located under `utils/font/patches` (BSD 3-Clause).

## Generate yourself

If you want to re-generate the font ROM, run the following command from the root folder of this project.

```sh
cargo run --release -p create_font -- -o utils/font/font.rom
```
