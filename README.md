# Solana MEV Bot

Bot de extracción de valor máximo (MEV) para la red Solana, diseñado para detectar y aprovechar oportunidades de front-running y sniping en tiempo real.

## Características

- ⚡ **Detección en tiempo real**: Monitorea el mempool de Solana para identificar oportunidades MEV
- 🚀 **Integración con Jito**: Envia transacciones con prioridad para mayor éxito en frontrun
- 💰 **Transacciones con propina (tip)**: Incluye transacciones de propina a cuentas de Jito para ser elegible en subastas
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

# Configuración de Jito (para transacciones prioritarias)
USE_JITO=true
JITO_RPC_URL=https://testnet.block-engine.jito.wtf/api/v1/bundles

# Cuentas de tip recomendadas por Jito (para Devnet) - No es necesario configurar manualmente
# El bot selecciona automáticamente una cuenta de tip para cumplir con los requisitos de Jito
# JITO_TIP_ACCOUNT=96gYZGLnJYVFJZpLUWK4JGsRU1uKiuN5Mjfn4xh3F933

# Estrategias MEV para Solana
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
  
  # Para Jito en Mainnet
  JITO_RPC_URL=https://mainnet.block-engine.jito.wtf/api/v1/bundles
  ```

## Cómo obtener acceso a Jito para Mainnet

### 1. Aplicar al programa MEV de Jito:

Para usar Jito en Mainnet con autenticación completa:

1. Visita: https://www.jito.wtf/
2. Busca el programa de "Searcher Registration" o "MEV Program"
3. Completa el formulario de aplicación
4. Espera aprobación (puede tomar varios días)
5. Recibirás un token de autenticación (UUID)

### 2. Actualizar la configuración:

Después de obtener acceso, actualiza tu `.env`:

```
# Mainnet con Jito autenticado
NETWORK=mainnet
USE_JITO=true
JITO_RPC_URL=https://mainnet.block-engine.jito.wtf/api/v1/bundles
JITO_AUTH_HEADER=Bearer tu_uuid_real_aqui
```

### 3. Configuración de cuentas de tip (Mainnet)

Para mainnet, puedes usar cualquiera de estas cuentas de tip recomendadas por Jito:

```
JITO_TIP_ACCOUNT=96gYZGLnJYVFJZpLUWK4JGsRU1uKiuN5Mjfn4xh3F933
# O cualquiera de estas otras:
# HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe
# Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY
# ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49
# DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh
# ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt
# DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL
# 3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT
```

### 4. Importante: Funcionalidad de propina (tip) implementada

El bot ahora incluye automáticamente transacciones de propina (tip) en los bundles de Jito para cumplir con los requisitos de elegibilidad para la subasta de Jito. El bot selecciona aleatoriamente una de las cuentas de propina conocidas de Jito para cada bundle que envía.

## Consideraciones de seguridad para Mainnet

- **Guarda tu clave privada con extrema seguridad**
- **Haz copias de seguridad del archivo de clave**
- **No compartas nunca el archivo de clave privada**
- **Considera usar una billetera hardware si es posible**
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