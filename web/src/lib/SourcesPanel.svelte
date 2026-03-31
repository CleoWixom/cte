<script>
  let uploading = false
  let uploadResult = null
  let dragOver = false
  let androidStatus = 'disconnected'

  async function handleFile(file) {
    if (!file) return
    uploading = true
    uploadResult = null
    const form = new FormData()
    form.append('file', file)
    form.append('source', 'upload')
    try {
      const resp = await fetch('/api/upload', { method: 'POST', body: form })
      uploadResult = await resp.json()
    } catch (e) {
      uploadResult = { error: String(e) }
    } finally {
      uploading = false
    }
  }

  function onDrop(e) {
    e.preventDefault()
    dragOver = false
    const file = e.dataTransfer?.files?.[0]
    if (file) handleFile(file)
  }

  function onFileInput(e) {
    handleFile(e.target.files?.[0])
  }

  // Try to connect Android WS
  function connectAndroid() {
    const ws = new WebSocket(`ws://${location.host}/api/ws/android`)
    ws.onopen = () => { androidStatus = 'connected' }
    ws.onclose = () => { androidStatus = 'disconnected' }
    ws.onerror = () => { androidStatus = 'error' }
  }
</script>

<section class="panel">
  <h2>Data Sources</h2>

  <div class="source-row">
    <span class="dot" class:green={true}></span>
    <span>OpenCelliD API</span>
    <span class="muted">(live on click)</span>
  </div>

  <div class="source-row">
    <span class="dot" class:green={androidStatus === 'connected'} class:red={androidStatus === 'error'}></span>
    <span>Android</span>
    <span class="muted">{androidStatus}</span>
    {#if androidStatus !== 'connected'}
      <button class="btn-xs" on:click={connectAndroid}>Connect</button>
    {/if}
  </div>

  <div
    class="dropzone"
    class:over={dragOver}
    on:dragover|preventDefault={() => dragOver = true}
    on:dragleave={() => dragOver = false}
    on:drop={onDrop}
    role="region"
    aria-label="CSV upload"
  >
    {#if uploading}
      <span class="muted">Importing…</span>
    {:else if uploadResult}
      {#if uploadResult.error}
        <span class="red">{uploadResult.error}</span>
      {:else}
        <span class="green">✓ {uploadResult.imported_towers} towers, {uploadResult.imported_measurements} measurements</span>
      {/if}
    {:else}
      <span>Drop OpenCelliD CSV here</span>
      <label class="btn-xs" style="cursor:pointer">
        Browse
        <input type="file" accept=".csv,.gz" on:change={onFileInput} style="display:none" />
      </label>
    {/if}
  </div>

  <details class="termux">
    <summary>Termux collector script</summary>
    <pre>{`#!/usr/bin/env python3
import subprocess, json, websocket, time

ws = websocket.WebSocket()
ws.connect("ws://YOUR_SERVER/api/ws/android")

while True:
    raw = subprocess.check_output(['termux-telephony-cellinfo'])
    cells = json.loads(raw)
    ws.send(json.dumps({"type":"measurement","cells":cells}))
    time.sleep(5)`}</pre>
  </details>
</section>

<style>
  .panel { display: flex; flex-direction: column; gap: 10px; }
  h2 { margin: 0 0 4px; font-size: 0.9rem; text-transform: uppercase;
       letter-spacing: 0.06em; color: #64748b; }
  .source-row { display: flex; align-items: center; gap: 8px; font-size: 0.85rem; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: #475569; flex-shrink: 0; }
  .dot.green { background: #22c55e; }
  .dot.red   { background: #ef4444; }
  .muted { color: #475569; font-size: 0.8rem; }
  .green { color: #22c55e; }
  .red   { color: #ef4444; }
  .btn-xs {
    margin-left: auto; font-size: 0.72rem; padding: 2px 8px;
    background: #2d3148; border: 1px solid #3d4466; border-radius: 4px;
    color: #94a3b8; cursor: pointer;
  }
  .btn-xs:hover { background: #3d4466; color: #e2e8f0; }
  .dropzone {
    border: 1px dashed #2d3148; border-radius: 6px; padding: 14px 12px;
    text-align: center; font-size: 0.82rem; color: #64748b;
    display: flex; flex-direction: column; align-items: center; gap: 8px;
    transition: border-color 0.2s;
  }
  .dropzone.over { border-color: #7c83ff; color: #a5b4fc; }
  .termux { margin-top: 4px; }
  summary { font-size: 0.78rem; color: #475569; cursor: pointer; }
  pre { font-size: 0.7rem; background: #0f1117; border-radius: 6px; padding: 10px;
        overflow-x: auto; color: #94a3b8; margin: 6px 0 0; }
</style>
