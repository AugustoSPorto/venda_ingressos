#!/bin/bash
# Script de testes automatizados
# Usage: ./test.sh

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

print_test() {
    echo -e "${BLUE}━━━ $1${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}ℹ $1${NC}"
}

echo "🧪 Executando Testes do Sistema Distribuído"
echo "============================================="
echo ""

# Teste 1: Status do cluster
print_test "Teste 1: Status do Cluster"
STATUS=$(curl -s http://localhost/status)
LEADER_ID=$(echo $STATUS | grep -o '"leader_id":[0-9]*' | cut -d':' -f2)
echo "Response: $STATUS"
if [ ! -z "$LEADER_ID" ]; then
    print_success "Cluster operacional. Líder: Node $LEADER_ID"
else
    print_error "Cluster sem líder!"
    exit 1
fi
echo ""

# Teste 2: Entrar na fila
print_test "Teste 2: Entrar na Fila"
FILA_RESPONSE=$(curl -s -X POST http://localhost/entrar_fila)
USER_ID=$(echo $FILA_RESPONSE | grep -o '"user_id":"[^"]*' | cut -d'"' -f4)
POSICAO=$(echo $FILA_RESPONSE | grep -o '"posicao":[0-9]*' | cut -d':' -f2)
echo "Response: $FILA_RESPONSE"
if [ ! -z "$USER_ID" ]; then
    print_success "Entrou na fila. User ID: $USER_ID, Posição: $POSICAO"
else
    print_error "Falha ao entrar na fila!"
    exit 1
fi
echo ""

# Teste 3: Comprar ingresso
print_test "Teste 3: Comprar Ingresso #1"
CHECKOUT_RESPONSE=$(curl -s -X POST http://localhost/checkout \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"$USER_ID\",\"ticket_id\":1}")
echo "Response: $CHECKOUT_RESPONSE"
if [[ "$CHECKOUT_RESPONSE" == *"sucesso"* ]]; then
    print_success "Compra realizada com sucesso!"
else
    print_error "Falha na compra: $CHECKOUT_RESPONSE"
    exit 1
fi
echo ""

# Teste 4: Double booking (deve falhar)
print_test "Teste 4: Tentativa de Double Booking (deve falhar)"
DOUBLE_RESPONSE=$(curl -s -w "%{http_code}" -X POST http://localhost/checkout \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"$USER_ID\",\"ticket_id\":1}")
HTTP_CODE="${DOUBLE_RESPONSE: -3}"
if [ "$HTTP_CODE" == "409" ]; then
    print_success "Double booking bloqueado corretamente! (HTTP 409)"
else
    print_error "Double booking NÃO foi bloqueado! (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

# Teste 5: Redirecionamento (Follower -> Leader)
print_test "Teste 5: Redirecionamento (Follower → Leader)"
# Criar novo user
FILA_RESPONSE2=$(curl -s -X POST http://localhost/entrar_fila)
USER_ID2=$(echo $FILA_RESPONSE2 | grep -o '"user_id":"[^"]*' | cut -d'"' -f4)

# Descobrir um follower (node != leader)
FOLLOWER_PORT=8001
if [ "$LEADER_ID" == "1" ]; then
    FOLLOWER_PORT=8002
fi

# Fazer request no follower e seguir redirect
REDIRECT_RESPONSE=$(curl -s -L -X POST http://localhost:$FOLLOWER_PORT/checkout \
    -H "Content-Type: application/json" \
    -d "{\"user_id\":\"$USER_ID2\",\"ticket_id\":5}")
echo "Response: $REDIRECT_RESPONSE"
if [[ "$REDIRECT_RESPONSE" == *"sucesso"* ]]; then
    print_success "Redirecionamento funcionou! Follower → Leader"
else
    print_error "Redirecionamento falhou!"
    exit 1
fi
echo ""

# Teste 6: Concorrência
print_test "Teste 6: Teste de Concorrência (5 requests simultâneos)"
print_info "Tentando comprar o mesmo ingresso 5 vezes..."

# Criar 5 usuários
declare -a USER_IDS
for i in {1..5}; do
    RESP=$(curl -s -X POST http://localhost/entrar_fila)
    UID=$(echo $RESP | grep -o '"user_id":"[^"]*' | cut -d'"' -f4)
    USER_IDS+=("$UID")
done

# Tentar comprar ingresso #10 simultaneamente
SUCCESS_COUNT=0
CONFLICT_COUNT=0
for uid in "${USER_IDS[@]}"; do
    HTTP_CODE=$(curl -s -w "%{http_code}" -o /dev/null -X POST http://localhost/checkout \
        -H "Content-Type: application/json" \
        -d "{\"user_id\":\"$uid\",\"ticket_id\":10}") &
done
wait

# Verificar no banco quantas vendas foram feitas
sleep 2
SOLD_COUNT=$(docker exec -i postgres psql -U admin -d ingressos -t -c \
    "SELECT COUNT(*) FROM ingressos WHERE id = 10 AND status = 'vendido';" | tr -d ' ')

if [ "$SOLD_COUNT" == "1" ]; then
    print_success "Concorrência OK! Apenas 1 venda (sem double booking)"
else
    print_error "Double booking detectado! Vendas: $SOLD_COUNT (esperado: 1)"
    exit 1
fi
echo ""

# Teste 7: Estatísticas do banco
print_test "Teste 7: Estatísticas do Sistema"
STATS=$(docker exec -i postgres psql -U admin -d ingressos -t -c \
    "SELECT * FROM estatisticas_vendas;")
echo "Estatísticas:"
echo "$STATS"
print_success "Banco de dados operacional"
echo ""

# Teste 8: Fila Redis
print_test "Teste 8: Fila Redis"
QUEUE_SIZE=$(docker exec -i redis redis-cli ZCARD fila_ingressos)
echo "Tamanho da fila: $QUEUE_SIZE"
if [ "$QUEUE_SIZE" -ge "0" ]; then
    print_success "Redis operacional"
else
    print_error "Redis com problemas!"
    exit 1
fi
echo ""

# Teste 9: Failover (OPCIONAL - comentado por padrão)
# Descomente as linhas abaixo para testar failover automaticamente
# print_test "Teste 9: Failover (Matar Líder)"
# print_info "Matando node $LEADER_ID..."
# docker stop node$LEADER_ID
# sleep 6  # Aguardar nova eleição
# NEW_STATUS=$(curl -s http://localhost/status)
# NEW_LEADER=$(echo $NEW_STATUS | grep -o '"leader_id":[0-9]*' | cut -d':' -f2)
# if [ "$NEW_LEADER" != "$LEADER_ID" ]; then
#     print_success "Novo líder eleito: Node $NEW_LEADER"
# else
#     print_error "Failover falhou!"
#     exit 1
# fi
# print_info "Religando node $LEADER_ID..."
# docker start node$LEADER_ID
# sleep 3
# echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo -e "${GREEN}✓ TODOS OS TESTES PASSARAM!${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "📊 Resumo:"
echo "  • Cluster operacional com líder eleito"
echo "  • Fila Redis funcionando"
echo "  • Sistema de compras bloqueando double booking"
echo "  • Redirecionamento follower→leader OK"
echo "  • Banco de dados consistente"
echo ""
echo "💡 Para testar failover manualmente:"
echo "   docker stop node$LEADER_ID"
echo "   sleep 5"
echo "   curl http://localhost/status"
echo ""
