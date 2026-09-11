# Plano de implementação — Traduz

## Objetivo

Criar uma aplicação local ou auto-hospedada para traduzir roteiros em LaTeX com segurança. A pessoa usuária edita somente trechos de prosa. Matemática, comandos, referências, imagens e ambientes LaTeX são armazenados como placeholders reversíveis e voltam ao arquivo no momento da exportação.

O resultado é um binário Rust que serve HTML, CSS e JavaScript mínimo, persiste o trabalho em SQLite e gera uma árvore .tex traduzida pronta para Git. Não há Node.js, npm, bundler, banco remoto ou processos extras no deploy.

## Tecnologia

| Área | Escolha | Finalidade |
| --- | --- | --- |
| Backend | Rust, tokio e axum | HTTP e um único servidor |
| HTML | askama | Templates compilados e escape automático |
| Banco | rusqlite + SQLite | Banco em arquivo e backup simples |
| Interação | htmx | Atualização parcial sem SPA |
| Serialização | serde e serde_json | Dados tipados e placeholders |
| Linha de comando | clap | Importar, servir e exportar |
| Estilo | CSS próprio mobile-first | Interface leve e responsiva |

## Estrutura de arquivos

    traduz/
    ├── Cargo.toml
    ├── PLANO.md
    ├── README.md
    ├── migrations/001_inicial.sql
    ├── src/
    │   ├── lib.rs
    │   ├── main.rs
    │   ├── bin/import.rs
    │   ├── bin/export.rs
    │   ├── db.rs
    │   ├── models.rs
    │   ├── parser/{mod.rs,latex.rs,segmenter.rs}
    │   ├── routes/{mod.rs,translator.rs,chapters.rs,glossary.rs}
    │   └── templates/{base.html,translator.html,chapters.html,sidebar.html,glossary.html}
    ├── static/{app.css,app.js}
    └── tests/{parser_integration.rs,export_integration.rs,fixtures/}

A biblioteca em src/lib.rs concentra domínio, modelos, banco, parser e exportação. O servidor e os dois comandos usam a mesma implementação.

## Banco de dados

    CREATE TABLE courses (
      id INTEGER PRIMARY KEY,
      nome TEXT NOT NULL
    );

    CREATE TABLE chapters (
      id INTEGER PRIMARY KEY,
      course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
      nome TEXT NOT NULL,
      ordem INTEGER NOT NULL,
      UNIQUE(course_id, ordem)
    );

    CREATE TABLE files (
      id INTEGER PRIMARY KEY,
      chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
      caminho_tex TEXT NOT NULL UNIQUE,
      ordem INTEGER NOT NULL,
      fonte_hash TEXT NOT NULL,
      UNIQUE(chapter_id, ordem)
    );

    CREATE TABLE segments (
      id INTEGER PRIMARY KEY,
      file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
      ordem INTEGER NOT NULL,
      texto_original TEXT NOT NULL,
      texto_traduzido TEXT NOT NULL DEFAULT '',
      status TEXT NOT NULL DEFAULT 'pendente'
        CHECK(status IN ('pendente', 'em_progresso', 'traduzido')),
      placeholders_json TEXT NOT NULL DEFAULT '[]',
      UNIQUE(file_id, ordem)
    );

    CREATE TABLE glossary (
      termo_original TEXT PRIMARY KEY COLLATE NOCASE,
      termo_traduzido TEXT NOT NULL
    );

Criar índices para a ordenação dos capítulos, arquivos e segmentos. Um segmento vazio é pendente; um texto salvo pelo autosave é em_progresso; Salvar e Próximo muda-o para traduzido. Apagar a tradução retorna o estado a pendente.

Exemplo de placeholders_json:

    [
      {"token":"{{MATH_1}}","original":"$x^2 + y^2$"},
      {"token":"{{CMD_1}}","original":"\\emph{important}"}
    ]

O campo editável recebe: The {{CMD_1}} result follows from {{MATH_1}}. Tokens recebem destaque visual, mas ficam literais no valor salvo para a exportação ser determinística.

## Fase 1 — parser e importação

### Entrada e descoberta

    cargo run --bin import -- \
      --db data/traduz.sqlite3 \
      --course "Nome do curso" \
      --source /caminho/para/roteiro

O importador encontra arquivos .tex recursivamente, guarda caminhos relativos, calcula SHA-256 da fonte e importa um arquivo por transação. A estrutura de diretórios determina capítulo e ordem no início. Se a árvore não refletir a sequência pedagógica, acrescentar depois um manifesto explícito.

Antes da importação geral, os arquivos reais chainRule e understandingFunctions são usados como validação inicial.

### Scanner seguro

LaTeX não será tratado com uma regex ampla. O scanner percorre caracteres, mantendo estados para texto normal, comentário, comando, argumento balanceado, matemática inline, matemática display, ambiente e conteúdo literal.

Representação interna:

    enum Piece {
        Prose(String),
        Protected { kind: PlaceholderKind, original: String },
    }

Proteger sem modificação:

- matemática $...$, \( ... \), $$...$$ e \[ ... \];
- ambientes equation, align, gather e suas variantes;
- comandos e argumentos aninhados, como \emph{...}, \ref{...}, \cite{...} e \includegraphics[...];
- comentários, inclusões, preâmbulo, macros e referências;
- ambientes estruturais ou literais: tikzpicture, figure, table, tabular, verbatim, lstlisting e minted.

A lista de ambientes é configurável. Um ambiente desconhecido deve gerar diagnóstico, não ser assumido como prosa traduzível.

### Segmentação

Após o scanner, agrupar peças na ordem original. Separar prosa primeiro por parágrafo; se necessário, por limite de frase, para manter segmentos perto de 800–1.200 caracteres. Não criar segmentos vazios ou formados apenas de tokens. Arquivos sem prosa ainda entram no banco e são copiados intactos.

### Garantia de segurança

Antes de gravar cada arquivo:

1. Reconstruir os segmentos injetando placeholders.
2. Comparar a reconstrução byte a byte com a fonte.
3. Falhar a importação daquele arquivo se houver diferença.
4. Informar linha, coluna e contexto de delimitadores não fechados.
5. Rejeitar placeholders ausentes ou duplicados.

Sem tradução, a sequência parser → placeholders → reconstrução deve reproduzir exatamente a entrada. Esta é a garantia central do produto.

### Testes

Fixtures devem cobrir fórmula inline e display, cifrão escapado, comentários, argumentos aninhados, comandos em fim de linha, align, TikZ, verbatim/listings, Unicode, arquivos sem prosa e delimitadores inválidos.

ChainRule e understandingFunctions devem passar por testes de integração, ou por uma raiz externa indicada por variável de ambiente caso não possam ser versionados. A fase só é aceita quando ambos fazem ida e volta idêntica, preservam construções e geram segmentos legíveis e ordenados.

## Fase 2 — MVP móvel

### Rotas

| Rota | Finalidade |
| --- | --- |
| GET / | Abre último segmento ou primeiro pendente |
| GET /chapters | Lista capítulos e arquivos |
| GET /segments/:id | Tela de tradução |
| PUT /segments/:id/draft | Autosave de rascunho |
| POST /segments/:id/complete | Salva, conclui e abre o próximo |
| POST /segments/:id/skip | Avança sem alterar o atual |
| GET /segments/:id/previous | Abre o anterior |
| GET /segments/:id/next | Abre o próximo |

### Interface

A referência é 360 px: coluna única e um segmento por tela.

    ┌──────────────────────────────────┐
    │ Capítulo 12/36 · arq. 3/4 · 8/20 │  progresso fixo
    ├──────────────────────────────────┤
    │ Original — leitura confortável    │
    │ termos do glossário destacados    │
    ├──────────────────────────────────┤
    │ Sua tradução                      │
    │ [ textarea ampla ]                │
    ├──────────────────────────────────┤
    │ [ Pular ] [ Salvar e Próximo     ]│  ações fixas
    └──────────────────────────────────┘

Requisitos: tema escuro por padrão, fonte de leitura de pelo menos 18 px, alvos de toque de no mínimo 44 px, respeito a área segura, nenhuma rolagem horizontal, nenhuma ação essencial em modal ou menu escondido. A lista de capítulos é uma tela própria.

### Autosave

O JavaScript fica limitado a:

1. Esperar 1,5 segundo depois da última entrada.
2. Enviar PUT para salvar o rascunho.
3. Salvar imediatamente quando o campo perde foco.
4. Mostrar Salvando, Salvo ou erro recuperável.
5. Cancelar requisições obsoletas para impedir respostas fora de ordem.

O servidor valida que cada placeholder original continua presente exatamente uma vez. Sem JavaScript, a página ainda funciona pelo botão principal.

## Fase 3 — desktop, atalhos e glossário

Em aproximadamente 900 px ou mais, a mesma rota ganha sidebar fixa com árvore de capítulos e arquivos; original e tradução aparecem em duas colunas. Não existe uma aplicação separada.

| Atalho | Ação |
| --- | --- |
| Ctrl+Enter | Salvar, concluir e avançar |
| Ctrl+S | Salvar rascunho |
| Ctrl+seta esquerda | Segmento anterior |
| Ctrl+seta direita | Próximo segmento |

O glossário é um CRUD simples. Ao renderizar o original, a busca ignora maiúsculas, prioriza o termo mais longo e evita sobreposição. A tradução preferida aparece como dica acessível, sem modificar o texto salvo.

## Fase 4 — exportação

    cargo run --bin export -- \
      --db data/traduz.sqlite3 \
      --source /caminho/para/roteiro \
      --output /caminho/para/roteiro-traduzido

O exportador:

1. Cria árvore de saída paralela, sem escrever na fonte.
2. Copia imagens, estilos, bibliografia e arquivos auxiliares.
3. Reconstrói cada arquivo .tex na ordem registrada.
4. Reinjeta cada placeholder pelo original.
5. Usa o texto original para pendências e avisa; a opção require-complete torna pendência em erro.
6. Rejeita tokens ausentes, duplicados ou alterados.
7. Emite relatório de arquivos, segmentos concluídos, pendentes e erros.

Uma exportação sem traduções é byte a byte igual à fonte. A validação opcional com latexmk pode existir, mas não é requisito.

## Reimportação, backup e deploy

Cada arquivo possui fonte_hash. Ao reimportar, uma mudança gera relatório e exige confirmação antes de substituir segmentos. Reaproveitar traduções somente quando o pareamento for inequívoco; nunca apagar tradução silenciosamente. Criar backup do SQLite antes de alterações estruturais.

O servidor escuta em 127.0.0.1 por padrão. Tailscale é indicado para acesso remoto pessoal. Exposição pública exige HTTPS e autenticação. Backup é uma cópia consistente do SQLite e, se desejado, a árvore exportada no fork do curso.

## Qualidade

- SQL parametrizado e texto escapado em HTML.
- Migrações versionadas e transacionais.
- Testes unitários de scanner e segmentação.
- Integração para SQLite, rotas e exportação.
- Verificações: cargo fmt --check, cargo clippy -- -D warnings e cargo test.
- Testes manuais em celular estreito, teclado desktop, rede lenta e recuperação de rascunho.

## Ordem de execução

1. Criar projeto Cargo, migration inicial, configuração e testes mínimos.
2. Localizar a árvore LaTeX e validar chainRule e understandingFunctions.
3. Implementar scanner, reconstrução exata e diagnósticos.
4. Implementar segmentação e persistência SQLite.
5. Construir tela móvel: progresso, autosave, salvar/avançar e pular.
6. Adicionar layout desktop, atalhos e glossário.
7. Implementar exportação, relatório e modo estrito.
8. Adicionar reimportação segura e endurecimento de deploy conforme necessário.

O maior risco é a compatibilidade com o dialeto LaTeX efetivamente usado pelo curso. A reconstrução exata dos dois arquivos reais é, portanto, o primeiro critério de aceite, antes de investir nas telas.

