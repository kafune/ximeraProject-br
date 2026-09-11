# Plano curto — visualização no estilo Ximera

Objetivo: fazer `GET /read/files/:id` parecer uma página de atividade do Ximera, sem alterar segmentos, traduções ou o importador.

Referências:

- Página-modelo: https://ximera.osu.edu/mooculus/calculus1/ximeraTutorial/howToUseXimera
- Comparação enviada pelo usuário: https://imgur.com/a/gm6hNFF
- CSS original disponível em `https://ximera.osu.edu/public/v1.8.1/stylesheets/base.css`.

## Implementação

1. Na rota/leiaute do leitor, carregar a lista ordenada de arquivos do curso e identificar a atividade atual. Renderizar a faixa horizontal do Ximera com os cartões reais: título, resumo opcional e estado ativo. Não usar três cartões artificiais.
2. Usar a estrutura visual do Ximera: barra superior, faixa de atividades, breadcrumb `mooculus / Calculus 1 / seção`, `container-fluid > row > col-md-10 > main.activity > .activity-body`, navegação Anterior/Próxima e rodapé escuro.
3. Remover do leitor elementos que não existem na atividade original: o título técnico inserido pelo Traduz, o caminho `*.tex` e o cartão centralizado. O conteúdo deve começar na coluna larga do `col-md-10`, alinhado à esquerda, sem `max-width` artificial.
4. Manter somente blocos com status `traduzido`; preservar a renderização segura de texto e fórmulas. MathJax deve continuar renderizando TeX, sem executar HTML vindo da tradução.
5. Reutilizar o CSS do Ximera somente para a página de leitura. Isolar overrides com `.reader-page` para não mudar a tela de tradução.
6. Fazer os links dos cartões e botões Anterior/Próxima apontarem para os arquivos vizinhos no Traduz (`/read/files/:id`). O botão para voltar à tradução deve apontar ao segmento/arquivo atual, não apenas à página inicial.

## Critérios de aceite

- Em desktop 2560px, a coluna de conteúdo começa aproximadamente onde começa no Ximera; não fica centralizada numa coluna estreita.
- A barra de atividades ocupa a largura da tela, tem cartões reais do curso e destaca a atividade atual em fundo escuro.
- A página tem a mesma sequência vertical da referência: topo, atividades, breadcrumb, conteúdo, Anterior/Próxima, rodapé.
- A página funciona em celular com a faixa horizontal rolável.
- Nenhuma tradução existente é apagada ou alterada.
- Rodar `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, `cargo build --release`; reiniciar `traduz.service` e conferir uma rota `/read/files/:id` no navegador.
