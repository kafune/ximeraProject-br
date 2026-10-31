#!/usr/bin/env bash
# Regenera os componentes de terceiros de static/ximera/ (veja o NOTICE.md lá).
#
# Precisa de node/npm e de um clone do servidor Ximera
# (https://github.com/XimeraProject/server, versão 1.8.1), cujo caminho vai
# em XIMERA_SERVER. O resultado já está versionado; rode só para atualizar.
#
#   XIMERA_SERVER=/caminho/server deploy/build-ximera-assets.sh
set -Eeuo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
server="${XIMERA_SERVER:?defina XIMERA_SERVER com o clone do servidor Ximera}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cd "$work"
npm init -y >/dev/null
npm install --no-audit --no-fund sass esbuild bootstrap@4.0.0-alpha.6 font-awesome@4.7.0 \
  mathjax@2.7.9 mathjax3@npm:mathjax@3 math-expressions@2.0.5-alpha >/dev/null

# base.css: a folha do servidor Ximera com Bootstrap 4 alpha e Font Awesome,
# sem os editores (Guppy e CodeMirror) e com as fontes servidas pelo Traduz.
mkdir scss
cp "$server"/public/stylesheets/*.scss scss/
sed -i \
  -e '/guppy-default.min.css/d' -e '/guppy-fixes/d' -e '/codemirror.css/d' \
  -e 's#"./node_modules/bootstrap/scss/bootstrap.scss"#"bootstrap/scss/bootstrap.scss"#' \
  -e 's#"./node_modules/font-awesome/scss/font-awesome.scss"#"font-awesome/scss/font-awesome.scss"#' \
  scss/base.scss
./node_modules/.bin/sass --no-source-map --quiet --load-path=node_modules scss/base.scss base.css
sed -e 's#/node_modules/font-awesome/fonts//#/static/ximera/fonts/#g' \
    -e 's#/node_modules/mathjax/fonts/HTML-CSS/TeX/[a-z]*/#/static/ximera/fonts/#g' \
    base.css > "$repo/static/ximera/base.css"

mkdir -p "$repo/static/ximera/fonts" "$repo/static/ximera/mathjax/fonts" "$repo/static/ximera/vendor"
cp node_modules/font-awesome/fonts/fontawesome-webfont.{woff2,woff,ttf} "$repo/static/ximera/fonts/"
cp node_modules/mathjax/fonts/HTML-CSS/TeX/woff/MathJax_Main-Regular.woff \
   node_modules/mathjax/fonts/HTML-CSS/TeX/woff/MathJax_Math-Italic.woff \
   node_modules/mathjax/fonts/HTML-CSS/TeX/otf/MathJax_Main-Regular.otf \
   node_modules/mathjax/fonts/HTML-CSS/TeX/otf/MathJax_Math-Italic.otf "$repo/static/ximera/fonts/"
cp "$server/public/images/logo/logo.svg" "$repo/static/ximera/logo.svg"

# MathJax 3 servido localmente (a leitura não depende de CDN).
cp node_modules/mathjax3/es5/tex-chtml-full.js "$repo/static/ximera/mathjax/"
rm -rf "$repo/static/ximera/mathjax/fonts/woff-v2"
cp -r node_modules/mathjax3/es5/output/chtml/fonts/woff-v2 "$repo/static/ximera/mathjax/fonts/"

# math-expressions compara a resposta digitada com a esperada (como no Ximera).
cat > entry.js <<'JS'
if (typeof globalThis.process === "undefined") { globalThis.process = { env: {}, browser: true }; }
module.exports = require("math-expressions");
JS
./node_modules/.bin/esbuild entry.js --bundle --minify --platform=browser --format=iife \
  --global-name=MathExpressions --define:global=globalThis \
  --outfile="$repo/static/ximera/vendor/math-expressions.js"
echo "assets atualizados em $repo/static/ximera"
