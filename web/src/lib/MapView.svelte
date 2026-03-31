<script>
  import { onMount, onDestroy, createEventDispatcher } from 'svelte'
  import L from 'leaflet'
  import 'leaflet/dist/leaflet.css'

  export let selectedPoint = null
  export let result = null
  export let loading = false

  const dispatch = createEventDispatcher()

  let mapEl
  let map
  let markerLayer
  let resultLayer
  let cloudLayer
  let ellipseLayer
  let towerLayer
  let showCloud = true

  onMount(() => {
    map = L.map(mapEl, { zoomControl: true }).setView([55.75, 37.62], 13)

    L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
      attribution: '© OpenStreetMap contributors',
      maxZoom: 19,
    }).addTo(map)

    markerLayer = L.layerGroup().addTo(map)
    towerLayer  = L.layerGroup().addTo(map)
    cloudLayer  = L.layerGroup().addTo(map)
    resultLayer = L.layerGroup().addTo(map)
    ellipseLayer = L.layerGroup().addTo(map)

    map.on('click', (e) => {
      dispatch('pointSelected', { lat: e.latlng.lat, lon: e.latlng.lng })
    })
  })

  onDestroy(() => { map?.remove() })

  // Selected point marker
  $: if (map && selectedPoint) {
    markerLayer.clearLayers()
    L.circleMarker([selectedPoint.lat, selectedPoint.lon], {
      radius: 8, color: '#7c83ff', fillColor: '#7c83ff',
      fillOpacity: 0.8, weight: 2,
    }).bindPopup(`Query: ${selectedPoint.lat.toFixed(5)}, ${selectedPoint.lon.toFixed(5)}`)
      .addTo(markerLayer)
  }

  // Result + cloud + ellipse
  $: if (map && result) {
    resultLayer.clearLayers()
    cloudLayer.clearLayers()
    ellipseLayer.clearLayers()

    // Monte Carlo cloud
    if (showCloud && result.cloud?.length) {
      result.cloud.forEach(pt => {
        L.circleMarker([pt.lat, pt.lon], {
          radius: 2, color: '#f59e0b', fillColor: '#f59e0b',
          fillOpacity: 0.25, weight: 0,
        }).addTo(cloudLayer)
      })
    }

    // Error ellipse (approximate with polygon)
    if (result.ellipse_semi_major_m > 0) {
      const pts = ellipsePoints(
        result.lat, result.lon,
        result.ellipse_semi_major_m,
        result.ellipse_semi_minor_m,
        result.ellipse_angle_deg,
        64
      )
      L.polygon(pts, {
        color: '#ef4444', fillColor: '#ef4444',
        fillOpacity: 0.08, weight: 1.5, dashArray: '4 4',
      }).addTo(ellipseLayer)
    }

    // CEP50 circle
    L.circle([result.lat, result.lon], {
      radius: result.cep50_m, color: '#22c55e',
      fillColor: '#22c55e', fillOpacity: 0.07,
      weight: 1.5, dashArray: '6 3',
    }).bindPopup(`CEP50: ${Math.round(result.cep50_m)} m`).addTo(resultLayer)

    // Result marker
    L.circleMarker([result.lat, result.lon], {
      radius: 10, color: '#22c55e', fillColor: '#22c55e',
      fillOpacity: 0.9, weight: 3,
    }).bindPopup(
      `<b>Estimated position</b><br>` +
      `${result.lat.toFixed(6)}, ${result.lon.toFixed(6)}<br>` +
      `CEP50: ${Math.round(result.cep50_m)} m<br>` +
      `CEP95: ${Math.round(result.cep95_m)} m<br>` +
      `GDOP: ${result.gdop.toFixed(2)}`
    ).addTo(resultLayer)
  }

  function ellipsePoints(clat, clon, a, b, angleDeg, npts) {
    const angleRad = (angleDeg * Math.PI) / 180
    const latScale = 1 / 111320
    const lonScale = 1 / (111320 * Math.cos(clat * Math.PI / 180))
    const pts = []
    for (let i = 0; i < npts; i++) {
      const t = (2 * Math.PI * i) / npts
      const x = a * Math.cos(t) * Math.cos(angleRad) - b * Math.sin(t) * Math.sin(angleRad)
      const y = a * Math.cos(t) * Math.sin(angleRad) + b * Math.sin(t) * Math.cos(angleRad)
      pts.push([clat + x * latScale, clon + y * lonScale])
    }
    return pts
  }
</script>

<div bind:this={mapEl} class="map"></div>

{#if loading}
  <div class="overlay">
    <div class="spinner"></div>
    <span>Computing triangulation…</span>
  </div>
{/if}

<div class="controls">
  <label>
    <input type="checkbox" bind:checked={showCloud} />
    Show MC cloud
  </label>
</div>

<style>
  .map {
    width: 100%;
    height: 100%;
    background: #1a1d27;
  }
  .overlay {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: rgba(26, 29, 39, 0.92);
    border: 1px solid #2d3148;
    border-radius: 8px;
    padding: 16px 24px;
    display: flex;
    align-items: center;
    gap: 12px;
    color: #e2e8f0;
    z-index: 1000;
  }
  .spinner {
    width: 20px;
    height: 20px;
    border: 2px solid #2d3148;
    border-top-color: #7c83ff;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .controls {
    position: absolute;
    bottom: 28px;
    left: 12px;
    z-index: 1000;
    background: rgba(26,29,39,0.88);
    border: 1px solid #2d3148;
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 0.8rem;
    color: #94a3b8;
  }
  .controls label { display: flex; align-items: center; gap: 6px; cursor: pointer; }
</style>
