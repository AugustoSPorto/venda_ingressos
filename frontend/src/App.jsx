import React, { useState, useEffect } from 'react';

const API_URL = 'http://localhost:8003';

function App() {
  const [userId, setUserId] = useState('');
  const [posicao, setPosicao] = useState(null);
  const [nodeId, setNodeId] = useState(null);
  const [ingressos, setIngressos] = useState([]);
  const [status, setStatus] = useState({});
  const [loading, setLoading] = useState(false);

  // Buscar ingressos disponíveis
  const fetchIngressos = async () => {
    try {
      // Nota: você precisará criar um endpoint /listar_ingressos no backend
      // Por enquanto, vamos simular alguns ingressos
      const mock = Array.from({ length: 10 }, (_, i) => ({
        id: i + 1,
        nome: `Ingresso ${i + 1}`,
        preco: 50 + i * 10,
        status: 'disponivel'
      }));
      setIngressos(mock);
    } catch (error) {
      console.error('Erro ao buscar ingressos:', error);
    }
  };

  // Buscar status dos nós
  const fetchStatus = async () => {
    try {
      const res = await fetch(`${API_URL}/status`);
      const data = await res.json();
      setStatus(data);
    } catch (error) {
      console.error('Erro ao buscar status:', error);
    }
  };

  useEffect(() => {
    fetchIngressos();
    fetchStatus();
    const interval = setInterval(fetchStatus, 2000);
    return () => clearInterval(interval);
  }, []);

  // Entrar na fila
  const entrarFila = async () => {
    setLoading(true);
    try {
      const res = await fetch(`${API_URL}/entrar_fila`, {
        method: 'POST',
      });
      const data = await res.json();
      setUserId(data.user_id);
      setPosicao(data.posicao);
      setNodeId(data.node_id);
      alert(`Você entrou na fila! Posição: ${data.posicao + 1}`);
    } catch (error) {
      console.error('Erro ao entrar na fila:', error);
      alert('Erro ao entrar na fila');
    } finally {
      setLoading(false);
    }
  };

  // Comprar ingresso
  const comprarIngresso = async (ticketId) => {
    if (!userId) {
      alert('Você precisa entrar na fila primeiro!');
      return;
    }

    setLoading(true);
    try {
      const res = await fetch(`${API_URL}/checkout`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          user_id: userId,
          ticket_id: ticketId,
        }),
      });

      if (res.ok) {
        alert('Compra realizada com sucesso!');
        fetchIngressos();
        setUserId('');
        setPosicao(null);
      } else if (res.status === 409) {
        alert('Ingresso esgotado!');
      } else if (res.status === 307 || res.status === 308) {
        alert('Redirecionando para o líder...');
      } else {
        alert('Erro ao realizar compra');
      }
    } catch (error) {
      console.error('Erro ao comprar:', error);
      alert('Erro ao realizar compra');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ padding: '20px', maxWidth: '1200px', margin: '0 auto' }}>
      <h1>🎫 Sistema de Venda de Ingressos Distribuído</h1>
      
      {/* Status do Cluster */}
      <div style={{ 
        background: '#f5f5f5', 
        padding: '15px', 
        borderRadius: '8px',
        marginBottom: '20px' 
      }}>
        <h2>📊 Status do Cluster</h2>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '10px' }}>
          <div>
            <strong>Node ID:</strong> {status.id || 'N/A'}
          </div>
          <div>
            <strong>Role:</strong> {status.role || 'N/A'}
          </div>
          <div>
            <strong>Term:</strong> {status.term || 'N/A'}
          </div>
          <div>
            <strong>Leader ID:</strong> {status.leader_id || 'N/A'}
          </div>
        </div>
      </div>

      {/* Fila */}
      <div style={{ 
        background: '#e3f2fd', 
        padding: '15px', 
        borderRadius: '8px',
        marginBottom: '20px' 
      }}>
        <h2>🚶 Fila de Espera</h2>
        {userId ? (
          <div>
            <p><strong>Seu ID:</strong> {userId}</p>
            <p><strong>Posição:</strong> {posicao !== null ? posicao + 1 : 'N/A'}</p>
            <p><strong>Atendido pelo Node:</strong> {nodeId}</p>
          </div>
        ) : (
          <button 
            onClick={entrarFila} 
            disabled={loading}
            style={{
              padding: '10px 20px',
              fontSize: '16px',
              background: '#1976d2',
              color: 'white',
              border: 'none',
              borderRadius: '4px',
              cursor: loading ? 'not-allowed' : 'pointer'
            }}
          >
            {loading ? 'Entrando...' : 'Entrar na Fila'}
          </button>
        )}
      </div>

      {/* Lista de Ingressos */}
      <div>
        <h2>🎟️ Ingressos Disponíveis</h2>
        <div style={{ 
          display: 'grid', 
          gridTemplateColumns: 'repeat(auto-fill, minmax(250px, 1fr))', 
          gap: '15px' 
        }}>
          {ingressos.map((ingresso) => (
            <div 
              key={ingresso.id}
              style={{
                background: 'white',
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '15px',
                boxShadow: '0 2px 4px rgba(0,0,0,0.1)'
              }}
            >
              <h3 style={{ margin: '0 0 10px 0' }}>{ingresso.nome}</h3>
              <p style={{ fontSize: '20px', color: '#1976d2', margin: '10px 0' }}>
                R$ {ingresso.preco.toFixed(2)}
              </p>
              <button
                onClick={() => comprarIngresso(ingresso.id)}
                disabled={loading || !userId}
                style={{
                  width: '100%',
                  padding: '10px',
                  background: userId ? '#4caf50' : '#ccc',
                  color: 'white',
                  border: 'none',
                  borderRadius: '4px',
                  cursor: (loading || !userId) ? 'not-allowed' : 'pointer',
                  fontSize: '14px'
                }}
              >
                {!userId ? 'Entre na fila primeiro' : 'Comprar'}
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Explicação do Sistema */}
      <div style={{ 
        marginTop: '30px', 
        padding: '15px', 
        background: '#fff3e0',
        borderRadius: '8px' 
      }}>
        <h3>🔧 Como funciona?</h3>
        <ul>
          <li><strong>Eleição de Líder:</strong> O nó com maior ID vence (algoritmo Bully)</li>
          <li><strong>Heartbeat:</strong> Líder envia heartbeats a cada 1s</li>
          <li><strong>Fila (Redis):</strong> Gerenciada em sorted set compartilhado</li>
          <li><strong>Checkout:</strong> Apenas o LÍDER processa compras (evita race conditions)</li>
          <li><strong>Consistência Forte:</strong> Líder aguarda ACK de todos os backups</li>
          <li><strong>Load Balancing:</strong> Nginx distribui requisições com least_conn</li>
        </ul>
      </div>
    </div>
  );
}

export default App;
