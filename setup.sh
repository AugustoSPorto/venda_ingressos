#!/bin/bash
# Script de setup automatizado para o sistema de ingressos distribuído
# Usage: ./setup.sh

set -e  # Exit on error

echo "🚀 Setup do Sistema de Ingressos Distribuído"
echo "=============================================="
echo ""

# Cores para output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Função para print colorido
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Verificar se Docker está instalado
echo "Verificando dependências..."
if ! command -v docker &> /dev/null; then
    print_error "Docker não encontrado! Instale: https://docs.docker.com/get-docker/"
    exit 1
fi
print_success "Docker instalado"

if ! command -v docker compose &> /dev/null; then
    print_error "Docker Compose não encontrado!"
    exit 1
fi
print_success "Docker Compose instalado"

echo ""
echo "📁 Verificando estrutura de arquivos..."

# Verificar arquivos essenciais
FILES=(
    "Cargo.toml"
    "Dockerfile"
    "docker-compose.yml"
    "nginx.conf"
    "init.sql"
    "src/main.rs"
    "src/node.rs"
    "src/election.rs"
    "src/message.rs"
    "src/sync.rs"
)

MISSING_FILES=()
for file in "${FILES[@]}"; do
    if [ ! -f "$file" ]; then
        MISSING_FILES+=("$file")
    fi
done

if [ ${#MISSING_FILES[@]} -ne 0 ]; then
    print_error "Arquivos faltando:"
    for file in "${MISSING_FILES[@]}"; do
        echo "  - $file"
    done
    echo ""
    echo "Execute este script na pasta raiz do projeto (projeto-sd-2025/)"
    exit 1
fi

print_success "Todos os arquivos encontrados"

echo ""
echo "🐳 Iniciando containers..."
echo "Isso pode levar alguns minutos na primeira vez (download de imagens)..."
echo ""

# Parar containers antigos se existirem
docker compose down 2>/dev/null || true

# Build e start
docker compose up --build -d

echo ""
print_success "Containers iniciados!"

echo ""
echo "⏳ Aguardando serviços ficarem prontos..."

# Aguardar PostgreSQL
echo -n "  Postgres: "
for i in {1..30}; do
    if docker exec postgres pg_isready -U admin &>/dev/null; then
        print_success "OK"
        break
    fi
    echo -n "."
    sleep 1
    if [ $i -eq 30 ]; then
        print_error "Timeout!"
        exit 1
    fi
done

# Aguardar Redis
echo -n "  Redis: "
for i in {1..30}; do
    if docker exec redis redis-cli ping &>/dev/null; then
        print_success "OK"
        break
    fi
    echo -n "."
    sleep 1
    if [ $i -eq 30 ]; then
        print_error "Timeout!"
        exit 1
    fi
done

# Aguardar Nós (verificar se estão respondendo HTTP)
for node in 1 2 3; do
    echo -n "  Node $node: "
    for i in {1..30}; do
        if curl -s http://localhost:800$node/status &>/dev/null; then
            print_success "OK"
            break
        fi
        echo -n "."
        sleep 1
        if [ $i -eq 30 ]; then
            print_error "Timeout!"
            exit 1
        fi
    done
done

# Aguardar eleição de líder
echo -n "  Eleição: "
sleep 5  # Dar tempo para a eleição acontecer
LEADER_ID=$(curl -s http://localhost/status | grep -o '"leader_id":[0-9]*' | cut -d':' -f2)
if [ ! -z "$LEADER_ID" ]; then
    print_success "Node $LEADER_ID é o líder"
else
    print_warning "Líder ainda não eleito (aguarde mais alguns segundos)"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🎉 Sistema pronto para uso!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "📊 Status dos serviços:"
echo "  • PostgreSQL:    http://localhost:5432"
echo "  • Redis:         http://localhost:6379"
echo "  • Node 1 (HTTP): http://localhost:8001"
echo "  • Node 2 (HTTP): http://localhost:8002"
echo "  • Node 3 (HTTP): http://localhost:8003"
echo "  • Nginx (LB):    http://localhost"
echo ""
echo "🧪 Testes rápidos:"
echo ""
echo "  1. Ver status do cluster:"
echo "     curl http://localhost/status"
echo ""
echo "  2. Entrar na fila:"
echo "     curl -X POST http://localhost/entrar_fila"
echo ""
echo "  3. Comprar ingresso (substitua USER_ID):"
echo "     curl -X POST http://localhost/checkout \\"
echo "       -H 'Content-Type: application/json' \\"
echo "       -d '{\"user_id\":\"USER_ID\",\"ticket_id\":1}'"
echo ""
echo "  4. Testar failover (matar líder):"
echo "     docker stop node$LEADER_ID"
echo "     sleep 5"
echo "     curl http://localhost/status  # Novo líder!"
echo ""
echo "📚 Ver logs:"
echo "     docker compose logs -f          # Todos"
echo "     docker logs -f node1            # Node específico"
echo ""
echo "🛑 Parar tudo:"
echo "     docker compose down             # Parar"
echo "     docker compose down -v          # Parar + apagar dados"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
