#!/usr/bin/env python3
"""Fetch the Phetsarath Lao web/app kit from the LaoFonts catalog API."""

from __future__ import annotations

import json
import pathlib
import urllib.request


API_URL = "https://fonts.mts.la/api/fonts.json"
FAMILY_NAME = "Phetsarath Lao"
WANTED_WEIGHTS = {400: "Regular", 600: "SemiBold", 700: "Bold"}
ROOT = pathlib.Path(__file__).resolve().parent.parent
DESTINATIONS = [
    ROOT / "macos" / "Sources" / "MiraMD" / "Resources" / "Fonts",
    ROOT / "windows" / "Assets" / "Fonts",
]


def get(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "MiraMD font fetcher/0.1"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read()


def main() -> None:
    catalog = json.loads(get(API_URL))
    family = next(item for item in catalog["families"] if item["name"] == FAMILY_NAME)
    selected = {
        style["weight"]: style
        for style in family["styles"]
        if not style["italic"] and style["weight"] in WANTED_WEIGHTS
    }
    if selected.keys() != WANTED_WEIGHTS.keys():
        raise RuntimeError("The LaoFonts API did not return all required Phetsarath Lao weights")

    for destination in DESTINATIONS:
        destination.mkdir(parents=True, exist_ok=True)
        for weight, output_name in WANTED_WEIGHTS.items():
            font = get(selected[weight]["woff2"])
            if not font.startswith(b"wOF2"):
                raise RuntimeError(f"Unexpected font payload for weight {weight}")
            (destination / f"PhetsarathLao-{output_name}.woff2").write_bytes(font)
        metadata = {
            "catalog": API_URL,
            "catalog_version": catalog["version"],
            "family": family["name"],
            "font_family_css": family["font_family_css"],
            "license": family["license"]["label"],
            "source_page": family["page"],
            "weights": sorted(WANTED_WEIGHTS),
        }
        (destination / "font-metadata.json").write_text(
            json.dumps(metadata, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
    print(f"Fetched {FAMILY_NAME} catalog version {catalog['version']}")


if __name__ == "__main__":
    main()
