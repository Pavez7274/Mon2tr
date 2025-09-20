export {}; // for top level await

const { Stream } = require("../mon2tr.node") as typeof import("../mon2tr");

const e = new TextEncoder().encode.bind(new TextEncoder());

const stream = new Stream(e(`Bot ${Bun.env.TOKEN}`));
stream.recvLoop();

const identifier = await stream.send(0x5, e("GET"), e("/api/v10/users/@me"));
const response   = await stream.recv(identifier);

console.log("\x1b[1m[ HEADERS ]\x1b[0m");
for (const header of response?.headers ?? []) {
    console.log("%s: %s", header.name, header.value);
}

console.log("\n\x1b[1m[ BODY ]\x1b[0m");
console.log(new TextDecoder().decode(Uint8Array.from(response?.body ?? [])));