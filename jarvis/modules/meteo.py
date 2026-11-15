"""Météo de secours (Open-Meteo, gratuit) quand la barre Givre n'est pas lancée.
Il faut indiquer une ville dans config.toml, section [meteo]."""
import logging
import time

import requests

log = logging.getLogger("meteo")
CIEL = {0: "ciel dégagé", 1: "plutôt dégagé", 2: "partiellement nuageux", 3: "couvert", 45: "brouillard",
        48: "brouillard givrant", 51: "bruine légère", 53: "bruine", 55: "bruine forte", 61: "pluie faible",
        63: "pluie", 65: "forte pluie", 71: "neige faible", 73: "neige", 75: "forte neige", 80: "averses",
        81: "averses", 82: "fortes averses", 95: "orage", 96: "orage avec grêle", 99: "orage avec grêle"}


def installer(ctx, reg):
    ville = ctx.cfg["meteo"]["ville"].strip()
    cache = {"t": 0, "texte": "", "lieu": None}

    def contexte():
        if not ville or ctx.givre.disponible():
            return ""
        if time.time() - cache["t"] < 900:
            return cache["texte"]
        try:
            if not cache["lieu"]:
                g = requests.get("https://geocoding-api.open-meteo.com/v1/search",
                                 params={"name": ville, "count": 1, "language": "fr"}, timeout=5).json()
                r = g["results"][0]
                cache["lieu"] = (r["latitude"], r["longitude"], r["name"])
            lat, lon, nom = cache["lieu"]
            m = requests.get("https://api.open-meteo.com/v1/forecast", timeout=5, params={
                "latitude": lat, "longitude": lon, "current": "temperature_2m,weather_code,wind_speed_10m",
                "daily": "temperature_2m_min,temperature_2m_max,weather_code", "forecast_days": 3,
                "timezone": "auto"}).json()
            c, d = m["current"], m["daily"]
            jours = " ; ".join(f"{d['time'][i]} {round(d['temperature_2m_min'][i])}° à {round(d['temperature_2m_max'][i])}° "
                               f"{CIEL.get(d['weather_code'][i], '')}" for i in range(len(d["time"])))
            cache["texte"] = (f"MÉTÉO ({nom}) : {round(c['temperature_2m'])}°C, {CIEL.get(c['weather_code'], '')}, "
                              f"vent {round(c['wind_speed_10m'])} km/h. Prévisions : {jours}.")
            cache["t"] = time.time()
        except Exception:  # noqa: BLE001
            log.warning("météo de secours indisponible")
        return cache["texte"]

    reg.contexte(contexte)
