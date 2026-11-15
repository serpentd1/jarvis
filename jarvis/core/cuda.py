"""Rend les bibliothèques CUDA installées par pip (cuBLAS, cuDNN) visibles pour faster-whisper."""
import os
import sys
from pathlib import Path

_fait = False


def preparer():
    global _fait
    if _fait or sys.platform != "win32":
        return
    _fait = True
    for base in map(Path, sys.path):
        nvidia = base / "nvidia"
        if not nvidia.is_dir():
            continue
        for dossier_bin in nvidia.glob("*/bin"):
            os.add_dll_directory(str(dossier_bin))
            os.environ["PATH"] = str(dossier_bin) + os.pathsep + os.environ.get("PATH", "")
