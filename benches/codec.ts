import { group, bench, run } from "mitata";
import { Codec } from "../lib";

const buf = Codec.warmup(4096); // still faster without warming

for (const length of [128, 256, 512, 1024, 2048, 4096]) {
    let to_encode = '';
    for (let i = 0; i < length; i++) {
        // Printable ASCII range: 32 (space) to 126 (~)
        to_encode += String.fromCharCode(Math.floor(Math.random() * (126 - 32 + 1)) + 32);
    }

    group(`Encoding string of length ${length}`, () => {
        bench("TextEncoder"  , () => new TextEncoder().encode(to_encode));
        bench("Codec"        , () => Codec.encode(to_encode));
    });
}

// for (const length of [128, 256, 512, 1024, 2048, 4096]) {
//     let to_decode = new Uint8Array(length).map(() => Math.floor(Math.random() * (126 - 32 + 1)) + 32);

//     group(`Decoding uint8array of length ${length}`, () => {
//         bench("TextDecoder"  , () => new TextDecoder().decode(to_decode));
//         bench("Codec"        , () => Codec.decode(to_decode));
//     });
// }

run();