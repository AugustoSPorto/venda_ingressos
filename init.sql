-- Schema de inicialização para o banco de ingressos

CREATE TABLE IF NOT EXISTS ingressos (
    id SERIAL PRIMARY KEY,
    nome VARCHAR(100) NOT NULL,
    preco DECIMAL(10, 2) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'disponivel',
    user_id VARCHAR(36),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT status_check CHECK (status IN ('disponivel', 'vendido'))
);

-- Criar índice para melhorar performance de queries por status
CREATE INDEX idx_ingressos_status ON ingressos(status);

-- Inserir 100 ingressos disponíveis para teste
INSERT INTO ingressos (nome, preco, status)
SELECT 
    'Ingresso Setor ' || (CASE 
        WHEN i <= 30 THEN 'VIP'
        WHEN i <= 70 THEN 'Premium'
        ELSE 'Geral'
    END) || ' #' || i,
    (CASE 
        WHEN i <= 30 THEN 150.00
        WHEN i <= 70 THEN 100.00
        ELSE 50.00
    END),
    'disponivel'
FROM generate_series(1, 100) AS i;

-- Criar view para estatísticas
CREATE OR REPLACE VIEW estatisticas_vendas AS
SELECT 
    COUNT(*) FILTER (WHERE status = 'disponivel') AS disponiveis,
    COUNT(*) FILTER (WHERE status = 'vendido') AS vendidos,
    SUM(preco) FILTER (WHERE status = 'vendido') AS receita_total
FROM ingressos;
