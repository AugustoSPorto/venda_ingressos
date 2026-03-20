# 🎫 Sistema Distribuído de Venda de Ingressos com Eleição de Líder

Sistema distribuído em Rust que combina:
- **Eleição de Líder** (algoritmo Bully)
- **Heartbeat** para detecção de falhas
- **Fila de Espera** (Redis Sorted Set)
- **Venda de Ingressos** (PostgreSQL com lock FOR UPDATE)
- **Consistência Forte** (replicação com ACK)
- **Load Balancing** (Nginx least_conn)
- **Frontend React** interativo

---

## 📁 Estrutura do Projeto

```
projeto-sd-2025/
├── src/
│   ├── main.rs         # Entry point com inicialização Redis/Postgres
│   ├── node.rs         # State machine + HTTP API + endpoints de ingressos
│   ├── election.rs     # Algoritmo Bully
│   ├── sync.rs         # Replicação com ACK
│   └── message.rs      # Protocolo inter-nó (JSON/TCP)
├── frontend/
│   └── src/
│       └── App.jsx     # Interface React
├── nginx.conf          # Load balancer
├── docker-compose.yml  # Orquestração (Postgres, Redis, 3 nós, Nginx)
├── init.sql            # Schema do banco
├── Dockerfile          # Imagem dos nós Rust
└── Cargo.toml          # Dependências
```

---

## 🏗️ Arquitetura

```
┌──────────────────────────────────────────────┐
│         Cliente HTTP (React)                  │
└─────────────────┬────────────────────────────┘
                  │ port 80
        ┌─────────▼─────────┐
        │   Nginx (LB)      │  least_conn
        └─┬────┬────┬───────┘
          │    │    │
    8001  │    │8002│ 8003
   ┌──────▼┐ ┌─▼────┐ ┌─▼─────┐
   │ node1 │ │node2 │ │ node3 │ ← Cluster Rust
   │Follower│ │Follower│ │Leader │
   └───┬───┘ └──┬───┘ └───┬───┘
       │        │         │
       └────────┴─────────┘ TCP 9001/9002/9003
              │       │
     ┌────────▼───┐ ┌▼────────┐
     │ PostgreSQL │ │  Redis  │
     └────────────┘ └─────────┘
```

### Como os Conceitos se Integram

| Conceito | Onde | Como Funciona |
|----------|------|---------------|
| **Eleição de Líder** | `election.rs` | Algoritmo Bully: ID mais alto vence. Iniciada por timeout de heartbeat. |
| **Heartbeat** | `node.rs` | Líder envia heartbeat a cada 1s; followers disparam eleição após 3s sem resposta. |
| **Fila de Espera** | Redis Sorted Set | Timestamp como score. Posição = rank no sorted set. |
| **Checkout** | PostgreSQL | `FOR UPDATE` bloqueia linha durante transação (evita double booking). |
| **Replicação** | `sync.rs` | Líder aguarda ACK de TODOS os backups antes de confirmar. |
| **Redirecionamento** | `node.rs` | Followers redirecionam escritas (checkout) para o líder via HTTP 307. |
| **Load Balancing** | Nginx | Distribui GET /status e POST /entrar_fila entre todos os nós. |

---

## 🚀 Como Executar

### Pré-requisitos

- [Docker](https://docs.docker.com/get-docker/) e Docker Compose
- [Node.js 18+](https://nodejs.org/) (para o frontend React)
- [Rust 1.70+](https://rustup.rs/) (opcional, para build local)

### 1. Iniciar o Backend (Cluster + Banco + Redis)

```bash
# Clone o repositório (ou descompacte os arquivos)
cd projeto-sd-2025

# Inicie os containers
docker compose up --build

# Aguarde até ver:
# [node 1] HTTP listening on 0.0.0.0:8001
# [node 2] HTTP listening on 0.0.0.0:8002
# [node 3] HTTP listening on 0.0.0.0:8003
# [node 3]: elected as leader (term 1)
```

**O que acontece:**
1. PostgreSQL inicializa com 100 ingressos (init.sql)
2. Redis inicia vazio
3. Nós 1, 2, 3 iniciam simultaneamente
4. Após 2s, nós detectam ausência de heartbeat
5. Node 3 (maior ID) vence a eleição
6. Node 3 começa a enviar heartbeats

### 2. Testar via API (Terminal)

```bash
# Ver status do cluster (roteado pelo Nginx)
curl http://localhost/status

# Entrar na fila
curl -X POST http://localhost/entrar_fila

# Exemplo de resposta:
# {"user_id":"f47ac10b-58cc-4372-a567-0e02b2c3d479","posicao":0,"node_id":2}

# Comprar ingresso (substitua <user_id> e <ticket_id>)
curl -X POST http://localhost/checkout \
  -H "Content-Type: application/json" \
  -d '{"user_id":"f47ac10b-58cc-4372-a567-0e02b2c3d479","ticket_id":1}'

# Se você chamar um follower, ele redireciona para o leader:
# < HTTP/1.1 307 Temporary Redirect
# < Location: http://node3:8003/checkout
```

### 3. Frontend React

```bash
cd frontend
npm install
npm start

# Abre http://localhost:3000
```

**Funcionalidades:**
- Ver status do cluster em tempo real
- Entrar na fila (retorna ID + posição)
- Comprar ingressos (somente após entrar na fila)
- Visualizar nó que processou cada operação

---

## 🧪 Testar Eleição e Failover

### Cenário 1: Matar o Líder

```bash
# 1. Verificar quem é o líder
curl http://localhost/status
# {"id":3,"role":"Leader",...}

# 2. Matar o líder
docker stop node3

# 3. Aguardar 3 segundos (HEARTBEAT_TIMEOUT)
# Logs mostrarão:
# [node 2] heartbeat timeout — starting election
# [node 2] elected as leader (term 2)

# 4. Verificar novo líder
curl http://localhost/status
# {"id":2,"role":"Leader",...}

# 5. Tentar comprar ingresso (funciona normalmente)
curl -X POST http://localhost/checkout \
  -H "Content-Type: application/json" \
  -d '{"user_id":"...","ticket_id":5}'

# 6. Religar o node3
docker start node3
# Ele volta como Follower e recebe heartbeat do novo líder
```

### Cenário 2: Partition Network

```bash
# Desconectar node1 da rede cluster
docker network disconnect projeto-sd-2025_cluster node1

# node1 inicia eleição sozinho (não consegue contactar peers)
# node2 e node3 continuam funcionando normalmente

# Reconectar
docker network connect projeto-sd-2025_cluster node1
# node1 recebe heartbeat e volta como Follower
```

### Cenário 3: Stress Test (Concorrência)

```bash
# Abrir 10 terminais e executar simultaneamente:
for i in {1..10}; do
  curl -X POST http://localhost/entrar_fila &
done

# Todos entram na fila com posições diferentes (0-9)

# Tentar comprar o MESMO ingresso simultaneamente:
USER_ID="..."  # Copiar de uma das respostas acima
for i in {1..5}; do
  curl -X POST http://localhost/checkout \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"$USER_ID\",\"ticket_id\":10}" &
done

# Apenas 1 requisição terá sucesso ("Compra realizada com sucesso!")
# As outras receberão 409 Conflict ("Ingresso esgotado")
```

---

## 📊 API Endpoints

| Método | Caminho | Descrição | Quem Processa |
|--------|---------|-----------|---------------|
| `GET` | `/status` | Status do nó (id, role, term, leader_id) | Qualquer nó |
| `GET` | `/read?key=<k>` | Lê chave do store K/V | Qualquer nó |
| `POST` | `/write?key=<k>&value=<v>` | Escreve no store K/V | Líder (followers redirecionam) |
| `POST` | `/entrar_fila` | Adiciona usuário na fila Redis | Qualquer nó |
| `POST` | `/checkout` | Compra ingresso (body JSON) | Líder (followers redirecionam) |

### Exemplo de Body para `/checkout`:

```json
{
  "user_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "ticket_id": 1
}
```

---

## 🔍 Protocolo Inter-Nó (TCP)

Mensagens JSON delimitadas por `\n` na porta `cluster_port`:

| Tipo | Campos | Direção | Quando |
|------|--------|---------|--------|
| `heartbeat` | `leader_id`, `term` | Líder → Followers | A cada 1s |
| `election` | `candidate_id` | Candidato → Peers(ID > candidato) | Timeout de heartbeat |
| `ok` | `from_id` | Peer → Candidato | Resposta a `election` |
| `coordinator` | `leader_id` | Novo líder → Todos | Fim da eleição |
| `replicate` | `key`, `value`, `seq` | Líder → Followers | Escrita K/V |
| `replicate_ack` | `seq`, `from_id` | Follower → Líder | Confirmação de escrita |

---

## 🗄️ Schema do Banco (PostgreSQL)

```sql
CREATE TABLE ingressos (
    id SERIAL PRIMARY KEY,
    nome VARCHAR(100) NOT NULL,
    preco DECIMAL(10, 2) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'disponivel',
    user_id VARCHAR(36),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT status_check CHECK (status IN ('disponivel', 'vendido'))
);

-- Índice para melhorar performance
CREATE INDEX idx_ingressos_status ON ingressos(status);
```

**100 ingressos são criados automaticamente** na inicialização:
- 30 VIP (R$ 150)
- 40 Premium (R$ 100)
- 30 Geral (R$ 50)

---

## ⚙️ Configuração

### Variáveis de Ambiente (via `docker-compose.yml`)

Cada nó aceita:
- `--id <u64>` — ID único (maior vence eleições)
- `--http-port <u16>` — Porta HTTP
- `--cluster-port <u16>` — Porta TCP inter-nó
- `--peers <lista>` — Peers no formato `id:host:porta`
- `--peer-http <lista>` — HTTP dos peers no formato `id:host:porta`
- `--db-url <string>` — URL do PostgreSQL
- `--redis-url <string>` — URL do Redis

### Nginx (Load Balancer)

```nginx
upstream backend {
    least_conn;  # Distribui para o nó com menos conexões ativas
    server node1:8001;
    server node2:8002;
    server node3:8003;
}
```

---

## 🧩 Extensões Possíveis

1. **Endpoint `/listar_ingressos`** no backend (retorna ingressos disponíveis)
2. **WebSocket** para atualizar fila em tempo real
3. **Raft** em vez de Bully (mais robusto para clusters grandes)
4. **Prometheus + Grafana** para métricas (latência, taxa de eleições)
5. **TLS** nas comunicações inter-nó
6. **Multi-região** com replicação assíncrona

---

## 📝 Licença

MIT

---

## 🤝 Contribuindo

1. Fork o projeto
2. Crie uma branch (`git checkout -b feature/nova-funcionalidade`)
3. Commit suas mudanças (`git commit -am 'Adiciona nova funcionalidade'`)
4. Push para a branch (`git push origin feature/nova-funcionalidade`)
5. Abra um Pull Request

---

## 📚 Referências

- [The Bully Algorithm](https://en.wikipedia.org/wiki/Bully_algorithm)
- [Redis Sorted Sets](https://redis.io/docs/data-types/sorted-sets/)
- [PostgreSQL Row-Level Locking](https://www.postgresql.org/docs/current/explicit-locking.html)
- [Axum Web Framework](https://github.com/tokio-rs/axum)
- [Raft Consensus](https://raft.github.io/)
