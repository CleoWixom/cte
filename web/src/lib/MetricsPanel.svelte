<script>
  export let result = null
  export let loading = false
  export let wasmReady = false

  function gdopLabel(v) {
    if (!isFinite(v)) return { text: 'N/A', cls: 'bad' }
    if (v < 2)  return { text: 'Excellent', cls: 'excellent' }
    if (v < 4)  return { text: 'Good',      cls: 'good' }
    if (v < 6)  return { text: 'Fair',      cls: 'fair' }
    return { text: 'Poor', cls: 'bad' }
  }

  $: gdop = result ? gdopLabel(result.gdop) : null
</script>

<section class="panel">
  <h2>Triangulation Metrics</h2>

  {#if !wasmReady}
    <div class="hint">⚠️ WASM not loaded — run <code>wasm-pack build crates/wasm --target web</code></div>
  {:else if loading}
    <div class="placeholder">Computing…</div>
  {:else if !result}
    <div class="placeholder">Click the map to run triangulation</div>
  {:else}
    <div class="grid">
      <div class="metric">
        <span class="label">CEP50</span>
        <span class="value green">{Math.round(result.cep50_m)} m</span>
      </div>
      <div class="metric">
        <span class="label">CEP95</span>
        <span class="value yellow">{Math.round(result.cep95_m)} m</span>
      </div>
      <div class="metric">
        <span class="label">GDOP</span>
        <span class="value {gdop.cls}">{isFinite(result.gdop) ? result.gdop.toFixed(2) : '∞'}</span>
      </div>
      <div class="metric">
        <span class="label">Quality</span>
        <span class="value {gdop.cls}">{gdop.text}</span>
      </div>
      <div class="metric">
        <span class="label">Towers used</span>
        <span class="value">{result.n_towers_used}</span>
      </div>
      <div class="metric">
        <span class="label">Measurements</span>
        <span class="value">{result.n_measurements}</span>
      </div>
      <div class="metric">
        <span class="label">MC samples</span>
        <span class="value">{result.cloud?.length ?? 0}</span>
      </div>
      <div class="metric">
        <span class="label">Model</span>
        <span class="value">{result.model_type}</span>
      </div>
    </div>

    <div class="ellipse-info">
      <span class="label">Error ellipse (95%)</span>
      <span>{Math.round(result.ellipse_semi_major_m)} × {Math.round(result.ellipse_semi_minor_m)} m</span>
      <span class="muted">@ {result.ellipse_angle_deg.toFixed(1)}°</span>
    </div>

    <div class="status">
      {#if result.converged}
        <span class="tag green">✓ Converged</span>
      {:else}
        <span class="tag yellow">⚠ Max iterations</span>
      {/if}
    </div>
  {/if}
</section>

<style>
  .panel { display: flex; flex-direction: column; gap: 12px; }
  h2 { margin: 0 0 4px; font-size: 0.9rem; text-transform: uppercase;
       letter-spacing: 0.06em; color: #64748b; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .metric { background: #0f1117; border-radius: 6px; padding: 8px 10px;
            display: flex; flex-direction: column; gap: 2px; }
  .label { font-size: 0.72rem; color: #64748b; text-transform: uppercase;
           letter-spacing: 0.04em; }
  .value { font-size: 1.1rem; font-weight: 700; color: #e2e8f0; }
  .green  { color: #22c55e; }
  .yellow { color: #f59e0b; }
  .fair   { color: #fb923c; }
  .bad    { color: #ef4444; }
  .excellent { color: #22c55e; }
  .good   { color: #4ade80; }
  .placeholder { color: #64748b; font-size: 0.85rem; padding: 12px 0; text-align: center; }
  .hint { color: #f59e0b; font-size: 0.8rem; background: #1c1a0f; padding: 8px;
          border-radius: 6px; border: 1px solid #44310a; }
  .hint code { background: #2d2510; padding: 1px 4px; border-radius: 3px; font-size: 0.75rem; }
  .ellipse-info { display: flex; align-items: center; gap: 8px; font-size: 0.82rem;
                  color: #94a3b8; flex-wrap: wrap; }
  .muted { color: #475569; }
  .status { display: flex; gap: 8px; }
  .tag { font-size: 0.78rem; padding: 3px 8px; border-radius: 4px; font-weight: 600; }
  .tag.green  { background: #052e16; color: #22c55e; border: 1px solid #166534; }
  .tag.yellow { background: #1c1a0e; color: #f59e0b; border: 1px solid #78350f; }
</style>
