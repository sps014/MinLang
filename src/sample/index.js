var memory = new WebAssembly.Memory({initial:1});

var importObject = { js: { mem: memory } };
let wasm_obj;
WebAssembly.instantiateStreaming(fetch('basic_sum.wasm'), importObject)
  .then(obj => {
    wasm_obj=obj.instance.exports;
  });