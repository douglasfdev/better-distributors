# Better Distributors

Cadastro de lojas e fornecedores em português, com interface Rust/WebAssembly (Leptos) e API Rust (Axum). Sem React, Vue ou Angular.

## Executar

Requisitos: Rust estável com ferramentas de compilação C/C++ (no Windows, Visual Studio Build Tools), target WebAssembly e Trunk.

```powershell
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
```

Na raiz, execute a API:

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

Abra http://localhost:3000. Execute o servidor a partir da raiz. Configurações opcionais: `DATABASE_URL` (padrão `sqlite://distributors.db?mode=rwc`), `BIND_ADDR` (padrão `127.0.0.1:3000`) e `FRONTEND_DIST` (padrão `frontend/dist`). `ALLOWED_HOSTS` define os hosts permitidos, separados por vírgulas (padrão `localhost,127.0.0.1,[::1]`); ao configurar um proxy HTTPS, inclua seu hostname explicitamente. Esta proteção não substitui autenticação.

## Funcionalidades

- Três áreas: registro, listagem e mapa.
- Cadastro e edição, ficha de detalhes, exclusão confirmada, pesquisa e filtros.
- Loja com contatos, segmento, tipo, endereço e coordenadas opcionais.
- Catálogo por link HTTP/HTTPS e lista de produtos, com preço opcional por produto.
- Captura/importação via seletor nativo de imagens e OCR no navegador. Texto reconhecido precisa de revisão humana.
- Mapa Leaflet/OpenStreetMap para coordenadas cadastradas.
- Contatos acionáveis e navegação externa na ficha; aviso de possível duplicidade ao criar cadastro.
- Persistência SQLite no servidor, não apenas no armazenamento do navegador.
- Validação no servidor e no modelo compartilhado.

## Privacidade e limites desta versão

**É um MVP local, sem login nem isolamento por usuário. Não exponha diretamente à internet ou a uma rede não confiável.** Todos os clientes com acesso à API consultam e alteram a mesma base. A implantação pública exige autenticação, autorização, HTTPS, proteção contra abuso, backups e política de retenção/LGPD.

O OCR roda no navegador; a imagem não é enviada à API nem armazenada. Tesseract e seus modelos de idioma são obtidos de serviços externos. O mapa também depende de recursos e tiles externos, cujos provedores recebem requisições de rede. Não há suporte offline completo. Revise as políticas dos provedores antes do uso em produção.

- O seletor com `capture` pode abrir a câmera no celular, mas o comportamento depende do navegador. Também é possível selecionar uma foto existente.
- Geolocalização exige permissão e contexto seguro (HTTPS ou localhost). A localização atual do aparelho **não é necessariamente o endereço da loja**; só use estando no estabelecimento.
- Não há geocodificação automática: endereço textual não cria um marcador. Informe latitude e longitude ou use a posição atual após confirmação.
- OCR não garante identificação correta de campos e não extrai automaticamente tabelas completas de produtos ou manuscritos complexos.
- Catálogo por link e produtos cadastrados; upload persistente de PDF/imagens ainda não implementado.
- Não há otimização de roteiros, leitura de códigos de barras, estatísticas de preços nem histórico de preços nesta etapa.
- A base não contém dados fictícios. Faça backup do arquivo `distributors.db` com o servidor parado.
- O manifesto permite metadados de instalação onde suportados; não há service worker nem garantia de instalação/offline em todos os navegadores.

## Validações executadas

- 13 testes Rust de domínio/API: opcionais, links, coordenadas, preços, CRUD, persistência e restrição de hosts/origens.
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
| `DATABASE_URL` | `sqlite://distributors.db?mode=rwc` | Banco SQLite |
| `BIND_ADDR` | `127.0.0.1:3000` | Endereço do painel (PC) |
| `FRONTEND_DIST` | `frontend/dist` | Build do frontend servido pelo backend |
| `ALLOWED_HOSTS` | `localhost,127.0.0.1,[::1]` | Hosts autorizados à API local |
| `CAPTURE_PUBLIC_URL` | (definida) | Origem que o celular acessa: HTTPS (túnel) ou HTTP local com `CAPTURE_ALLOW_HTTP=1` |
| `CAPTURE_BIND_ADDR` | `127.0.0.1:3001` | Porta da página pública de captura |
| `CAPTURE_FRONTEND_DIR` | `frontend` | Pasta com `capture.html`/`bridge.js` |

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

O gerenciamento (cadastros e painel) fica apenas em `BIND_ADDR` (127.0.0.1 por padrão) e **não** é publicado pelo túnel. A sessão do QR expira em 15 minutos (prazo absoluto) e permite várias capturas, uma por vez. Depois de enviar, o celular aguarda o PC revisar e **salvar com sucesso**; só então uma confirmação libera a próxima captura na mesma página. Falhas de salvamento não liberam o celular. Use **Encerrar sessão** no celular ou **Fechar captura** no PC; a página mostra o encerramento sem depender de `window.close()`. Não são expostos lojas, painel nem a foto do cartão — o OCR roda no próprio navegador.

O protocolo usa polling, sem WebSocket: token de escrita somente no bridge público, token de leitura/confirmação somente no painel local. Cada envio e confirmação contém uma sequência; reenvio idêntico é idempotente, confirmações antigas não liberam capturas novas, e transições são serializadas no backend. O painel repete confirmações em falhas transitórias após salvar, sem salvar a loja novamente. Sessões ficam em memória e são invalidadas ao reiniciar o servidor; recarregar a página do celular perde o token (removido do endereço por segurança). A confirmação é uma declaração do painel local após resposta de sucesso da API de lojas, não uma transação SQLite conjunta.

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

Use um VPS Linux amd64 com Docker Engine e o plugin Docker Compose v2 com suporte a `up --wait`, além de Bash, gzip e flock (util-linux). Instale Docker pelo procedimento oficial da distribuição antes do primeiro deploy. O usuário de deploy precisa executar `docker` sem sudo (acesso ao grupo Docker equivale a root). Crie `/opt/better-distributors`, de propriedade desse usuário, com permissão `0700`. Reserve espaço para compilação no runner e, no VPS, para o arquivo comprimido, a imagem nova, a anterior e o banco; monitore disco e logs do Docker.

Crie manualmente `/opt/better-distributors/.env.production` com permissão `0600`, sem versioná-lo nem enviá-lo como artifact. Exemplo (substitua o domínio de captura):

```dotenv
ALLOWED_HOSTS=localhost,127.0.0.1,[::1]
CAPTURE_PUBLIC_URL=https://capture.example.com
```

O Compose fixa os binds internos em `0.0.0.0:3000` e `0.0.0.0:3001`, os caminhos dos assets e `DATABASE_URL=sqlite:///data/distributors.db?mode=rwc`; não tente sobrescrevê-los no arquivo. HTTP de captura é desativado. Inclua `127.0.0.1` em `ALLOWED_HOSTS` para o healthcheck e adicione o hostname de qualquer proxy privado autorizado. O volume nomeado `better-distributors-data` recebe `/data` com proprietário UID/GID `10001:10001` na primeira criação. Volumes existentes precisam dessa propriedade corrigida antes do deploy. O processo roda sem root, com certificados CA e curl instalados.

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

O deploy carrega a imagem localmente, valida o Compose e aguarda o healthcheck `GET /api/stores`. `.env.release` guarda `APP_IMAGE` do release ativo; `.env.previous` e `compose.previous.yml` preservam o anterior. Se a atualização falhar, o workflow tenta restaurar a imagem e o Compose anteriores e termina com erro; no primeiro deploy, remove os containers sem apagar o volume. Falhas de SSH/energia podem exigir intervenção manual. Para rollback manual, execute na pasta `/opt/better-distributors`:

```bash
docker compose --env-file .env.previous -f compose.previous.yml up -d --wait --wait-timeout 120
cp compose.previous.yml compose.yml
cp .env.previous .env.release
```

Não remova a imagem anterior com `docker image prune -a`; retenha pelo menos as imagens referenciadas em `.env.release` e `.env.previous`. As transferências ficam em `releases/<SHA>`; o arquivo comprimido é removido após `docker load`. Revise e limpe releases antigos conforme a política de retenção. O rollback de imagem **não desfaz migrações nem alterações no SQLite**.

### Backup e recuperação

Antes de atualizar versões com alterações no banco, pare o serviço com `docker compose --env-file .env.release stop app`, faça backup de todo o volume `better-distributors-data` (incluindo eventuais arquivos WAL/SHM), e reinicie com `docker compose --env-file .env.release up -d --wait`. Use ferramenta de backup de volumes ou container auxiliar confiável com o volume montado somente para leitura enquanto o serviço está parado. Mantenha backups criptografados fora do VPS, defina retenção e teste a restauração com UID/GID `10001:10001`. Nunca execute `docker compose down -v` em produção. Imagens Docker e rollback não são backup dos dados.

## Estrutura

- `frontend/`: interface Rust/Leptos, CSS e bridge JavaScript para OCR/mapa/geolocalização.
- `backend/`: API Axum e persistência SQLite.
- `shared/`: contratos de dados e validação comum.
- `src/main.rs`: arquivo inicial preservado, fora do workspace ativo.

## Testes

```powershell
cargo test -p distributor-model -p distributor-server
cargo check -p distributor-web --target wasm32-unknown-unknown
```

Próximas etapas de produção: autenticação/equipes, PostgreSQL/PostGIS e migrações, armazenamento privado de anexos, geocodificação contratada, testes em aparelhos reais e rotas com múltiplas paradas.
