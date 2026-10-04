---
name: afterwork
description: roda ao fim de toda tarefa, antes do pull request. lê o specs.yaml ao lado, percorre os arquivos que a mudança deixou desatualizados e põe cada um em dia, no próprio lugar.
project: urna
audience: agentes de ia e dev humanos
status: active
last-updated: 2026-10-04
domain: workflow
---

# Afterwork

Analisar todas tarefas realizadas nessa seçao, comparar com diff, commits changelogs. O `specs.yaml`, nesta pasta, é a lista de todo arquivo que lista todos arquivos que precisam de atualização em determinados tipos de mudanças e/ou implementa;cões.

Edite no próprio lugar, como se o arquivo sempre tivesse dito a coisa certa. Histórico mora em três lugares apenas: `docs/CHANGELOG`, a mensagem do commit e o corpo do pull request. Nenhuma nota do tipo "renomeado x para y" ou "atualizado para o novo fluxo" em outro lugar, e nenhuma edição em arquivo que a mudança não afeta. Cada entrada só se aplica quando o diff bate com o seu `when`: uma mudança só de docs pula os builds e o gate, uma mudança fora do formato pula as fixtures.

## O walk

Ao fim de cada tarefa e antes de encerrar uma sessão:

1. Revise o diff final (`git diff origin/main` mais `git ls-files --others --exclude-standard`) e nomeie os comportamentos, comandos, interfaces, artefatos e processos que ele muda.
2. Leia o `specs.yaml` contra essa lista. Toda entrada cujo `when` o diff atende é aberta e posta em dia; sua `rule` diz como (um gerador a rodar, um symlink a preservar, um arquivo que nunca se edita à mão). O snippet abaixo imprime as entradas que o diff já toca e qualquer entrada cujo path não existe mais; o resto é ler o `when` contra o diff.
3. Siga cada atualização pelos seus chamadores e referências (`git grep -n <nome>`; nunca `grep -r`, que varre os clones ignorados pelo git em `tools/` e `TMP/`) até não sobrar referência inconsistente. Inspecione os diretórios relevantes recursivamente; não edite arquivos alheios à mudança só porque foram inspecionados.
4. Valide comandos, links, configuração e exemplos afetados com as verificações cabíveis. Reaproveite evidência válida e nomeie o commit em que ela foi testada. Uma mudança só de docs não repete benchmarks.
5. Registre o resultado final, os caminhos não verificados e as ações indispensáveis do mantenedor. Não declare conclusão enquanto houver referência afetada ou verificação exigida em aberto.

```sh
python3 - <<'EOF'
import os, subprocess, yaml
spec = yaml.safe_load(open(".contracts/.agents/.skills/afterwork/specs.yaml"))
run = lambda *a: subprocess.run(["git", *a], capture_output=True, text=True).stdout.split()
diff = run("diff", "--name-only", "origin/main") + run("ls-files", "--others", "--exclude-standard")
for group in spec["groups"]:
    for entry in group["files"]:
        path = entry["path"]
        if not os.path.lexists(path):
            print("missing:", path)
        elif any(d == path or d.startswith(path.rstrip("/") + "/") for d in diff):
            print("touched:", path, "::", entry["when"])
EOF
```

Três entradas carregam a maior parte das mudanças e merecem ser nomeadas: `docs/CHANGELOG` recebe uma linha em `[Unreleased]` para qualquer coisa que um usuário, um operador ou um contribuidor perceberia, com o porquê e os números medidos (ele é também o registro de decisões, e as contagens de testes nele batem com o que roda); `docs/arc/ARC.toml` recebe a mudança de arquitetura, um arquivo versionado novo no seu `inventory`, o `last-updated` avançado e uma frase curta e datada na `summary`; `.contracts/.agents/AGENTS.md` recebe um comando, gotcha, limitação conhecida, contagem de testes ou layout que mudou (o `CLAUDE.md` da raiz é um symlink para ele; nunca edite o link nem crie um arquivo paralelo).

## Higiene de arquivos

Todo arquivo que a mudança criou ou fez crescer:

- Acima de 639 linhas: leia o que ele faz, do que depende e quem o importa, depois divida por responsabilidade em módulos que fazem uma coisa cada. Atualize todo import e chamador, e mantenha a superfície pública onde estava. Os testes passam antes e depois, com a mesma contagem. Isentos: testes, dados e arquivos gerados, lockfiles, JSON, YAML, TOML, RON, JSONL, CSV, datasets, arquivos vendorizados.
- Peso morto: scripts temporários, logs, backups, arquivos perdidos, código que nada chama. Confira que nada o importa, apague, rode os testes.
- Fora do lugar: um arquivo fora da pasta a que seu papel pertence segundo o ARC.toml, ou nomeado contra o estilo do repo (diretórios e assets em kebab-case, os nomes em maiúsculas dos docs como `USAGE.md` e `ARC.toml`, fontes idiomáticas à linguagem). Mova ou renomeie (`git mv`, `git mv -f` para renome só de caixa), corrija toda referência, liste no inventário do ARC.toml e no `specs.yaml`.
- No código: caminhos específicos da máquina e valores fixos que deveriam vir de config ou env, e comentários prolixos, obsoletos ou que narram histórico. Corrija ou enxugue.

## Lições

Algo que falhou primeiro e depois achou seu conserto vale uma linha para a próxima pessoa:

- Uma armadilha de dev ou de agente vai para os `gotchas` do AGENTS.md;
- Uma lição de release ou operação vai para o checklist do mantenedor em `docs/USAGE.md` (item 11);
- Uma decisão vai para a sua entrada no `docs/CHANGELOG`, com o porquê.

Nada de arquivos de notas por sessão; a lição mora onde a próxima pessoa vai procurar.

## Antes do pull request

- O walk está feito: toda entrada do `specs.yaml` que o diff atende foi atualizada, e o snippet não imprime nenhuma linha `missing:`.
- O bloco de higiene de arquivos acima está feito: nenhum arquivo acima do limite, nada morto ou fora do lugar.
- Para uma mudança de código: `scripts/release_check.sh` (ele reconstrói o `python/_urna.so` após mudanças em Rust; rode-o, não o edite para passar), ou no mínimo `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` e ruff, limpos. Uma mudança só de docs diz no pull request que o gate não rodou.
- `forge-core`, quando a mudança o toca: testado no seu próprio manifesto (`--workspace` não o alcança).
- Nenhum script temporário, arquivo perdido (um gerenciador de pacotes rodado da raiz do repo deixa um `package.json`), código morto ou import órfão; `git status` limpo fora a mudança.
- Nenhum caminho fixo da máquina; comentários curtos e atuais.
- Docs: sem emoji, sem travessão.
- Local e remoto em sincronia: branches mescladas apagadas, nada sem push.
- Um pull request por assunto, em inglês: o título diz o que muda, o corpo diz por quê e como foi testado.
