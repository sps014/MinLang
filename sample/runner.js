const fs = require('fs');

const wasmBuffer = fs.readFileSync(process.argv[2]);

let wasmMemory = null;

function readString(ptr) {
    if (!wasmMemory) return `ptr:${ptr}`;
    const bytes = new Uint8Array(wasmMemory.buffer);
    let end = ptr;
    while (bytes[end] !== 0) {
        end++;
    }
    return new TextDecoder('utf8').decode(bytes.subarray(ptr, end));
}

const importObject = {
    env: {
        print_string: (arg) => process.stdout.write(readString(arg)),
        println: (arg) => console.log(readString(arg)),
        print_int: (arg) => console.log(arg),
        print_float: (arg) => console.log(arg),
        concat_strings: (a, b) => {
            console.log("concat_strings called with", readString(a), readString(b));
            return 0; // Dummy
        },
        sin: Math.sin,
        cos: Math.cos,
        abs: Math.abs,
        sqrt: Math.sqrt,
        strlen: (arg) => {
            return readString(arg).length;
        },
        malloc: (arg) => {
            console.log("malloc called with", arg);
            return 0; // Dummy
        },
        free: (arg) => {
            console.log("free called with", arg);
        }
    }
};

WebAssembly.instantiate(wasmBuffer, importObject).then(wasmModule => {
    wasmMemory = wasmModule.instance.exports.memory;
    const { main } = wasmModule.instance.exports;
    if (main) {
        main();
    } else {
        console.log("No main function found");
    }
}).catch(e => {
    console.error("Error instantiating WebAssembly:", e);
});
