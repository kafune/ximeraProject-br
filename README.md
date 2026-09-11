# Traduz

Tradutor local de prosa em LaTeX: fórmulas, comandos, comentários e ambientes protegidos entram como tokens reversíveis. O banco é SQLite e a exportação cria uma árvore paralela.

```sh
cargo run --bin import -- --db data/traduz.sqlite3 --course "Meu curso" --source /caminho/roteiro
cargo run -- --db data/traduz.sqlite3
cargo run --bin export -- --db data/traduz.sqlite3 --source /caminho/roteiro --output /caminho/roteiro-traduzido
```

Para reimportar um arquivo que mudou, revise o relatório e use `--replace`. Para exigir que tudo esteja concluído na exportação, acrescente `--require-complete`.
