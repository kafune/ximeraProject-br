# Visualização no estilo Ximera

Objetivo: fazer `GET /read/files/:id` parecer uma página de atividade do Ximera, com as traduções do Traduz, sem alterar segmentos, traduções ou o importador.

Referência: https://ximera.osu.edu/mooculus/calculus1/ximeraTutorial/howToUseXimera

## Como está feito

- O conteúdo é compilado pelo ximeraLatex (`src/ximera.rs`), com os mesmos passos do `xmlatex texHtml`, a partir da fonte traduzida. O fragmento e as figuras ficam em `rendered/` (versionado) e o servidor só os serve. `cargo run --bin render` compila tudo que não está atual.
- Pós-processamento igual ao do luaxake: remove scripts e comentários, injeta as macros do `.xmjax` para o MathJax, corrige `\begin {`, remove parágrafos vazios e aponta as imagens para `/rendered/...`. Os `\answer{...}` dentro das fórmulas viram caixas que o `reader.js` preenche; `\answer[given]` fica visível.
- A faixa de atividades, o título do curso, o capítulo da trilha e Anterior/Próxima vêm do xourse (`src/xourse.rs` lê `\activity`, `\chapterstyle`, `\sectionstyle` e `\part` do `calculus1.tex` guardado no banco).
- O template `reader.html` reproduz o DOM do servidor Ximera 1.8.1 (navbar, `course-navigation card-group scroll`, `breadcrumb`, `container-fluid > row > col-md-10 > main.activity > .activity-body`, `pagination`, `footer.bg-inverse`), com a folha `base.css` compilada do SCSS do servidor (Bootstrap 4 alpha 6) e os rótulos em português em `reader.css`.
- `reader.js` reimplementa a interatividade: dicas com "Revelar dica", múltipla escolha e selecione todas com Verificar/Correto/Tente de novo, escolha de palavra, caixas de resposta (comparação simbólica com math-expressions), resposta livre, comentários liberados após a tentativa, ambientes aninhados liberados após a resposta, progresso no cartão. Tudo fica no localStorage.

## Critérios de aceite

- Em desktop a coluna de conteúdo começa onde começa no Ximera; a faixa de atividades ocupa a largura da tela, com os cartões reais do curso e a atividade atual em fundo escuro.
- A página tem a mesma sequência vertical da referência: topo, atividades, trilha, conteúdo, Anterior/Próxima, rodapé; em celular a faixa rola na horizontal.
- Nenhuma tradução existente é apagada ou alterada.
- `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`; na VPS, `deploy/redeploy.sh` e conferir uma rota `/read/files/:id` no navegador.
