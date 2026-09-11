# Traduz

Tradutor local de prosa em LaTeX: fórmulas, comandos, comentários e ambientes protegidos entram como tokens reversíveis. O banco é SQLite e a exportação cria uma árvore paralela.

```sh
cargo run --bin import -- --db data/traduz.sqlite3 --course "Meu curso" --source /caminho/roteiro
cargo run -- --db data/traduz.sqlite3
cargo run --bin export -- --db data/traduz.sqlite3 --source /caminho/roteiro --output /caminho/roteiro-traduzido
```

Para reimportar arquivos que mudaram, use `--replace`: trechos idênticos mantêm a tradução, e o que não puder ser reaproveitado é listado e fica no backup criado antes da alteração. Rascunhos não entram na exportação; para exigir que tudo esteja concluído, acrescente `--require-complete`.

## Deploy

Na VPS, dentro do clone, rode `deploy/redeploy.sh`. O script lê o binário, `--db` e `--listen` do `traduz.service` (do sistema ou de `systemctl --user`), avança para `origin/main`, roda os testes, compila em release, guarda banco e binário atuais em `data/backups/` e reinicia o serviço. Se o serviço não responder, volta ao commit e ao binário anteriores. As variáveis opcionais estão no topo do script.
