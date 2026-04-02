/**
 * Trieval — minimal Android cell data collector (Kotlin)
 *
 * Reads all cell info via TelephonyManager and streams to trieval WebSocket.
 * Add to any Activity or Service. Requires permissions:
 *   android.permission.READ_PHONE_STATE
 *   android.permission.ACCESS_FINE_LOCATION  (needed for CID on Android 9+)
 *
 * Dependencies (app/build.gradle):
 *   implementation("com.squareup.okhttp3:okhttp:4.12.0")
 */

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.telephony.*
import androidx.core.content.ContextCompat
import okhttp3.*
import okio.ByteString
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

class TrievalCollector(
    private val context: Context,
    private val serverUrl: String = "ws://10.0.2.2:8080",  // 10.0.2.2 = host from emulator
) {
    private val client = OkHttpClient.Builder()
        .pingInterval(30, TimeUnit.SECONDS)
        .build()

    private var webSocket: WebSocket? = null
    private var totalImported = 0

    // ── Connect ──────────────────────────────────────────────────────────────

    fun connect() {
        val url = serverUrl.trimEnd('/') + "/api/ws/android"
        val request = Request.Builder().url(url).build()
        webSocket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(ws: WebSocket, response: Response) {
                android.util.Log.i("Trieval", "WebSocket connected to $url")
            }
            override fun onMessage(ws: WebSocket, text: String) {
                val ack = JSONObject(text)
                totalImported += ack.optInt("imported", 0)
                android.util.Log.d("Trieval", "ack: imported=${ack.optInt("imported")} total=$totalImported")
            }
            override fun onFailure(ws: WebSocket, t: Throwable, response: Response?) {
                android.util.Log.e("Trieval", "WebSocket error: ${t.message}")
            }
            override fun onClosed(ws: WebSocket, code: Int, reason: String) {
                android.util.Log.i("Trieval", "WebSocket closed: $reason")
            }
        })
    }

    fun disconnect() {
        webSocket?.close(1000, "collector stopped")
        client.dispatcher.executorService.shutdown()
    }

    // ── Measure & Send ───────────────────────────────────────────────────────

    fun sendMeasurement(lat: Double? = null, lon: Double? = null) {
        if (!hasPermissions()) return
        val cells = readCells(lat, lon)
        if (cells.length() == 0) return

        val msg = JSONObject().apply {
            put("type", "measurement")
            put("cells", cells)
        }
        webSocket?.send(msg.toString())
    }

    // ── Cell reading ─────────────────────────────────────────────────────────

    @Suppress("MissingPermission")
    private fun readCells(lat: Double?, lon: Double?): JSONArray {
        val tm = context.getSystemService(Context.TELEPHONY_SERVICE) as TelephonyManager
        val result = JSONArray()

        val allCells: List<CellInfo> = try {
            tm.allCellInfo ?: emptyList()
        } catch (e: SecurityException) {
            return result
        }

        for (cell in allCells) {
            val obj = when (cell) {
                is CellInfoLte -> parseLte(cell)
                is CellInfoGsm -> parseGsm(cell)
                is CellInfoWcdma -> parseWcdma(cell)
                is CellInfoNr -> parseNr(cell)
                else -> null
            } ?: continue

            if (lat != null && lon != null) {
                obj.put("lat", lat)
                obj.put("lon", lon)
            }
            result.put(obj)
        }
        return result
    }

    private fun parseLte(cell: CellInfoLte): JSONObject? {
        val id = cell.cellIdentity
        val sig = cell.cellSignalStrength
        val cid = id.ci.takeIf { it != Int.MAX_VALUE } ?: return null
        val mcc = id.mccString?.toIntOrNull() ?: return null
        val mnc = id.mncString?.toIntOrNull() ?: return null
        return JSONObject().apply {
            put("radio", "LTE")
            put("mcc", mcc); put("mnc", mnc)
            put("lac", id.tac.takeIf { it != Int.MAX_VALUE } ?: 0)
            put("cid", cid)
            put("rssi", sig.rsrp.takeIf { it != Int.MIN_VALUE } ?: sig.dbm)
        }
    }

    private fun parseGsm(cell: CellInfoGsm): JSONObject? {
        val id = cell.cellIdentity
        val sig = cell.cellSignalStrength
        val cid = id.cid.takeIf { it != Int.MAX_VALUE } ?: return null
        val mcc = id.mccString?.toIntOrNull() ?: return null
        val mnc = id.mncString?.toIntOrNull() ?: return null
        return JSONObject().apply {
            put("radio", "GSM")
            put("mcc", mcc); put("mnc", mnc)
            put("lac", id.lac.takeIf { it != Int.MAX_VALUE } ?: 0)
            put("cid", cid)
            put("rssi", sig.dbm)
        }
    }

    private fun parseWcdma(cell: CellInfoWcdma): JSONObject? {
        val id = cell.cellIdentity
        val sig = cell.cellSignalStrength
        val cid = id.cid.takeIf { it != Int.MAX_VALUE } ?: return null
        val mcc = id.mccString?.toIntOrNull() ?: return null
        val mnc = id.mncString?.toIntOrNull() ?: return null
        return JSONObject().apply {
            put("radio", "UMTS")
            put("mcc", mcc); put("mnc", mnc)
            put("lac", id.lac.takeIf { it != Int.MAX_VALUE } ?: 0)
            put("cid", cid)
            put("rssi", sig.dbm)
        }
    }

    private fun parseNr(cell: CellInfoNr): JSONObject? {
        val id = cell.cellIdentity as? CellIdentityNr ?: return null
        val sig = cell.cellSignalStrength as? CellSignalStrengthNr ?: return null
        val mcc = id.mccString?.toIntOrNull() ?: return null
        val mnc = id.mncString?.toIntOrNull() ?: return null
        return JSONObject().apply {
            put("radio", "NR")
            put("mcc", mcc); put("mnc", mnc)
            put("lac", id.tac.takeIf { it != Int.MAX_VALUE } ?: 0)
            put("cid", id.nci.takeIf { it != Long.MAX_VALUE } ?: return null)
            put("rssi", sig.ssRsrp.takeIf { it != Int.MIN_VALUE } ?: sig.dbm)
        }
    }

    // ── Permissions ───────────────────────────────────────────────────────────

    private fun hasPermissions(): Boolean = listOf(
        Manifest.permission.READ_PHONE_STATE,
        Manifest.permission.ACCESS_FINE_LOCATION,
    ).all { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }
}

// ── Usage example in Activity ─────────────────────────────────────────────────

/*
class MainActivity : AppCompatActivity() {
    private lateinit var collector: TrievalCollector
    private val handler = Handler(Looper.getMainLooper())
    private val interval = 5_000L  // 5 seconds

    private val measureRunnable = object : Runnable {
        override fun run() {
            // Pass GPS coordinates if you have them
            collector.sendMeasurement(lat = null, lon = null)
            handler.postDelayed(this, interval)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        collector = TrievalCollector(this, serverUrl = "ws://192.168.1.100:8080")
        collector.connect()
        handler.post(measureRunnable)
    }

    override fun onDestroy() {
        handler.removeCallbacks(measureRunnable)
        collector.disconnect()
        super.onDestroy()
    }
}
*/
