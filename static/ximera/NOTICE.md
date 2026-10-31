# Componentes de terceiros em `static/ximera/`

Estes arquivos reproduzem a apresentação do Ximera (https://ximera.osu.edu) e
são redistribuídos sob as licenças originais. Nada aqui é código do Traduz,
exceto `reader.css`, `reader.js` e `mathjax-config.js`.

| Arquivo | Origem | Licença |
| --- | --- | --- |
| `base.css` | folha de estilo do servidor Ximera 1.8.1 (`public/stylesheets/base.scss`, https://github.com/XimeraProject/server), compilada com Bootstrap 4.0.0-alpha.6 e Font Awesome 4.7.0; só os caminhos das fontes foram alterados | GPL-2.0 (Ximera), MIT (Bootstrap), MIT (CSS do Font Awesome) |
| `logo.svg` | logotipo do projeto Ximera (`public/images/logo/logo.svg` do mesmo repositório) | GPL-2.0 |
| `fonts/fontawesome-webfont.*` | Font Awesome 4.7.0 | SIL OFL 1.1 |
| `fonts/MathJax_*` | fontes TeX do MathJax 2.7.9 | Apache-2.0 |
| `mathjax/` | MathJax 3 (`tex-chtml-full.js` e fontes CHTML) | Apache-2.0 |
| `vendor/math-expressions.js` | math-expressions 2.0.5-alpha (https://github.com/kisonecat/math-expressions), empacotado com esbuild | GPL-3.0 ou Apache-2.0 |

O script `deploy/build-ximera-assets.sh` documenta como regenerar `base.css` e
`vendor/math-expressions.js`.
