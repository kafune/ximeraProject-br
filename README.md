# Traduz

Tradutor local de prosa em LaTeX: fórmulas, comandos, comentários e ambientes protegidos entram como tokens reversíveis. O banco é SQLite e a exportação cria uma árvore paralela.

```sh
cargo run --bin import -- --db data/traduz.sqlite3 --course "Meu curso" --source /caminho/roteiro
cargo run -- --db data/traduz.sqlite3
cargo run --bin export -- --db data/traduz.sqlite3 --source /caminho/roteiro --output /caminho/roteiro-traduzido
```

## Traduções versionadas

A pasta `translations/` guarda traduções em JSON, uma lista por capítulo com `arquivo`, `ordem`, `hash` (SHA-256 da prosa original com os tokens) e `traducao`. O comando `apply` grava essas traduções no banco, só em segmentos ainda pendentes; o que foi feito na tela nunca é sobrescrito sem `--overwrite`. Cada texto passa pela mesma validação de tokens da tela, e uma entrada cujo original mudou é ignorada com aviso.

```sh
cargo run --bin apply -- --db data/traduz.sqlite3 --translations translations/calculus1
```

O arquivo `protegidos.json` do mesmo diretório traduz trechos que o scanner protege e que o tradutor nunca vê: tokens inteiros como `\dfn{critical point}` ou `\begin{theorem}[Rolle's Theorem]`, rótulos de figuras como `{slope of the}` e o conteúdo de `\text{...}` dentro de fórmulas. Essas entradas vão para a tabela `protected_translations` e são aplicadas pela exportação e pelo leitor ao reinjetar segmentos concluídos e ao LaTeX entre segmentos de arquivos já traduzidos; um arquivo sem tradução continua saindo byte a byte igual à fonte.

A tradução completa do curso Calculus 1 (3588 segmentos, 793 arquivos) está em `translations/calculus1/`; o guia de estilo e o glossário usados estão em `translations/calculus1/GUIA.md`.

Para reimportar arquivos que mudaram, use `--replace`: trechos idênticos mantêm a tradução, e o que não puder ser reaproveitado é listado e fica no backup criado antes da alteração. Rascunhos não entram na exportação; para exigir que tudo esteja concluído, acrescente `--require-complete`.

## Deploy

Na VPS, dentro do clone, rode `deploy/redeploy.sh`. O script lê o binário, `--db` e `--listen` do `traduz.service` (do sistema ou de `systemctl --user`), avança para `origin/main`, roda os testes, compila em release, guarda banco e binário atuais em `data/backups/` e reinicia o serviço. Se o serviço não responder, volta ao commit e ao binário anteriores. As variáveis opcionais estão no topo do script.
