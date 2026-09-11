#!/usr/bin/env bash
# Redeploy do Traduz na VPS.
#
# Avança o clone até origin/main (só fast-forward), roda os testes, compila em
# release, guarda banco e binário atuais em data/backups/, reinicia o serviço
# systemd e confere se ele responde. Se algo falhar, volta ao commit e ao
# binário anteriores; se o serviço já tinha sido parado, sobe a versão antiga.
#
# Uso (no clone da VPS, com o usuário dono do repositório):
#   deploy/redeploy.sh
#
# O binário, o --db e o --listen vêm do ExecStart do serviço. Opcionais:
#   SERVICE=traduz   REMOTE=origin   BRANCH=main
#   SKIP_TESTS=1     pula o cargo test
#   FORCE=1          recompila e reinicia mesmo sem commits novos
#   KEEP_BACKUPS=10  quantos backups manter em data/backups/
#   HEALTH_URL=URL   padrão: http://<listen>/chapters
set -Eeuo pipefail

SERVICE="${SERVICE:-traduz}"
REMOTE="${REMOTE:-origin}"
BRANCH="${BRANCH:-main}"
SKIP_TESTS="${SKIP_TESTS:-0}"
FORCE="${FORCE:-0}"
KEEP_BACKUPS="${KEEP_BACKUPS:-10}"

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
if [ -d "$HOME/.cargo/bin" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
fi
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  SUDO="sudo"
fi

phase="preparação"
previous=""
backup=""
bin_path=""
argv=""
swapped=0

log() { printf '\n==> %s\n' "$*"; }

die() {
  printf 'erro: %s\n' "$*" >&2
  exit 1
}

wait_healthy() {
  for _ in $(seq 1 30); do
    if systemctl is-active --quiet "$SERVICE" &&
      curl -fsS -o /dev/null --max-time 5 "$HEALTH_URL"; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# Troca por rename para nunca escrever sobre um executável em uso.
install_binary() {
  local source="$1" sudo=""
  if [ "$source" -ef "$bin_path" ]; then
    return 0
  fi
  if [ ! -w "$(dirname "$bin_path")" ]; then
    sudo="$SUDO"
  fi
  $sudo install -m 755 "$source" "$bin_path.new"
  $sudo mv -f "$bin_path.new" "$bin_path"
}

on_error() {
  local status=$?
  if [ $# -gt 0 ]; then
    status="$1"
  fi
  trap - ERR INT TERM
  set +e
  printf '\nerro na etapa "%s" (status %s)\n' "$phase" "$status" >&2
  if [ -n "$previous" ] && [ "$(git rev-parse HEAD)" != "$previous" ]; then
    echo "Voltando o clone para $(git rev-parse --short "$previous")." >&2
    git reset --hard --quiet "$previous"
  fi
  if [ -n "$backup" ] && [ -f "$backup/traduz" ] && ! cmp -s "$backup/traduz" "$bin_path"; then
    echo "Restaurando o binário anterior." >&2
    install_binary "$backup/traduz"
  fi
  if [ "$swapped" = 1 ]; then
    $SUDO systemctl restart "$SERVICE"
    if wait_healthy; then
      echo "A versão anterior está no ar." >&2
    else
      echo "ATENÇÃO: a versão anterior também não respondeu em $HEALTH_URL; veja journalctl -u $SERVICE -n 50" >&2
    fi
    echo "O banco não foi restaurado; a cópia de antes do deploy está em $backup." >&2
  else
    echo "O serviço em execução não foi alterado." >&2
  fi
  exit "$status"
}

for tool in git cargo curl systemctl flock install cmp; do
  command -v "$tool" >/dev/null || die "comando ausente: $tool"
done
exec 9>"${TMPDIR:-/tmp}/traduz-redeploy.lock"
flock -n 9 || die "outro redeploy já está em andamento"

[ "$(systemctl show -p LoadState --value "$SERVICE")" = loaded ] ||
  die "serviço $SERVICE.service não encontrado"
exec_start="$(systemctl show -p ExecStart --value "$SERVICE")"
re='path=([^ ;]+)'
if [[ $exec_start =~ $re ]]; then
  bin_path="${BASH_REMATCH[1]}"
fi
re='argv\[\]=([^;]*)'
if [[ $exec_start =~ $re ]]; then
  argv="${BASH_REMATCH[1]}"
fi
[ -n "$bin_path" ] || die "não consegui ler o binário no ExecStart de $SERVICE"

arg() {
  local re="--$1[ =]([^ ]+)"
  if [[ $argv =~ $re ]]; then
    printf '%s' "${BASH_REMATCH[1]}"
  fi
}

workdir="$(systemctl show -p WorkingDirectory --value "$SERVICE")"
workdir="${workdir#[-!]}"
listen="$(arg listen)"
listen="${listen:-127.0.0.1:3000}"
db="$(arg db)"
db="${db:-data/traduz.sqlite3}"
case "$db" in
  /*) ;;
  *) db="${workdir:-$repo}/$db" ;;
esac
host="${listen%:*}"
case "$host" in
  0.0.0.0 | "[::]" | "") host="127.0.0.1" ;;
esac
HEALTH_URL="${HEALTH_URL:-http://$host:${listen##*:}/chapters}"
if [ -n "$workdir" ] && [ "$workdir" != "$repo" ]; then
  echo "aviso: o serviço roda em $workdir, não neste clone ($repo); static/ e courses/ vêm de lá" >&2
fi

[ "$(git symbolic-ref --short -q HEAD || true)" = "$BRANCH" ] ||
  die "o clone precisa estar no branch $BRANCH"
if ! git diff --quiet || ! git diff --cached --quiet; then
  die "há alterações locais em arquivos versionados; resolva antes do deploy"
fi

trap on_error ERR
trap 'on_error 130' INT TERM

log "Buscando $REMOTE/$BRANCH"
git fetch --quiet "$REMOTE" "$BRANCH"
previous="$(git rev-parse HEAD)"
target="$(git rev-parse "$REMOTE/$BRANCH")"
if [ "$previous" = "$target" ] && [ "$FORCE" != 1 ]; then
  echo "Nada novo: $(git log --oneline -1). Use FORCE=1 para recompilar e reiniciar mesmo assim."
  exit 0
fi
git merge-base --is-ancestor "$previous" "$target" ||
  die "$REMOTE/$BRANCH não contém o commit atual; resolva o histórico manualmente"

backup="$repo/data/backups/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$backup"
printf '%s\n' "$previous" >"$backup/commit"
if [ -f "$bin_path" ]; then
  cp -p "$bin_path" "$backup/traduz"
fi

phase="atualização do código"
log "Atualizando $(git rev-parse --short HEAD) → $(git rev-parse --short "$target")"
git merge --ff-only --quiet "$target"
git --no-pager log --oneline "$previous..HEAD"

if [ "$SKIP_TESTS" != 1 ]; then
  phase="testes"
  log "Rodando os testes"
  cargo test --locked --quiet
fi

phase="compilação"
log "Compilando em release"
cargo build --release --locked --bins

phase="parada do serviço"
log "Parando $SERVICE e copiando o banco para $backup"
swapped=1
$SUDO systemctl stop "$SERVICE"

phase="backup do banco"
for file in "$db" "$db-wal" "$db-shm"; do
  if [ -f "$file" ]; then
    cp -p "$file" "$backup/"
  fi
done

phase="instalação do binário"
install_binary "$repo/target/release/traduz"

phase="reinício"
$SUDO systemctl start "$SERVICE"

phase="verificação"
log "Conferindo $HEALTH_URL"
if ! wait_healthy; then
  echo "o serviço não respondeu em $HEALTH_URL" >&2
  false
fi

trap - ERR INT TERM
ls -1d "$repo"/data/backups/[0-9]*-[0-9]* | sort | head -n "-$KEEP_BACKUPS" |
  xargs -r -d '\n' rm -rf -- || true
log "Deploy concluído: $(git log --oneline -1)"
echo "Backup de antes do deploy: $backup"
