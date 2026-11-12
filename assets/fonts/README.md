Inter 4.1 (https://github.com/rsms/inter, licença OFL em `LICENSE.txt`), arquivos `extras/ttf/Inter-{Regular,SemiBold,Bold}.ttf`.

A Inter tem glifos próprios na área privada do Unicode (U+E000–U+F8FF), a mesma faixa dos ícones Phosphor.
Esses glifos foram removidos para os ícones aparecerem:

    uvx --from fonttools==4.55.3 pyftsubset Inter-Regular.ttf \
      --unicodes="U+0000-DFFF,U+F900-FFFF,U+10000-10FFFF" --layout-features='*' \
      --glyph-names --name-IDs='*' --notdef-outline --output-file=Inter-Regular.ttf
