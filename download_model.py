import urllib.request
import os

os.makedirs("models/all-MiniLM-L6-v2", exist_ok=True)

# Download ONNX model
print("Downloading model.onnx...")
urllib.request.urlretrieve(
    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx",
    "models/all-MiniLM-L6-v2/model.onnx"
)

# Download tokenizer
print("Downloading tokenizer.json...")
urllib.request.urlretrieve(
    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json",
    "models/all-MiniLM-L6-v2/tokenizer.json"
)

print("Done.")
