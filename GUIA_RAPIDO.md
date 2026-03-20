# 🚀 Guia Rápido de Instalação

## Estrutura de Diretórios

Organize os arquivos assim:

```
projeto-sd-2025/
├── src/
│   ├── main.rs
│   ├── node.rs
│   ├── election.rs
│   ├── sync.rs
│   └── message.rs
├── frontend/
│   ├── public/
│   │   └── index.html
│   ├── src/
│   │   ├── App.jsx
│   │   └── index.js
│   └── package.json
├── Cargo.toml
├── Dockerfile
├── docker-compose.yml
├── nginx.conf
├── init.sql
└── README_COMPLETO.md
```

## Passos Rápidos

### 1. Backend (Cluster Rust)

```bash
# Na raiz do projeto
docker compose up --build

# Aguarde até ver:
# [node 3]: elected as leader (term 1)
```

### 2. Frontend (React) - OPCIONAL

```bash
cd frontend

# Criar arquivos públicos básicos
mkdir -p public src

# Criar public/index.html
cat > public/index.html << 'EOF'
<!DOCTYPE html>
<html lang="pt-BR">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Sistema de Ingressos</title>
</head>
<body>
  <noscript>Você precisa habilitar JavaScript para executar este app.</noscript>
  <div id="root"></div>
</body>
</html>
EOF

# Criar src/index.js
cat > src/index.js << 'EOF'
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

const root = ReactDOM.createRoot(document.getElementById('root'));
root.render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
EOF

# Mover App.jsx para src/
# (arquivo App.jsx já foi fornecido)

# Instalar e rodar
npm install
npm start
```

### 3. Testar no Terminal

```bash
# Status do cluster
curl http://localhost/status

# Entrar na fila
curl -X POST http://localhost/entrar_fila

# Retorna algo como:
# {"user_id":"abc-123","posicao":0,"node_id":2}

# Comprar ingresso (use o user_id acima)
curl -X POST http://localhost/checkout \
  -H "Content-Type: application/json" \
  -d '{"user_id":"abc-123","ticket_id":1}'
```

## Troubleshooting

### Erro: "failed to bind HTTP port"

**Causa:** Porta já em uso.

**Solução:**
```bash
# Ver o que está usando a porta 8001/8002/8003
lsof -i :8001
# Matar o processo ou mudar a porta no docker-compose.yml
```

### Erro: "Failed to connect to PostgreSQL"

**Causa:** Banco não inicializou ainda.

**Solução:**
```bash
# Aguardar 5-10 segundos e tentar novamente
docker compose down
docker compose up --build
```

### Erro: "connection refused" no Redis

**Causa:** Redis não está rodando.

**Solução:**
```bash
# Verificar logs
docker logs redis

# Reiniciar
docker restart redis
```

### Frontend não conecta ao backend

**Causa:** CORS ou proxy mal configurado.

**Solução:**
```bash
# Verificar se o proxy está em package.json:
# "proxy": "http://localhost"

# Ou testar direto:
curl http://localhost/status
```

## Logs Úteis

```bash
# Ver logs de um nó específico
docker logs -f node1

# Ver logs do Nginx
docker logs -f nginx

# Ver logs do Postgres
docker logs -f postgres

# Ver logs do Redis
docker logs -f redis

# Ver todos os logs
docker compose logs -f
```

## Parar Tudo

```bash
# Parar containers
docker compose down

# Parar E remover volumes (apaga dados do banco)
docker compose down -v
```

## Próximos Passos

1. Ler o `README_COMPLETO.md` para entender a arquitetura
2. Testar os cenários de failover (matar o líder)
3. Testar concorrência (múltiplos checkouts simultâneos)
4. Adicionar métricas com Prometheus
5. Implementar WebSocket para updates em tempo real

---

**Dúvidas?** Leia o README_COMPLETO.md ou abra uma issue no repositório.
