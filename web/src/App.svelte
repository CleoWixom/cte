<script>
  import { onMount } from 'svelte'
  import MapView from './lib/MapView.svelte'
  import MetricsPanel from './lib/MetricsPanel.svelte'
  import SourcesPanel from './lib/SourcesPanel.svelte'

  let selectedPoint = null
  let result = null
  let loading = false
  let engine = null
  let wasmReady = false

  onMount(async () => {
    try {
      const { default: init, TriangulationEngine } = await import('./wasm/trieval_wasm.js')
      await init()
      engine = new TriangulationEngine()
      wasmReady = true
    } catch (e) {
      console.warn('WASM not available (run wasm-pack build first):', e)
    }
  })

  async function onPointSelected(e) {
    const { lat, lon } = e.detail
    selectedPoint = { lat, lon }
    result = null

    if (!wasmReady || !engine) return

    loading = true
    try {
      // Fetch towers & measurements from backend
      const [towersResp, measResp] = await Promise.all([
        fetch(`/api/cells?lat=${lat}&lon=${lon}&radius_m=2000`).then(r => r.json()),
        fetch(`/api/measurements?lat=${lat}&lon=${lon}&radius_m=2000`).then(r => r.json()),
      ])

      engine.load_towers(towersResp.towers)
      engine.load_measurements(measResp.measurements)
      engine.set_model('kriging')
      engine.set_mc_iterations(300)

      result = engine.solve(lat, lon)
    } catch (err) {
      console.error('Triangulation failed:', err)
    } finally {
      loading = false
    }
  }
</script>

<main>
  <header>
    <h1>📡 Cell Triangulation Evaluator</h1>
    <span class="subtitle">Click the map to evaluate trilateration accuracy</span>
  </header>

  <div class="layout">
    <div class="map-container">
      <MapView
        {selectedPoint}
        {result}
        {loading}
        on:pointSelected={onPointSelected}
      />
    </div>

    <aside class="sidebar">
      <MetricsPanel {result} {loading} {wasmReady} />
      <SourcesPanel />
    </aside>
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    background: #0f1117;
    color: #e2e8f0;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  header {
    padding: 12px 20px;
    background: #1a1d27;
    border-bottom: 1px solid #2d3148;
    display: flex;
    align-items: baseline;
    gap: 16px;
  }

  h1 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 700;
    color: #7c83ff;
  }

  .subtitle {
    color: #64748b;
    font-size: 0.85rem;
  }

  .layout {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .map-container {
    flex: 1;
    position: relative;
  }

  .sidebar {
    width: 320px;
    min-width: 280px;
    overflow-y: auto;
    background: #1a1d27;
    border-left: 1px solid #2d3148;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  @media (max-width: 768px) {
    .layout { flex-direction: column; }
    .sidebar { width: 100%; border-left: none; border-top: 1px solid #2d3148; }
  }
</style>
