import * as wasm from "wasm-minidump";

document.documentElement.addEventListener('drop', (e) => {
  e.preventDefault()
  const item = e.dataTransfer.items[0]
  const file = item.getAsFile()
  const reader = new FileReader
  reader.onload = () => {
    const arr = new Uint8Array(reader.result)
    document.body.textContent = wasm.parse(new Uint8Array(arr));
  }
  reader.readAsArrayBuffer(file)
})

document.documentElement.addEventListener('dragover', (e) => {
  e.preventDefault()
})

document.documentElement.addEventListener('dragleave', (e) => {
  e.preventDefault()
})

