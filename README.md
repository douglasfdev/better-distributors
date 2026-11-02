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

O gerenciamento (cadastros e painel) fica apenas em `BIND_ADDR` (127.0.0.1 por padrão) e **não** é publicado pelo túnel. A sessão do QR expira em 15 minutos, permite um único envio e não expõe lojas, painel nem a foto do cartão — o OCR roda no próprio navegador.

### Alternativa offline (mesma rede, sem túnel)

Sem internet ou sem domínio, o pareamento funciona por HTTP na rede local. No `.env`:

```dotenv
CAPTURE_ALLOW_HTTP=1
CAPTURE_PUBLIC_URL=http://192.168.0.50:3001   # IP do PC (ipconfig)
CAPTURE_BIND_ADDR=0.0.0.0:3001
```

Libere a porta 3001 no firewall do Windows (rede privada). Nesse modo, **a câmera usa o app nativo do celular** e a digitação manual (“Colar texto”) funciona sem internet; o OCR da foto precisa de internet no celular para baixar o leitor. HTTP local é para teste/diário em rede confiável — para produção, prefira o túnel HTTPS.

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
