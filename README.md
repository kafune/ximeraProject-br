# Traduz

Tradutor local de prosa em LaTeX: fórmulas, comandos, comentários e ambientes protegidos entram como tokens reversíveis. O banco é SQLite e a exportação cria uma árvore paralela.

```sh
cargo run --bin import -- --db data/traduz.sqlite3 --course "Meu curso" --source /caminho/roteiro
cargo run -- --db data/traduz.sqlite3
cargo run --bin export -- --db data/traduz.sqlite3 --source /caminho/roteiro --output /caminho/roteiro-traduzido
```

Para reimportar arquivos que mudaram, use `--replace`: trechos idênticos mantêm a tradução, e o que não puder ser reaproveitado é listado e fica no backup criado antes da alteração. Rascunhos não entram na exportação; para exigir que tudo esteja concluído, acrescente `--require-complete`.
