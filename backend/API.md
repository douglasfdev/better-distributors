# API local

Todas as respostas de dados são JSON. Não há autenticação nesta versão: não publique diretamente. Hosts e origens são restritos a localhost/loopback por padrão (`ALLOWED_HOSTS` permite configuração explícita); requisições bloqueadas retornam 403.

| Método | Rota | Resultado |
|---|---|---|
| GET | `/api/health` | `200 {"status":"ok"}` |
| GET | `/api/stores` | `200 Store[]` |
| POST | `/api/stores` | `201 Store`, id e timestamp gerados no servidor |
| PUT | `/api/stores/{id}` | `200 Store`, substituição do registro |
| DELETE | `/api/stores/{id}` | `204`, sem conteúdo |

Campos omitidos recebem valores padrão; PUT substitui o registro inteiro, não é atualização parcial. Limite do corpo: 1 MiB. Nome obrigatório; latitude/longitude devem ser um par válido; preço é opcional e não negativo; URLs devem começar com HTTP(S). Erros de domínio são `{ "error": "mensagem" }`, com 422 para validação, 404 para registro inexistente e 500 para falha interna. Erros do extrator HTTP (JSON inválido, tamanho excedido) podem retornar texto.

Exemplo de criação:

```json
{
  "name": "Nome do fornecedor",
  "segment": "Alimentos",
  "kind": "Distribuidora",
  "phone": "11999999999",
  "address": "Endereço a confirmar",
  "catalog_url": "",
  "products": [
    { "name": "Produto sem cotação", "unit": "caixa", "price": null }
  ]
}
```

O contrato completo está em `shared/src/lib.rs`. A API não faz geocodificação, não transfere URLs cadastradas e não recebe fotos para OCR.
