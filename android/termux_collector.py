#!/usr/bin/env python3
"""
Trieval — Termux cell data collector
Reads cell info via termux-api and streams it to the trieval WebSocket.

Requirements (in Termux):
  pkg install termux-api python
  pip install websocket-client

Usage:
  python3 termux_collector.py --server ws://YOUR_SERVER_IP:8080
"""

import argparse
import json
import subprocess
import sys
import time


def get_cell_info() -> list:
    """Call termux-telephony-cellinfo and return parsed list."""
    try:
        raw = subprocess.check_output(
            ["termux-telephony-cellinfo"], timeout=5
        )
        cells = json.loads(raw)
        return cells if isinstance(cells, list) else []
    except (subprocess.TimeoutExpired, json.JSONDecodeError, FileNotFoundError) as e:
        print(f"[warn] termux-telephony-cellinfo failed: {e}", file=sys.stderr)
        return []


def get_location() -> dict | None:
    """Get GPS coordinates via termux-location (best effort)."""
    try:
        raw = subprocess.check_output(
            ["termux-location", "-p", "gps", "-r", "once"], timeout=10
        )
        loc = json.loads(raw)
        return {"lat": loc["latitude"], "lon": loc["longitude"]}
    except Exception:
        return None


def normalize_cells(raw_cells: list, location: dict | None) -> list:
    """Convert termux cell format to trieval measurement format."""
    result = []
    for cell in raw_cells:
        cell_type = cell.get("type", "").upper()

        # Map termux type names to our radio types
        radio_map = {
            "GSM": "GSM", "CDMA": "GSM",
            "WCDMA": "UMTS", "UMTS": "UMTS",
            "LTE": "LTE",
            "NR": "NR", "5G": "NR",
        }
        radio = radio_map.get(cell_type, "LTE")

        # Extract identifiers (field names vary by Android version)
        mcc = cell.get("mcc") or cell.get("mobile_country_code")
        mnc = cell.get("mnc") or cell.get("mobile_network_code")
        lac = cell.get("lac") or cell.get("tac") or cell.get("tracking_area_code") or 0
        cid = (cell.get("cid") or cell.get("cell_identity") or
               cell.get("ci") or cell.get("nci") or 0)

        # Signal strength
        rssi = (cell.get("signal_strength") or cell.get("rsrp") or
                cell.get("dbm") or cell.get("ss_rsrp") or -100)

        if not all([mcc, mnc, cid]):
            continue  # Skip incomplete records

        record = {
            "radio": radio,
            "mcc": int(mcc),
            "mnc": int(mnc),
            "lac": int(lac),
            "cid": int(cid),
            "rssi": int(rssi),
        }
        # Attach GPS if available
        if location:
            record["lat"] = location["lat"]
            record["lon"] = location["lon"]

        result.append(record)
    return result


def main():
    parser = argparse.ArgumentParser(description="Trieval Termux collector")
    parser.add_argument("--server", default="ws://localhost:8080",
                        help="Trieval WebSocket URL (default: ws://localhost:8080)")
    parser.add_argument("--interval", type=float, default=5.0,
                        help="Measurement interval in seconds (default: 5)")
    parser.add_argument("--gps", action="store_true",
                        help="Also capture GPS location (slower, drains battery)")
    args = parser.parse_args()

    ws_url = args.server.rstrip("/") + "/api/ws/android"

    try:
        import websocket
    except ImportError:
        print("Install websocket-client: pip install websocket-client", file=sys.stderr)
        sys.exit(1)

    ws = websocket.WebSocket()
    print(f"Connecting to {ws_url} ...")
    ws.connect(ws_url)
    print("Connected. Sending measurements every", args.interval, "s  (Ctrl+C to stop)")

    sent = 0
    try:
        while True:
            location = get_location() if args.gps else None
            raw = get_cell_info()
            cells = normalize_cells(raw, location)

            if cells:
                msg = json.dumps({"type": "measurement", "cells": cells})
                ws.send(msg)
                ack = ws.recv()
                ack_data = json.loads(ack)
                sent += ack_data.get("imported", 0)
                print(f"[{time.strftime('%H:%M:%S')}] sent {len(cells)} cells"
                      f"  (total imported: {sent})")
            else:
                print(f"[{time.strftime('%H:%M:%S')}] no cells found")

            time.sleep(args.interval)

    except KeyboardInterrupt:
        print(f"\nStopped. Total imported: {sent}")
    finally:
        ws.close()


if __name__ == "__main__":
    main()
