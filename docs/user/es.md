# Pando Assistant: guía de usuario

Pando Assistant es un cliente de escritorio nativo para [Pando](https://github.com/digiogithub/pando).
Muestra tus agentes como un organismo vivo (la célula madre es Mesnada, cada agente es una
célula hija), incluye un chat completo y se maneja con la voz. Todo lo que puede ejecutarse
en tu equipo se ejecuta en tu equipo: reconocimiento de voz (Whistle), síntesis de voz (Piper)
y el modelo que decide si has dado una orden o estás conversando.

> Estado: las partes marcadas **(planificado)** están en el plan pero todavía no están
> conectadas en la aplicación. Esta guía describe lo que existe hoy.

## 1. Instalación

Necesitas un Pando en marcha (`pando serve`, `pando app` o `pando desktop`) en el mismo
equipo. Sin Pando puedes probar el organismo con `pando-assistant --demo`.

### Linux (Ubuntu 24.04, Debian 12 o superior)

- **.deb**: `sudo apt install ./pando-assistant_<versión>_amd64.deb`
- **AppImage**: `chmod +x Pando_Assistant_*.AppImage && ./Pando_Assistant_*.AppImage`
- **Flatpak**: `flatpak install dev.pando.assistant` (cuando esté publicado).

Requisitos: Vulkan (`libvulkan1` y el controlador de tu GPU) y PipeWire o PulseAudio para el
micrófono. Sin GPU compatible la aplicación usa un modo de dibujo por CPU, más lento.

### macOS (Apple Silicon, macOS 12 o superior)

Abre el `.dmg` y arrastra la aplicación a Aplicaciones. En Mac Intel no existe motor Whistle:
se usará el reconocimiento alternativo (Parakeet), más pesado. **(planificado)**

### Windows 10/11 (64 bits)

Ejecuta el instalador `.exe` (NSIS) o el `.msi`. Elige idioma español o inglés.

## 2. Primer arranque

1. Inicia Pando. El asistente lo descubre solo y lo muestra en la barra superior (chip de
   conexión). Si Pando está en otra dirección, indícala en los ajustes (`pando_url`).
2. El idioma sigue al del sistema. Puedes forzar `es` o `en` en `settings.json`.
3. La primera vez que uses el micrófono, el sistema te pedirá permiso (macOS, Windows y
   Flatpak). Acepta: el audio se procesa solo en tu equipo.
4. Los modelos de voz (Whistle y la voz Piper `es_ES-davefx-medium`) vienen incluidos en los
   paquetes y funcionan sin conexión.

Los ajustes se guardan en `settings.json` dentro de la carpeta de configuración del usuario
(Linux: `~/.config/pando-assistant/`). Si el archivo se corrompe, se guarda una copia y la
aplicación arranca con los valores por defecto.

## 3. Atajos y ventanas

| Acción | Cómo |
|---|---|
| Hablar (mantener pulsado) | `Ctrl+Alt+Espacio` (configurable: `shell.ptt_shortcut`) |
| Hablar desde el compositor (Wayland) | `pando-assistant ptt start` al pulsar y `pando-assistant ptt stop` al soltar |
| Mostrar / ocultar ventana | `pando-assistant show` / `hide` |
| Mini ventana flotante | `pando-assistant toggle-mini` o el menú de la bandeja |
| Salir | `pando-assistant quit` o «Salir» en la bandeja |

La **bandeja del sistema** ofrece: Escuchar, Manos libres, Silenciar voz, Abrir ventana,
Mini ventana y Salir, además del número de peticiones pendientes. Cerrar la ventana deja la
aplicación activa en la bandeja (se puede desactivar con `shell.keep_running`). Puedes iniciar
con la sesión (`shell.autostart`) y arrancar minimizado (`shell.start_minimized`).

Ejemplo para Sway:

```
bindsym --no-repeat $mod+space exec pando-assistant ptt start
bindsym --release $mod+space exec pando-assistant ptt stop
```

## 4. Órdenes de voz (ejemplos)

Mantén el atajo y habla. El asistente decide si es una **orden** (se ejecuta), una
**conversación** (se envía a Pando y responde en voz alta) o **dictado** (texto al cuadro de
entrada: «dicta…», «apunta…», «toma nota…»).

- «Cancela migración auth» (cancelar un agente).
- «Aprueba» / «Rechaza» cuando un agente espera tu permiso.
- «Cambia al proyecto bitácora».
- «Avísame cuando termine» (se anuncia en voz alta al acabar una sesión de Claude Code).

**Seguridad**: las órdenes destructivas exigen alta confianza y el identificador literal; si
no está claro, el asistente te pide confirmación en pantalla. Lo marcado por Pando como
«requiere aprobación explícita» solo se aprueba con una decisión tuya explícita, nunca
automáticamente. Ante la duda, el asistente no ejecuta.

## 5. Manos libres y privacidad

- **Manos libres** **(planificado)**: la palabra de activación «Oye Pando» (o «Hey Pando»)
  activa la escucha sin tocar el teclado. Detección de voz y verificación por Whistle, todo local.
- El audio nunca sale del equipo. Se procesan en local el micrófono, la transcripción y la voz.
  El asistente solo habla con tu Pando (normalmente `localhost`).
- El token de Pando no se escribe en los registros.
- La integración con Claude Code instala unos *hooks* locales; no envía datos a ningún servicio.
- Los registros están en `~/.cache/pando-assistant/logs/` (Linux).

## 6. Claude Code como células

Si usas Claude Code, el asistente puede instalar *hooks* en su `settings.json` (con copia de
seguridad previa; se pueden desinstalar). Cada sesión aparece como una célula: se ve cuándo
trabaja, cuándo espera un permiso y cuándo termina. Puedes enviarle mensajes por el socket de
mensajería. Limitación conocida: tras aprobar un permiso la célula sigue en «esperando» hasta el
siguiente evento.

## 7. Limitaciones por plataforma

- **Wayland**: no hay atajo global universal. Usa `pando-assistant ptt start/stop` desde tu
  compositor, o el portal GlobalShortcuts cuando esté disponible **(planificado)**. La bandeja
  requiere StatusNotifier (en GNOME, la extensión AppIndicator).
- **Flatpak**: `/tmp` no se comparte, así que el descubrimiento de Pando por `/tmp/pando-instances`
  usa la dirección por defecto.
- **Mac Intel**: sin Whistle; reconocimiento alternativo más pesado.
- **Windows**: el worker de reconocimiento es un ejecutable aparte compilado con otra cadena
  de herramientas; es normal ver tres procesos.
- Sin GPU Vulkan/Metal/DirectX 12 el organismo se dibuja por CPU.

## 8. Resolución de problemas

| Síntoma | Qué hacer |
|---|---|
| «Conexión con Pando perdida» | Comprueba que Pando sigue en marcha; el asistente reintenta solo. Indica `pando_url` si usas otra dirección. |
| La ventana se queda en negro o no arranca | Ejecuta `pando-assistant --smoke-test`; prueba `PANDO_ORGANISM_RENDER=raster`. Revisa los registros. |
| No oye el micrófono | Revisa el dispositivo de entrada del sistema y el permiso de micrófono. En Linux comprueba que PipeWire/PulseAudio está activo. |
| La voz tarda en empezar | En PulseAudio el primer audio puede tardar unos 2 s; PipeWire es más rápido. |
| El atajo no hace nada (Wayland) | Configura el atajo en tu compositor con `pando-assistant ptt` (sección 3). |
| Reconoce mal nombres de proyecto | Los nombres de tus proyectos se pasan a Whistle como lista de palabras; habla con claridad y comprueba que Pando los lista. |

## 9. Licencias

La aplicación y el reconocimiento de voz son MIT. El sintetizador `pando-tts` es GPL-3 (usa
espeak-ng) y cada versión ofrece su código fuente completo. Detalles en los archivos `NOTICE`
y `SOURCE-OFFER.md` incluidos en el paquete.
