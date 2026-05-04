# Rust HTTP App — Deploy en cima20paas

Template de despliegue para aplicaciones Rust HTTP (Axum / Actix) con Traefik
como reverse proxy en el servidor **cima20paas**.

---

## Arquitectura

```
Internet → Hetzner Firewall (80/443) → Traefik → :8080 → Rust App (Axum)
```

- **Traefik** gestiona SSL (Let's Encrypt) y el routing por dominio.
- La app corre como usuario no-root (`1000:1000`) con recursos limitados.
- Los datos persistentes van en un volumen Docker nombrado (`app-data`).

---

## Requisitos previos

- Acceso SSH a `root@cima20paas` (via Tailscale o IP directa).
- Docker y Docker Compose instalados en cima20paas.
- Tu app Rust debe exponer un endpoint `GET /health` que devuelva `200`.
- El nombre del binario en `Cargo.toml` debe coincidir con `APP_NAME`.

---

## Pasos de deploy

### 1. Crear la red Docker frontend

Si es la primera vez que despliegas algo con Traefik en cima20paas:

```bash
ssh root@cima20paas
docker network create MYAPP-frontend
```

> Si la red ya existe, Docker mostrará un aviso — es seguro ignorarlo.

### 2. Conectar Traefik a la red frontend

El contenedor de Traefik debe estar en la misma red que tus apps. Verifica:

```bash
docker network connect MYAPP-frontend traefik
```

> Sustituye `traefik` por el nombre real del contenedor Traefik si es diferente.
> Si Traefik ya está conectado, Docker mostrará un error — ignorar.

### 3. Copiar `.env.example` a `.env` y editar

En tu máquina local (o directamente en cima20paas):

```bash
cd /path/to/rust-http-app
cp .env.example .env
nano .env   # o tu editor preferido
```

**Variables obligatorias que debes cambiar:**

- `APP_NAME` → nombre de tu app (debe coincidir con el binario de Cargo.toml).
- `DOMAIN` → dominio público (ej: `miapp.cima20.io`).
- `TRAEFIK_HOST` → mismo valor que `DOMAIN`.
- `TRAEFIK_NETWORK` → nombre de la red frontend creada en paso 1.
- `COMPOSE_PROJECT_NAME` → identificador único (se usa para nombrar routers Traefik).
- `DATABASE_URL` → si tu app usa base de datos.

### 4. Construir la imagen

**Opción A: Build en cima20paas (recomendado para primera vez)**

```bash
# Transferir código al servidor
scp -r . root@cima20paas:/opt/miapp/
ssh root@cima20paas
cd /opt/miapp
docker compose build
```

**Opción B: Pull desde un registry (GHCR / Docker Hub)**

```bash
# En .env, establecer:
# DOCKER_IMAGE=ghcr.io/tu-user/miapp:latest
docker compose pull
```

### 5. Levantar el servicio

```bash
docker compose up -d
```

Verifica que el contenedor está corriendo:

```bash
docker compose ps
# Debe mostrar estado "Up" y eventualmente "healthy"
```

Revisa los logs:

```bash
docker compose logs -f
```

### 6. Verificar con curl

Desde cima20paas:

```bash
curl -sf http://localhost:8080/health
# Debe responder: {"status":"ok"} o similar
```

Desde fuera (con dominio configurado):

```bash
curl -sf https://miapp.cima20.io/health
# Debe responder igual, con SSL válido
```

---

## Troubleshooting

### Dokploy sobreescribe los labels de Traefik

Si usas Dokploy para gestionar el deploy, **Dokploy inyecta sus propios labels**
de Traefik que pueden sobreescribir los del `docker-compose.yml`.

**Solución:**
- No uses labels manuales de Traefik si Dokploy gestiona el dominio.
- O bien: configura el dominio en la UI de Dokploy y elimina los labels del
  `docker-compose.yml`.
- Si necesitas configuraciones custom (middlewares, TLS options), créalos como
  archivos dinámicos de Traefik (`/etc/traefik/dynamic/`) en lugar de labels.

### Hetzner firewall solo permite 80/443

El firewall de Hetzner en cima20paas tiene abiertos **solo los puertos 80 y 443**.

- No intentes acceder directamente al puerto de la app (ej: `8080`) desde fuera.
- Todo el tráfico externo debe pasar por Traefik (puertos 80 → redirect HTTPS → 443).
- Para debug desde fuera, usa túneles SSH o Tailscale.

**Acceso interno directo:**

```bash
ssh root@cima20paas
curl http://localhost:8080/health
```

### El contenedor no arranca

```bash
# Ver logs detallados
docker compose logs --tail=50

# Errores comunes:
# - "permission denied" → revisar APP_UID/APP_GID
# - "address already in use" → otro contenedor usa el mismo puerto
# - "binary not found" → APP_NAME no coincide con el binario compilado
```

### Traefik no redirige al contenedor

1. Verificar que la red frontend es la misma:
   ```bash
   docker network inspect MYAPP-frontend | grep -A5 container_name
   ```

2. Verificar que Traefik ve el contenedor:
   ```bash
   docker logs traefik 2>&1 | grep "rust-http-app"
   ```

3. Verificar DNS: el dominio debe apuntar a la IP de cima20paas:
   ```bash
   dig +short miapp.cima20.io
   ```

### SSL no funciona (certificado no generado)

- Traefik necesita **puerto 80 accesible** para el challenge HTTP-01 de Let's Encrypt.
- Verificar que el dominio resuelve a la IP pública correcta.
- Revisar logs de Traefik: `docker logs traefik 2>&1 | grep -i "acme\|tls\|cert"`

### Cambiar el nombre del router Traefik

Si despliegas múltiples apps y necesitas routers con nombres únicos, usa
`TRAEFIK_ROUTER_NAME` en `.env`. Los labels en `docker-compose.yml` usan
`${COMPOSE_PROJECT_NAME}` como prefijo — cámbialo ahí.

---

## Estructura del template

```
rust-http-app/
├── .env.example          # Variables de entorno (copiar a .env)
├── docker-compose.yml    # Compose con Traefik labels
├── Dockerfile            # Multi-stage build (Rust → Debian slim)
└── README.md             # Esta guía
```

---

## Comandos útiles en cima20paas

```bash
# Ver estado de todos los contenedores
docker compose ps

# Logs en tiempo real
docker compose logs -f

# Reiniciar sin rebuild
docker compose restart

# Rebuild completo (sin cache)
docker compose build --no-cache && docker compose up -d

# Parar y eliminar
docker compose down

# Limpiar imágenes huérfanas
docker system prune -f
```

---

## Variables de entorno (referencia rápida)

- `APP_NAME` — Nombre de la app / binario Cargo
- `COMPOSE_PROJECT_NAME` — Prefijo para recursos Docker
- `DOMAIN` / `TRAEFIK_HOST` — Dominio público
- `PORT` / `APP_PORT` — Puerto interno de la app
- `TRAEFIK_ROUTER_NAME` — Nombre del router en Traefik
- `TRAEFIK_NETWORK` — Red Docker compartida con Traefik
- `DOCKER_IMAGE` — Imagen Docker (registry o build local)
- `APP_UID` / `APP_GID` — Usuario no-root en el contenedor
- `APP_DATA_DIR` — Directorio de datos persistentes
- `RUST_LOG` — Nivel de logging
- `DATABASE_URL` — Connection string de base de datos
