# Better Distributors

Cadastro de lojas e fornecedores em português, com interface Rust/WebAssembly (Leptos) e API Rust (Axum). Sem React, Vue ou Angular.

## Executar

Requisitos: Rust estável com ferramentas de compilação C/C++ (no Windows, Visual Studio Build Tools), target WebAssembly e Trunk.

```powershell
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
```

Configure primeiro `DATABASE_URL` em `.env` (veja PostgreSQL/Supabase abaixo). É necessário um PostgreSQL acessível; SQLite não é mais utilizado. Na raiz, execute a API:

```powershell
cargo run -p distributor-server
```

Em outro terminal:

```powershell
cd frontend
trunk serve --port 8080
```

Abra http://localhost:8080. O Trunk encaminha `/api/` para o servidor em `127.0.0.1:3000`.

### Build para servir tudo pela API

```powershell
cd frontend
trunk build --release
cd ..
cargo run -p distributor-server --release
```

Abra http://localhost:3000. Execute o servidor a partir da raiz. `DATABASE_URL` é obrigatória e deve apontar para PostgreSQL. Configurações opcionais: `BIND_ADDR` (padrão `127.0.0.1:3000`) e `FRONTEND_DIST` (padrão `frontend/dist`). `ALLOWED_HOSTS` define os hosts permitidos, separados por vírgulas (padrão `localhost,127.0.0.1,[::1]`); ao configurar um proxy HTTPS, inclua seu hostname explicitamente. Esta proteção não substitui autenticação.

## Funcionalidades

- Três áreas: registro, listagem e mapa.
- Cadastro e edição, ficha de detalhes, exclusão confirmada, pesquisa e filtros.
- Loja com contatos, segmento, tipo, endereço e coordenadas opcionais.
- Catálogo por link HTTP/HTTPS e lista de produtos, com preço opcional por produto.
- Captura/importação via seletor nativo de imagens e OCR no navegador. Texto reconhecido precisa de revisão humana.
- Mapa Leaflet/OpenStreetMap para coordenadas cadastradas.
- Contatos acionáveis e navegação externa na ficha; aviso de possível duplicidade ao criar cadastro.
- Persistência PostgreSQL no servidor, não apenas no armazenamento do navegador.
- Validação no servidor e no modelo compartilhado.

## Privacidade e limites desta versão

**É um MVP local, sem login nem isolamento por usuário. Não exponha diretamente à internet ou a uma rede não confiável.** Todos os clientes com acesso à API consultam e alteram a mesma base. A implantação pública exige autenticação, autorização, HTTPS, proteção contra abuso, backups e política de retenção/LGPD.

O OCR roda no navegador por padrão (Tesseract local); a imagem não é enviada à API nem armazenada. Opcionalmente, com `OCR_PROVIDER=gemini` e `GEMINI_API_KEY` no servidor, a imagem **editada e confirmada** (recorte/rotação, normalizada a ≤2048 px JPEG, sem metadados) é transmitida pela aplicação à API do Google apenas sob confirmação explícita no navegador (na prática gratuita, o Google pode usar imagem/texto para melhoria dos produtos, conforme a política vigente). No plano gratuito sem faturamento, a chave do Google é de uso pessoal; solicite-a fora do chat e nunca a publique. O limite diário `OCR_DAILY_LIMIT` é contabilizado somente na memória do processo (sem persistência) e não garante uso gratuito nem evita cobrança; se o Google indicar limite, o servidor aplica um intervalo de espera e o navegador usa o OCR local. Tesseract e seus modelos de idioma são obtidos de serviços externos. O mapa também depende de recursos e tiles externos, cujos provedores recebem requisições de rede. Não há suporte offline completo. Revise as políticas dos provedores antes do uso em produção.

- O seletor com `capture` pode abrir a câmera no celular, mas o comportamento depende do navegador. Também é possível selecionar uma foto existente.
- Geolocalização exige permissão e contexto seguro (HTTPS ou localhost). A localização atual do aparelho **não é necessariamente o endereço da loja**; só use estando no estabelecimento.
- Não há geocodificação automática: endereço textual não cria um marcador. Informe latitude e longitude ou use a posição atual após confirmação.
- OCR não garante identificação correta de campos e não extrai automaticamente tabelas completas de produtos ou manuscritos complexos. O modelo de OCR é instruído a transcrever literalmente; ele não tem acesso a ferramentas, mas pode falhar ou inventar trechos ilegíveis — a revisão humana do texto antes de salvar continua obrigatória.
- OCR Google é opt-in por variáveis de ambiente do servidor; sem isso nada sai do dispositivo.
- Catálogo por link e produtos cadastrados; upload persistente de PDF/imagens ainda não implementado.
- Não há otimização de roteiros, leitura de códigos de barras, estatísticas de preços nem histórico de preços nesta etapa.
- A base não contém dados fictícios. Faça backups PostgreSQL com `pg_dump` e teste a restauração (veja abaixo).
- O manifesto permite metadados de instalação onde suportados; não há service worker nem garantia de instalação/offline em todos os navegadores.

## Validações executadas

- 13 testes Rust de domínio/API: opcionais, links, coordenadas, preços, CRUD, persistência e restrição de hosts/origens.
- Testes adicionais do OCR Google (com provedor simulado, sem chamada real): autenticação por bearer e sequência, guarda de mesma origem, limites de tamanho/tipo, cota diária, 429 e respostas inválidas e exposição apenas do booleano `enabled`.
- Checagem do frontend para `wasm32-unknown-unknown`.
- Análise estática Clippy do backend/modelo e frontend WASM sem warnings.
- `trunk build` concluído.
- Teste automatizado em Microsoft Edge: criar, editar, listar, abrir detalhes pelo mapa e excluir; nenhum erro JavaScript no fluxo.
- Layout em 390 × 844 sem overflow horizontal.
- OCR real com imagem controlada: nome, e-mail e telefone reconhecidos corretamente (não mede precisão em cartões reais).

A precisão do OCR depende da foto. Testes automatizados não substituem a validação da câmera/GPS em Android e iPhone reais.

## Configuração (.env)

O servidor carrega automaticamente o arquivo **`.env` na raiz do projeto** (`cargo run` executado da raiz). Variáveis definidas no ambiente do sistema têm prioridade sobre o arquivo. Copie `.env.example` se o `.env` local for apagado; o `.env` não é versionado.

| Variável | Padrão | Uso |
|---|---|---|
| `DATABASE_URL` | (obrigatória, sem padrão) | URL PostgreSQL completa, somente no servidor |

### Aspas e caracteres especiais no `.env`

No `.env`, envolva a URL completa em **aspas duplas** quando houver espaços ou caracteres especiais:

```dotenv
DATABASE_URL="postgres://postgres:minhasenha@localhost:5432/distributors?sslmode=require"
```

As aspas não substituem a codificação obrigatória:** caractere `#` na senha **deve** ser percent-encodado como `%23`, senão o `#` inicia um comentário no arquivo e trunca a URL — as interface não lê segredos do chat nem registra o valor no log, conforme definido em ambiente seguro.
| `BIND_ADDR` | `127.0.0.1:3000` | Endereço do painel (PC) |
| `FRONTEND_DIST` | `frontend/dist` | Build do frontend servido pelo backend |
| `ALLOWED_HOSTS` | `localhost,127.0.0.1,[::1]` | Hosts autorizados à API local |
| `CAPTURE_PUBLIC_URL` | (definida) | Origem que o celular acessa: HTTPS (túnel) ou HTTP local com `CAPTURE_ALLOW_HTTP=1` |
| `CAPTURE_BIND_ADDR` | `127.0.0.1:3001` | Porta da página pública de captura |
| `CAPTURE_FRONTEND_DIR` | `frontend` | Pasta com `capture.html`/`bridge.js` |
| `OCR_PROVIDER` | (desativado) | `gemini` ativa o OCR remoto; qualquer outro valor usa apenas OCR local |
| `GEMINI_API_KEY` | (nenhuma) | Chave do Google, somente do lado do servidor; nunca no navegador nem no chat |
| `GEMINI_MODEL` | `gemini-3.5-flash-lite` | Modelo do endpoint `generateContent` fixo |
| `OCR_DAILY_LIMIT` | `20` | Teto diário estimativo, apenas em memória; sem garantia de custo zero |

### PostgreSQL / Supabase

No dashboard do seu projeto Supabase, abra **Connect**, selecione **Transaction pooler** (porta **6543**) e copie a URL PostgreSQL completa fornecida pelo painel para `DATABASE_URL`. Não deduza o hostname ou usuário a partir do ID do projeto. Substitua o marcador de senha pela senha real do banco, com caracteres especiais percent-encoded (por exemplo, `@` → `%40`, `#` → `%23`, `%` → `%25`); codifique somente a senha, sem duplicar a codificação. Entre aspas duplas a senha fica protegida de espaços e `#` literal; o `#` percent-encodado (`%23`) continua obrigatório dentro e fora de aspas, pois em dotenv o `#` comentado trunca o valor na leitura. Inclua `sslmode=require` na query (`?` para o primeiro parâmetro, `&` para os seguintes). Não use a chave anon/service-role como senha. Nunca publique a URL nem a envie ao navegador, logs ou artifacts.

O backend exige PostgreSQL, promove TLS `prefer`/`allow` para `require`, desativa o cache de statements do cliente e limita o pool a **5 conexões por processo**. `sslmode=disable` é apenas para PostgreSQL local de desenvolvimento/testes. Supabase remoto exige conectividade; o pareamento local de captura não torna a persistência offline.

Use projeto/banco e credenciais dedicados à aplicação, com privilégios mínimos. Na inicialização, o backend executa `CREATE TABLE IF NOT EXISTS stores` no schema resolvido pelo `search_path` (normalmente `public`), com `id TEXT`, `data TEXT` e `insertion_order BIGINT GENERATED ALWAYS AS IDENTITY`. A credencial precisa permitir essa inicialização, acesso à tabela e à sequência; não há gerenciador de migrações nem ajuste automático de tabelas incompatíveis. **Não há migração automática de SQLite:** preserve o arquivo/volume antigo, planeje exportação/importação explícita e valide os dados antes de desativá-lo.

O backend não configura RLS nem políticas Supabase. Se `public` estiver exposto pela Data API, restrinja grants de `anon`/`authenticated` e configure RLS/políticas adequadas ou desative essa exposição. Não presuma que RLS protege conexões de owners ou roles com `BYPASSRLS`; ela também não adiciona autenticação à API Axum. Não compartilhe este banco com testes.

### OCR Google (opcional)

Para acionar a leitura no servidor, o `.env` precisa de `OCR_PROVIDER=gemini` e `GEMINI_API_KEY=<sua chave>`; obtenha a chave no painel do Google AI Studio (camada gratuita, sem faturamento obrigatório). A chave é usada somente pelo servidor via cabeçalho `x-goog-api-key` no endpoint fixo `generativelanguage.googleapis.com`, com timeout (25 s) e sem redirecionamentos; nada é registrado em log nem persistido. Sem modelo configurado, usa-se `GEMINI_MODEL=gemini-3.5-flash-lite`. `OCR_DAILY_LIMIT` (padrão 20/dia) é um teto conservador contabilizado apenas **na memória do processo** — não são garantias de custo; ao reiniciar, o contador zera. Entradas aceitas: ≤4 MiB de imagem JPEG/PNG, até 2048 px por lado; 2 execuções simultâneas no máximo, sem tentativas de reenvio. Em 429 do Google o servidor entra em espera e o cliente cai automaticamente para o OCR local. Ativação em produção: injete as variáveis no `.env.production` do servidor (o `env_file` do Compose já carrega os valores).

### Captura por QR Code: mesmo Wi‑Fi e 4G/5G

O QR gerado aponta para `CAPTURE_PUBLIC_URL/capture`. **Uma única URL HTTPS pública atende os dois cenários** (o celular acessa pelo mesmo link estando no Wi‑Fi ou no 4G/5G).

Exemplo com cloudflared:

```powershell
cloudflared tunnel --url http://127.0.0.1:3001
# copia a URL https://...trycloudflare.com impressa e coloca em .env:
#   CAPTURE_PUBLIC_URL=https://...trycloudflare.com
#   CAPTURE_BIND_ADDR=127.0.0.1:3001
cargo run -p distributor-server
```

O gerenciamento (cadastros e painel) fica apenas em `BIND_ADDR` (127.0.0.1 por padrão) e **não** é publicado pelo túnel. A sessão do QR expira em 15 minutos (prazo absoluto) e permite várias capturas, uma por vez. Depois de enviar, o celular aguarda o PC revisar e **salvar com sucesso**; só então uma confirmação libera a próxima captura na mesma página. Falhas de salvamento não liberam o celular. Use **Encerrar sessão** no celular ou **Fechar captura** no PC; a página mostra o encerramento sem depender de `window.close()`. Não são expostos lojas, painel nem a foto do cartão — o OCR local roda no próprio navegador, e o OCR Google opcional transmite apenas a imagem editada e confirmada, sob autorização (bearer + sequência) e confirmação explícita.

O protocolo usa polling, sem WebSocket: token de escrita somente no bridge público, token de leitura/confirmação somente no painel local. Cada envio e confirmação contém uma sequência; reenvio idêntico é idempotente, confirmações antigas não liberam capturas novas, e transições são serializadas no backend. O painel repete confirmações em falhas transitórias após salvar, sem salvar a loja novamente. Sessões ficam em memória e são invalidadas ao reiniciar o servidor; recarregar a página do celular perde o token (removido do endereço por segurança). A confirmação é uma declaração do painel local após resposta de sucesso da API de lojas, não uma transação PostgreSQL conjunta.

### Alternativa offline (mesma rede, sem túnel)

Sem internet ou sem domínio, o pareamento funciona por HTTP na rede local. No `.env`:

```dotenv
CAPTURE_ALLOW_HTTP=1
CAPTURE_PUBLIC_URL=http://192.168.0.50:3001   # IP do PC (ipconfig)
CAPTURE_BIND_ADDR=0.0.0.0:3001
```

Libere a porta 3001 no firewall do Windows (rede privada). Nesse modo, **a câmera usa o app nativo do celular** e a digitação manual (“Colar texto”) funciona sem internet; o OCR da foto precisa de internet no celular para baixar o leitor. HTTP local é para teste/diário em rede confiável — para produção, prefira o túnel HTTPS.

## Deploy Docker no VPS

O workflow separado `Deploy` executa somente após `CI` concluir com sucesso em um push de `main` deste repositório. O checkout usa o `head_sha` testado, e o Buildx produz `linux/amd64` com tag `better-distributors:sha-<SHA completo>`, sem `latest`. A imagem é exportada com `docker save`, comprimida e transferida por SSH; não há push ao GHCR, token de registry nem permissão `packages: write`. Novos deploys aguardam o deploy em andamento, sem cancelá-lo. Reexecuções do mesmo SHA podem reconstruir dependências de sistema diferentes, pois Rust stable e as imagens base acompanham atualizações; para repetir exatamente um release, preserve sua imagem local.

### Preparação do servidor

Use um VPS Linux amd64 com Docker Engine e o plugin Docker Compose v2 com suporte a `up --wait`, além de Bash, gzip e flock (util-linux). Instale Docker pelo procedimento oficial da distribuição antes do primeiro deploy. O usuário de deploy precisa executar `docker` sem sudo (acesso ao grupo Docker equivale a root). Crie `/opt/better-distributors`, de propriedade desse usuário, com permissão `0700`. Reserve espaço para compilação no runner e, no VPS, para o arquivo comprimido, a imagem nova e a anterior; o banco é externo ao container; monitore disco e logs do Docker.

Crie manualmente `/opt/better-distributors/.env.production` com permissão `0600`, sem versioná-lo nem enviá-lo como artifact. Preencha `DATABASE_URL` com a URL completa do painel Supabase, incluindo senha codificada e `sslmode=require`, conforme acima. Exemplo (preencha o banco e substitua o domínio de captura):

```dotenv
DATABASE_URL=
ALLOWED_HOSTS=localhost,127.0.0.1,[::1]
CAPTURE_PUBLIC_URL=https://capture.example.com
```

O Compose mantém `env_file: .env.production`: `DATABASE_URL` vem desse arquivo, sem override no Compose ou na imagem. Os binds internos são fixos em `0.0.0.0:3000` e `0.0.0.0:3001`, assim como os caminhos dos assets. HTTP de captura é desativado. Inclua `127.0.0.1` em `ALLOWED_HOSTS` para o healthcheck e adicione o hostname de qualquer proxy privado autorizado. Não há volume de banco nem diretório `/data`; PostgreSQL é administrado separadamente. O processo roda sem root (UID/GID `10001:10001`), com certificados CA e curl instalados.

### GitHub e SSH

Crie o environment **production**, preferencialmente com aprovação obrigatória e restrição à branch `main`, e configure estes secrets nele:

| Secret | Conteúdo |
|---|---|
| `VPS_HOST` | Hostname ou IPv4 do VPS, sem protocolo ou porta |
| `VPS_USER` | Usuário de deploy proprietário de `/opt/better-distributors` |
| `VPS_SSH_KEY` | Chave privada SSH dedicada, sem senha interativa; instale a chave pública no VPS |
| `VPS_KNOWN_HOSTS` | Linha(s) known_hosts verificadas por canal independente, como o console do provedor |
| `VPS_PORT` | Opcional; porta SSH, padrão `22` |

Para porta não padrão, known_hosts deve identificar `[host]:porta`. Não confie cegamente no resultado de `ssh-keyscan`; compare o fingerprint com o servidor por canal confiável. O workflow exige verificação estrita, sem aceitar automaticamente chaves novas. Permita SSH dos runners conforme sua política de firewall. O workflow `workflow_run` precisa existir na branch padrão; mantenha `main` como branch padrão e o nome do workflow de validação como `CI`.

### Acesso, saúde e rollback

As duas portas são publicadas **somente em 127.0.0.1 do VPS**. Não abra 3000/3001 no firewall, não publique a API Docker e não crie endpoint público de gerenciamento. Acesse o painel por VPN ou túnel SSH, por exemplo `ssh -L 3000:127.0.0.1:3000 usuario@vps`, abrindo `http://localhost:3000`. Se usar reverse proxy para gerenciamento, mantenha-o privado, com HTTPS, autenticação e autorização obrigatórias: a aplicação não implementa login.

Para celulares, configure um reverse proxy HTTPS no host para encaminhar **somente o serviço de captura** a `127.0.0.1:3001`, usando a origem de `CAPTURE_PUBLIC_URL`. Nunca encaminhe esse domínio para 3000. Configure limites de requisições e tamanho, TLS e controles de acesso apropriados; preserve o protocolo HTTPS e teste o fluxo QR real. Não há endpoint de deploy ou administração exposto pela aplicação.

O deploy carrega a imagem localmente, valida o Compose e aguarda o healthcheck `GET /api/stores`. `.env.release` guarda `APP_IMAGE` do release ativo; `.env.previous` e `compose.previous.yml` preservam o anterior. Se a atualização falhar, o workflow tenta restaurar a imagem e o Compose anteriores e termina com erro; no primeiro deploy, remove os containers. O rollback é somente da aplicação; ambos os releases usam o `DATABASE_URL` atual de `.env.production`. Não volte a uma imagem/Compose SQLite após a migração para PostgreSQL. Falhas de SSH/energia podem exigir intervenção manual. Para rollback manual, execute na pasta `/opt/better-distributors`:

```bash
docker compose --env-file .env.previous -f compose.previous.yml up -d --wait --wait-timeout 120
cp compose.previous.yml compose.yml
cp .env.previous .env.release
```

Não remova a imagem anterior com `docker image prune -a`; retenha pelo menos as imagens referenciadas em `.env.release` e `.env.previous`. As transferências ficam em `releases/<SHA>`; o arquivo comprimido é removido após `docker load`. Revise e limpe releases antigos conforme a política de retenção. O rollback de imagem **não restaura PostgreSQL nem desfaz alterações de schema/dados**; confirme compatibilidade do release anterior antes de usá-lo.

### Backup e recuperação

Use `pg_dump` de versão compatível com o servidor PostgreSQL antes de mudanças de schema. Para ferramentas administrativas, obtenha no **Connect** do Supabase a conexão direta ou session pooler apropriada; não reutilize automaticamente o transaction pooler. Configure `PGHOST`, `PGPORT`, `PGUSER`, `PGDATABASE`, `PGSSLMODE=require` e um `PGPASSFILE` protegido (permissão `0600` no Linux), sem senha na linha de comando/histórico:

```bash
pg_dump --format=custom --no-owner --no-acl --file=distributors.dump
pg_restore --dbname=distributors_restore --no-owner --no-acl distributors.dump
```

A restauração acima exige um banco vazio separado, previamente criado para teste; nunca aponte para produção sem um plano de recuperação. `pg_dump` fornece snapshot consistente sem parar o serviço; para cutover de migração, suspenda gravações e valide contagens/dados. Mantenha backups criptografados fora do VPS, defina retenção e teste a restauração. Imagens Docker e rollback não são backup dos dados. Preserve backups SQLite antigos até concluir e validar a importação manual.

## Migrar do SQLite legado para PostgreSQL

O antigo `distributors.db` (SQLite) não é migrado automaticamente. Use o exportador standalone, que abre o banco **somente leitura** (`?mode=ro`), nunca altera o arquivo e nunca executa SQL no PostgreSQL de destino:

```powershell
python export_sqlite.py --source distributors.db --output migration_stores.sql
```

O `--output` gere um arquivo transactional com todas as inserções dentro de um `BEGIN`/`COMMIT`; falha em qualquer linha reverte todo o import. O arquivo gera `createdb` registra o schema atual (`stores` apenas, colunas `id` e `data`). O SQL resultante:

- Contém apenas o schema e os dados relevantes; sem credenciais, `DATABASE_URL` ou segredos.
- Escapa todas as strings como literais PostgreSQL robustos (`E'...'`), com aspas, contrabarras (`\\`), Unicode, `NULL`, booleanos, números finitos e sem injeção por conteúdo do banco.
- Converte IDs UUID legados do SQLite para **ULIDs** (26 caracteres, ordenáveis por tempo), mantendo a tabela temporária `store_id_mapping` (`old_id` → `new_id`) no mesmo Lote para rastreio de todas as referências cruzadas.
- Trunca a importação se a tabela `stores` no PostgreSQL de destino não estiver vazia, evitando sobrescrever registros existentes.
- A primeira execução nunca sobrescreve um `--output` existente; escolha um nome de arquivo novo por execução.

Importe no PostgreSQL com `\set ON_ERROR_STOP` para abortar o `psql` no primeiro erro e reverte a transação:

```bash
psql -v ON_ERROR_STOP=1 -f migration_stores.sql
```

Faça backup antes (`pg_dump`, seção abaixo), teste em banco de teste primeiro e nunca aponte para produção sem validar. O SQL gerado segue transação; o `psql` sem `ON_ERROR_STOP` pode mascarar falhas parciais. Ajustes finais com o editor ou patches são opcionais; o `psql` deve rodar com o mesmo encoding UTF-8 e `client_min_messages` padrão.

Os ULIDs novos **coexistem** com IDs legados do PostgreSQL: o backend continua aceitando e atualizando registros cujo `id` ainda é UUID, sem destruição automática de dados antigos; apenas **novos** cadastros recebem ULIDs. Tokens de autorização da captura permanecem `uuid` aleatórios (não afetados). `capture.rs` também usa `uuid::Uuid`, preservando porte UUID→ULID aqui.
## Estrutura

- `frontend/`: interface Rust/Leptos, CSS e bridge JavaScript para OCR/mapa/geolocalização.
- `backend/`: API Axum e persistência PostgreSQL.
- `shared/`: contratos de dados e validação comum.
- `src/main.rs`: arquivo inicial preservado, fora do workspace ativo.

## Testes

Os testes de banco exigem `TEST_DATABASE_URL` apontando para um PostgreSQL descartável dedicado, nunca produção; não usam `DATABASE_URL`. A role precisa criar/remover schemas isolados por teste. A CI usa PostgreSQL 17 (`postgres`/`postgres`, banco `distributors_test`) com healthcheck `pg_isready`. O E2E roda depois dos testes nativos e usa o mesmo banco local seguro via `DATABASE_URL`, na tabela padrão, enquanto os testes nativos usam schemas próprios. `OCR_PROVIDER=local` e `GEMINI_API_KEY` vazia impedem chamadas reais ao Google.

Com esse banco local criado e acessível:

```powershell
$env:TEST_DATABASE_URL = 'postgres://postgres:postgres@localhost:5432/distributors_test?sslmode=disable'
$env:OCR_PROVIDER = 'local'
$env:GEMINI_API_KEY = ''
cargo test -p distributor-model -p distributor-server
cargo check -p distributor-web --target wasm32-unknown-unknown
```

Próximas etapas de produção: autenticação/equipes, PostGIS e migrações versionadas, armazenamento privado de anexos, geocodificação contratada, testes em aparelhos reais e rotas com múltiplas paradas.
