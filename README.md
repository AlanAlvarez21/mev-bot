# Solana MEV Bot

Bot de extracción de valor máximo (MEV) para la red Solana, diseñado para detectar y aprovechar oportunidades de front-running en tiempo real.

## Características

- ⚡ **Detección en tiempo real**: Monitorea el mempool de Solana para identificar oportunidades MEV
- 🚀 **Integración con Jito**: Envia transacciones con prioridad para mayor éxito en frontrun
- 🔒 **Firmado de transacciones**: Creación de transacciones firmadas con clave privada
- 💰 **Cálculo de rentabilidad**: Evalúa oportunidades para evitar pérdidas
- 🛡️ **Manejo robusto de errores**: Sistema completo de gestión de fallos
- 📊 **Registro detallado**: Todos los eventos y oportunidades son registrados

## Requisitos previos

- Rust (versión 1.70 o superior)
- Una cuenta en Solana Devnet o Mainnet Beta
- Clave privada de billetera Solana (archivo JSON)
- Acceso a un RPC de Solana (público o privado)

## Instalación

1. **Clona el repositorio:**
```bash
git clone [URL_DEL_REPOSITORIO]
cd solana-mev-bot
```

2. **Instala Rust si aún no lo tienes:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

3. **Compila el proyecto:**
```bash
cargo build --release
```

## Configuración

Crea un archivo `.env` en la raíz del proyecto con la siguiente estructura:

```
# Configuración de red
NETWORK=devnet  # o "mainnet" para producción

# Configuración de Solana
SOLANA_RPC_URL=https://api.devnet.solana.com  # Cambia a mainnet si corres en mainnet
SOLANA_WS_URL=wss://api.devnet.solana.com    # Cambia a mainnet si corres en mainnet

# Configuración de Jito
USE_JITO=true
JITO_RPC_URL=https://mainnet.block-engine.jito.wtf/api/v1/bundles  # Para mainnet
JITO_TIP_ACCOUNT=96gYZGLnJYVFJZpLUWK4JGsRU1uKiuN5Mjfn4xh3F933

# Estrategias MEV
STRATEGY=frontrun,snipe
```

## Configuración de billetera

Guarda tu archivo de clave privada de Solana como `solana-keypair.json` en la raíz del proyecto. Puedes generar uno con:

```bash
solana-keygen new --outfile solana-keypair.json --no-passphrase
```

## Modo Devnet vs Mainnet

### Devnet (Para pruebas)

- **RPC URLs**: Usa endpoints de Devnet
- **Saldo**: Puedes obtener SOL gratuito con `solana airdrop`
- **Riesgo**: 0, perfecto para pruebas
- **Configuración típica**:
  ```
  NETWORK=devnet
  SOLANA_RPC_URL=https://api.devnet.solana.com
  SOLANA_WS_URL=wss://api.devnet.solana.com
  ```

### Mainnet (Producción)

- **RPC URLs**: Usa endpoints de Mainnet Beta
- **Saldo**: Solo SOL real, con valor económico
- **Riesgo**: Alto, puedes perder fondos si algo falla
- **Configuración típica**:
  ```
  NETWORK=mainnet
  SOLANA_RPC_URL=https://api.mainnet-beta.solana.com  # O un endpoint RPC privado
  SOLANA_WS_URL=wss://api.mainnet-beta.solana.com
  ```

## Cómo implementar en Mainnet

### 1. Cambios necesarios para Mainnet

#### .env changes:
```
# Cambia NETWORK a mainnet
NETWORK=mainnet

# Usa endpoints de Mainnet
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
SOLANA_WS_URL=wss://api.mainnet-beta.solana.com

# Asegúrate de tener una cuenta con credenciales reales de Jito
JITO_RPC_URL=https://mainnet.block-engine.jito.wtf/api/v1/bundles
# Necesitarás un token de autenticación real de Jito
JITO_AUTH_TOKEN=your_real_jito_auth_token
```

#### Billetera:
- Usa una billetera con fondos reales en Mainnet
- Mantén un saldo seguro para cubrir tarifas y posibles pérdidas durante el aprendizaje

### 2. Consideraciones de seguridad para Mainnet

- **Guarda tu clave privada con extrema seguridad**
- **Haz copias de seguridad del archivo de clave**
- **No compartas nunca el archivo de clave privada**
- **Considera usar una billetera hardware si es posible**

### 3. Consideraciones de riesgo para Mainnet

- **Empieza con pequeñas cantidades**: Haz pruebas con pequeños montos primero
- **Entiende que puedes perder fondos**: Las estrategias MEV no garantizan ganancias
- **Monitorea constantemente**: Supervisa las operaciones en todo momento
- **Prepara sistemas de límite de pérdidas**: Configura controles para detener pérdidas grandes

## Ejecución

1. **Para Devnet:**
```bash
cargo run
```

2. **Para Mainnet (después de configurar correctamente):**
```bash
NETWORK=mainnet cargo run
```

## Cómo funciona

El bot realiza los siguientes pasos:

1. **Monitoreo**: Se conecta al mempool de Solana vía WebSocket para recibir transacciones en tiempo real
2. **Análisis**: Evalúa cada oportunidad para determinar si es rentable
3. **Firmado**: Crea transacciones firmadas usando tu clave privada
4. **Prioridad**: Si está configurado Jito, envía transacciones con prioridad
5. **Ejecución**: Intenta ejecutar estrategias MEV como frontrun o snipe

## Contribuciones

Las contribuciones son bienvenidas. Por favor abre un issue o PR para discutir cambios.

## Advertencia

Este bot opera en mercados altamente competitivos y puede resultar en la pérdida de fondos. Ú️ ¡Úsalo bajo tu propio riesgo! No somos responsables de ninguna pérdida financiera.

## Licencia

MIT