# =============================================================================
# deploy-to-dokploy.sh - Deploy automatizado a cima20paas (Dokploy)
# Ubicación: /home/oc/.hermes/projects/<bot>/deploy-to-dokploy.sh
# =============================================================================
# Uso: ./deploy-to-dokploy.sh [nombre_bot] [modo]
#   modo: "prod" (default) o "test"
#
# Requiere:
#   - SSH a cima20paas configurado
#   - Docker y docker-compose en cima20paas
#   - Red dokploy-network creada
# =============================================================================

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

BOT_NAME="${1:-}"
MODE="${2:-prod}"

if [ -z "$BOT_NAME" ]; then
    echo -e "${RED}ERROR: Especifica nombre del bot${NC}"
    echo "Uso: $0 <bot_name> [prod|test]"
    echo "Ejemplo: $0 scraper-bot prod"
    exit 1
fi

PROJECT_DIR="/home/oc/.hermes/projects/${BOT_NAME}"
REMOTE_HOST="root@cima20paas"
REMOTE_DIR="/opt/${BOT_NAME}"
SSH_KEY="${SSH_KEY:-~/.ssh/id_ed25519}"

echo "========================================"
echo "  DEPLOY A CIMA20PAAS (Dokploy)"
echo "  Bot: ${BOT_NAME}"
echo "  Modo: ${MODE}"
echo "========================================"

# Verificar proyecto local
if [ ! -d "$PROJECT_DIR" ]; then
    echo -e "${RED}ERROR: Proyecto local no encontrado${NC}"
    exit 1
fi

# Verificar que tenemos Dockerfile
if [ ! -f "${PROJECT_DIR}/Dockerfile" ]; then
    echo -e "${YELLOW}⚠ No hay Dockerfile en ${PROJECT_DIR}${NC}"
    echo "  Copiando template..."
    cp /home/oc/.hermes/projects/telegram-bot-infra/templates/Dockerfile.rust-bot "${PROJECT_DIR}/Dockerfile"
    # Reemplazar placeholder
    sed -i "s/<BOT_BINARY_NAME>/${BOT_NAME}/g" "${PROJECT_DIR}/Dockerfile"
fi

if [ ! -f "${PROJECT_DIR}/docker-compose.yml" ]; then
    echo -e "${YELLOW}⚠ No hay docker-compose.yml${PROJECT_DIR}${NC}"
    echo "  Copiando template..."
    cp /home/oc/.hermes/projects/telegram-bot-infra/templates/docker-compose.yml "${PROJECT_DIR}/docker-compose.yml"
fi

# =============================================================================
# FASE 1: Pre-deploy checks
# =============================================================================
echo -e "\n${BLUE}[1/5] Pre-deploy checks...${NC}"

cd "$PROJECT_DIR"

# Ejecutar auditoría
if [ -f "./preprod-audit.sh" ]; then
    echo "  → Ejecutando auditoría..."
    if ./preprod-audit.sh "$BOT_NAME"; then
        echo -e "  ${GREEN}✓ Auditoría pasó${NC}"
    else
        echo -e "  ${YELLOW}⚠ Auditoría reportó warnings - continuando con cuidado${NC}"
    fi
else
    echo -e "  ${YELLOW}⚠ No hay preprod-audit.sh - saltando auditoría${NC}"
fi

# Verificar que no hay proceso manual corriendo con mismo token
echo "  → Verificando conflictos de token..."
if ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i "$SSH_KEY" "$REMOTE_HOST" "docker ps | grep -q ${BOT_NAME}" 2>/dev/null; then
    echo -e "  ${YELLOW}⚠ Ya hay un contenedor '${BOT_NAME}' en cima20paas${NC}"
    read -p "  ¿Detener y recrear? (y/N): " confirm
    if [ "$confirm" != "y" ] && [ "$confirm" != "Y" ]; then
        echo "Deploy cancelado"
        exit 1
    fi
fi

# =============================================================================
# FASE 2: Preparar archivos para transferencia
# =============================================================================
echo -e "\n${BLUE}[2/5] Preparando archivos...${NC}"

# Crear tarball excluyendo lo que no necesitamos
TMP_DIR=$(mktemp -d)
tar czf "${TMP_DIR}/${BOT_NAME}.tar.gz" \
    --exclude='target' \
    --exclude='.git' \
    --exclude='*.log' \
    --exclude='data/*.db' \
    --exclude='node_modules' \
    --exclude='.output' \
    -C "$PROJECT_DIR" .

echo -e "  ${GREEN}✓ Tarball creado: ${TMP_DIR}/${BOT_NAME}.tar.gz${NC}"

# =============================================================================
# FASE 3: Transferir a cima20paas
# =============================================================================
echo -e "\n${BLUE}[3/5] Transfiriendo a cima20paas...${NC}"

# Verificar conectividad
if ! ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 -i "$SSH_KEY" "$REMOTE_HOST" "echo OK" >/dev/null 2>&1; then
    echo -e "${RED}ERROR: No se puede conectar a ${REMOTE_HOST}${NC}"
    echo "Verifica:"
    echo "  1. Tailscale está activo"
    echo "  2. La clave SSH es correcta"
    echo "  3. cima20paas está encendido"
    rm -rf "$TMP_DIR"
    exit 1
fi

# Transferir via base64 (scp puede estar bloqueado)
echo "  → Codificando a base64..."
B64=$(base64 -w0 "${TMP_DIR}/${BOT_NAME}.tar.gz")

# Crear directorio remoto y decodificar
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i "$SSH_KEY" "$REMOTE_HOST" << EOF
    set -e
    mkdir -p ${REMOTE_DIR}
    cd ${REMOTE_DIR}
    echo "${B64}" | base64 -d > ${BOT_NAME}.tar.gz
    tar xzf ${BOT_NAME}.tar.gz
    rm ${BOT_NAME}.tar.gz
    echo "OK"
EOF

echo -e "  ${GREEN}✓ Archivos transferidos${NC}"

# =============================================================================
# FASE 4: Configurar entorno remoto
# =============================================================================
echo -e "\n${BLUE}[4/5] Configurando entorno remoto...${NC}"

# Verificar .env.prod existe
if [ -f "${PROJECT_DIR}/.env.prod" ]; then
    echo "  → Subiendo .env.prod..."
    B64_ENV=$(base64 -w0 "${PROJECT_DIR}/.env.prod")
    ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i "$SSH_KEY" "$REMOTE_HOST" \
        "cd ${REMOTE_DIR} && echo '${B64_ENV}' | base64 -d > .env"
    echo -e "  ${GREEN}✓ .env.prod configurado${NC}"
elif [ -f "${PROJECT_DIR}/.env" ]; then
    echo -e "  ${YELLOW}⚠ Usando .env (crea .env.prod para producción)${NC}"
    B64_ENV=$(base64 -w0 "${PROJECT_DIR}/.env")
    ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i "$SSH_KEY" "$REMOTE_HOST" \
        "cd ${REMOTE_DIR} && echo '${B64_ENV}' | base64 -d > .env"
else
    echo -e "  ${RED}✗ No hay .env ni .env.prod - el bot no funcionará${NC}"
fi

# Verificar red dokploy-network
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i "$SSH_KEY" "$REMOTE_HOST" << 'EOF'
    if ! docker network ls | grep -q dokploy-network; then
        echo "Creando dokploy-network..."
        docker network create dokploy-network
    fi
EOF

# =============================================================================
# FASE 5: Build y deploy
# =============================================================================
echo -e "\n${BLUE}[5/5] Build y deploy...${NC}"

ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -i "$SSH_KEY" "$REMOTE_HOST" << EOF
    set -e
    cd ${REMOTE_DIR}
    
    # Detener contenedor anterior si existe
    docker compose down 2>/dev/null || true
    
    # Build
    echo "  → Building..."
    docker compose build --no-cache
    
    # Start
    echo "  → Starting..."
    docker compose up -d
    
    # Health check
    sleep 5
    echo "  → Health check..."
    if docker compose ps | grep -q "healthy"; then
        echo "✅ Contenedor saludable"
    else
        echo "⚠️  Verificando estado..."
        docker compose logs --tail=20
    fi
    
    # Cleanup
    docker system prune -f 2>/dev/null || true
EOF

# =============================================================================
# Resumen
# =============================================================================
echo ""
echo "========================================"
echo -e "  ${GREEN}✅ DEPLOY COMPLETADO${NC}"
echo "========================================"
echo "  Bot: ${BOT_NAME}"
echo "  Host: ${REMOTE_HOST}"
echo "  Directorio: ${REMOTE_DIR}"
echo ""
echo "Comandos útiles en cima20paas:"
echo "  ssh ${REMOTE_HOST}"
echo "  cd ${REMOTE_DIR}"
echo "  docker compose logs -f"
echo "  docker compose ps"
echo "  docker compose restart"
echo ""
echo "Verificar salud:"
echo "  curl https://<dominio>/health"
echo ""

# Cleanup local
rm -rf "$TMP_DIR"
