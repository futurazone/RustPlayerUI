# Next Steps / Backlog

## Pendiente prioritario (del bloque inicial)

1. Endurecer la capa API (`src/api.rs`)
   - Validar respuestas HTTP con `error_for_status()` en todas las llamadas.
   - Definir `timeout` explícito en cliente `reqwest::blocking`.
   - Evitar query params concatenados a mano (`track_id`): usar encoding seguro.
   - Objetivo: evitar estados optimistas falsos cuando el backend devuelve 4xx/5xx o hay red inestable.

## Swiper (estado actual y seguimiento)

1. Ajustes de snap/flick ya aplicados:
   - Umbrales más conservadores.
   - Predicción inercial reducida.
   - Clamp de slots para evitar saltos excesivos.

2. Optimizaciones ya aplicadas:
   - Reutilización de `x_positions` (menos trabajo por frame).
   - Índice `track_id -> cover` para evitar escaneo completo en caliente.
   - Tap lateral de 1 slot consistente.

3. Validación pendiente en Raspberry Pi:
   - Swipe corto de 1 disco.
   - Tap lateral repetido.
   - Flick largo.
   - Revisar logs de `slow frame` y `selector release`.

## Refactor recomendado (sin urgencia alta, pero útil antes de nuevas pantallas)

1. Partir el loop principal de `src/main.rs` en etapas pequeñas:
   - `process_status`
   - `process_library_update`
   - `process_images`
   - `update_swiper`
   - `update_background`

2. Añadir `ScreenController` ligero para centralizar transiciones:
   - `selector <-> player <-> track_picker <-> visualizer`

3. Encapsular estado de “now playing” para que visualizadores (VU/spectrum) consuman una fuente única.

## Futuro: pantallas visuales (VU / Spectrum 90s)

1. Definir fuente de señal:
   - RMS + FFT desde backend, o endpoint dedicado de visualización.

2. Definir ritmo de refresco:
   - FPS objetivo y estrategia de suavizado.

3. Definir UX de acceso:
   - Botón/tap desde Player.
   - Alternancia entre modos visuales.

4. Implementación por fases:
   - Fase 1: VU meter simple.
   - Fase 2: Spectrum estilo 90s.
   - Fase 3: temas visuales y transiciones.
