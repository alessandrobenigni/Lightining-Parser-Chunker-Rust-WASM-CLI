#!/usr/bin/env python3
"""Download ONNX models for Parser Chunker's vision pipeline.

Strategy: Install rapidocr_onnxruntime (which bundles PaddleOCR ONNX models)
and copy the models to the local models/ directory.

Alternative: If you have paddle2onnx installed, you can convert models from
the official PaddleOCR model zoo (https://paddleocr.bj.bcebos.com/).
"""
import os
import sys
import shutil
import importlib
import subprocess

MODELS_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "models")
os.makedirs(MODELS_DIR, exist_ok=True)

NEEDED_MODELS = {
    "paddleocr-det-en.onnx": "ch_PP-OCRv3_det_infer.onnx",
    "paddleocr-rec-en.onnx": "ch_PP-OCRv3_rec_infer.onnx",
}


def find_rapidocr_models():
    """Try to find ONNX models from the rapidocr_onnxruntime package."""
    try:
        import rapidocr_onnxruntime
        pkg_dir = os.path.dirname(rapidocr_onnxruntime.__file__)
        models_subdir = os.path.join(pkg_dir, "models")
        if os.path.isdir(models_subdir):
            return models_subdir
    except ImportError:
        pass
    return None


def install_rapidocr():
    """Install rapidocr_onnxruntime via pip (no-deps to avoid heavy dependencies)."""
    print("  Installing rapidocr_onnxruntime (for ONNX model files)...")
    result = subprocess.run(
        [sys.executable, "-m", "pip", "install", "rapidocr_onnxruntime", "--no-deps", "-q"],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        print(f"  pip install failed: {result.stderr}", file=sys.stderr)
        return False
    # Reload to find the newly installed package
    importlib.invalidate_caches()
    return True


def extract_dict_from_model(model_path, dict_path):
    """Extract character dictionary from ONNX model metadata."""
    try:
        import onnxruntime
        sess = onnxruntime.InferenceSession(model_path)
        meta = sess.get_modelmeta()
        char_str = meta.custom_metadata_map.get("character", "")
        if char_str:
            chars = [c for c in char_str.split("\n") if c]
            with open(dict_path, "w", encoding="utf-8") as f:
                for c in chars:
                    f.write(c + "\n")
            print(f"  EXTRACTED: en_dict.txt ({len(chars)} chars from model metadata)")
            return True
    except Exception as e:
        print(f"  Could not extract dict from model metadata: {e}")
    return False


def write_fallback_dict(dict_path):
    """Write a basic ASCII character dictionary as fallback."""
    chars = (
        "0123456789"
        "abcdefghijklmnopqrstuvwxyz"
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"
        " "
    )
    with open(dict_path, "w", encoding="utf-8") as f:
        for ch in chars:
            f.write(ch + "\n")
    print(f"  CREATED: en_dict.txt ({len(chars)} ASCII chars, fallback)")


def main():
    print(f"Models directory: {MODELS_DIR}")
    print()

    # Check what's already present
    all_present = True
    for local_name in NEEDED_MODELS:
        path = os.path.join(MODELS_DIR, local_name)
        if os.path.exists(path):
            size_mb = os.path.getsize(path) / 1048576
            print(f"  EXISTS: {local_name} ({size_mb:.1f} MB)")
        else:
            all_present = False

    dict_path = os.path.join(MODELS_DIR, "en_dict.txt")
    if os.path.exists(dict_path):
        print(f"  EXISTS: en_dict.txt")
    else:
        all_present = False

    if all_present:
        print("\nAll models already present!")
        return

    # Try to find or install rapidocr_onnxruntime
    models_src = find_rapidocr_models()
    if models_src is None:
        if not install_rapidocr():
            print("\nFailed to install rapidocr_onnxruntime.", file=sys.stderr)
            print("Manual download instructions:", file=sys.stderr)
            print("  1. pip install rapidocr_onnxruntime", file=sys.stderr)
            print("  2. Re-run this script", file=sys.stderr)
            sys.exit(1)
        models_src = find_rapidocr_models()

    if models_src is None:
        print("\nCould not find ONNX model files.", file=sys.stderr)
        sys.exit(1)

    print(f"\n  Found RapidOCR models at: {models_src}")

    # Copy models
    all_ok = True
    for local_name, src_name in NEEDED_MODELS.items():
        dest = os.path.join(MODELS_DIR, local_name)
        if os.path.exists(dest):
            continue
        src = os.path.join(models_src, src_name)
        if os.path.exists(src):
            shutil.copy2(src, dest)
            size_mb = os.path.getsize(dest) / 1048576
            print(f"  COPIED: {local_name} ({size_mb:.1f} MB)")
        else:
            print(f"  NOT FOUND: {src_name} in {models_src}", file=sys.stderr)
            all_ok = False

    # Extract or create dictionary
    if not os.path.exists(dict_path):
        rec_model = os.path.join(MODELS_DIR, "paddleocr-rec-en.onnx")
        if os.path.exists(rec_model):
            if not extract_dict_from_model(rec_model, dict_path):
                write_fallback_dict(dict_path)
        else:
            write_fallback_dict(dict_path)

    print()
    if all_ok:
        print("All models ready!")
    else:
        print("Some models could not be obtained. Check errors above.", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
