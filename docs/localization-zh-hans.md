# Simplified Chinese catalog

The `zh-hans` catalog is registered in PhotoCraft's Rust i18n implementation and
covers the current menu labels, `tl!` literals, blend modes and preference labels.
The original catalog came from the English keys in
[PhotoCraft PR #169](https://github.com/storytold/photocraft/pull/169). Additional
translations credit [PR #359](https://github.com/storytold/photocraft/pull/359)
and current-source additions in [ATTRIBUTION.md](../ATTRIBUTION.md).
The Chinese wording uses ordinary
image-editing terminology; no proprietary translation resources were extracted
or copied. Contributions use the repository's MIT OR Apache-2.0 license.

## Live language switching

Select **Preferences > Interface > Language > 简体中文** to preview Chinese labels.
Apply or OK commits the setting immediately, without restarting PhotoCraft;
Cancel discards the preview. The existing preference store preserves the setting.
See [UI localisation](localization.md) for the supported languages and switching tests.

## Files and integration

- `crates/ui-egui/src/i18n/zh-hans.tsv` contains the translations, independent of
  the lookup implementation. Its UTF-8 columns are `context<TAB>source<TAB>translation`.
- `zh-hans` is registered in `crates/ui-egui/src/i18n/mod.rs`, displayed as
  `简体中文`, with one plural form and enforced complete-catalog checks.
- Select **Preferences > Interface > Language > 简体中文**, or set
  `interface.language` to `zh-hans` through the existing `prefs.set` command.

Keep English source keys, contexts, command IDs, placeholders and escapes intact.
Retain the trailing `…` on commands that open a dialog. Missing translations use
the framework's English fallback. User-supplied names and document data are not
translated. Product names and technology names such as PhotoCraft, ArtCraft,
OpenType, RGB, CMYK and Lab retain their spelling.

The locale resolver recognizes `zh`, `zh-CN`, `zh-SG` and `zh-Hans`
variants. Traditional Chinese locales (`zh-TW`, `zh-HK`, `zh-MO`, `zh-Hant`) do
not select this catalog. Native automatic detection follows the system locale;
the web build currently defaults to English until a language is selected.

## Terminology

| English | Simplified Chinese |
| --- | --- |
| Layer / Layer Comp | 图层 / 图层复合 |
| Mask / Clipping Mask | 蒙版 / 剪贴蒙版 |
| Selection / Feather | 选区 / 羽化 |
| Blend Mode / Opacity | 混合模式 / 不透明度 |
| Adjustment Layer | 调整图层 |
| Smart Object / Smart Filter | 智能对象 / 智能滤镜 |
| Canvas / Artboard | 画布 / 画板 |
| Brush / Stroke | 画笔 / 描边 |
| Fill / Gradient | 填充 / 渐变 |
| Path / Rasterize | 路径 / 栅格化 |
| Preset / Swatch | 预设 / 色板 |
| Export / Preferences | 导出 / 首选项 |

## Validation and maintenance

Run `cargo test -p photocraft-ui-egui`, the touched-crate all-target clippy check,
`cargo xtask layers` and `cargo xtask wasm`. The shared catalog tests validate
duplicate keys, placeholders, ellipses, menu coverage, `tl!` literals and blend
modes. Chinese-specific tests cover locale selection, fallback, plural messages
and formatted labels. Render and inspect the menus, Preferences and representative
dialogs with the existing offscreen `snapshot` example.

Font delivery is separate from translation data; no font assets are added here.
The native lazy loader prioritises Simplified Chinese fonts for `zh-hans` and
resets its cache when the committed language changes. Verify web glyph coverage
separately because that build does not read installed system fonts.

Review new English source meanings before adding translations, retain command
IDs and placeholders, and rerun the coverage and visual checks after catalog edits.
