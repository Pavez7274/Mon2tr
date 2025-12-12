export {}

import { Connection, Codec, Stream } from "../lib/index";

const AUTH = "Bot TOKEN";

export function ft(ns: number): string {
    let value = ns; let unit = "ns";

    if (value >= 1e3) {
        value = value / 1e3; // ns → μs
        unit = "μs";
        if (value >= 1e3) {
            value = value / 1e3; // μs → ms
            unit = "ms";
            if (value >= 1e3) {
                value = value / 1e3; // ms → s
                unit = "s";
            }
        }
    }

    return `${Math.round(value)} ${unit}`;
}

export function fb(bytes: number): string {
  let value = bytes;
  let unit = "b ";

  if (value >= 1024) {
    value = value / 1024; // B → KB
    unit = "kb";
    if (value >= 1024) {
      value = value / 1024; // KB → MB
      unit = "mb";
    }
  }

  return `${Math.round(value).toString().padStart(3)} ${unit}`;
}

console.log();
async function bench(name: string, fn: () => Promise<void> | void) {
    const { heapTotal: ht0, heapUsed: hu0, rss: rss0 } = process.memoryUsage();
    const start = Bun.nanoseconds();
    await fn();

    const time = Bun.nanoseconds() - start;
    const { heapTotal: ht1, heapUsed: hu1, rss: rss1 } = process.memoryUsage();
    console.log(
        `\x1b[1m${name} \x1b[0;2m`.padEnd(50, "."),
        `\x1b[0;95m${ft(time).padEnd(6)}\x1b[0;2m |`,
        `\x1b[0;93m${fb(hu1  + ht1 - hu0 - ht0)}\x1b[0;2m |` ,
        `\x1b[0;91m${fb(rss1       - rss0     )}\x1b[0m`
    );
}

var client: Connection = null!;
await bench("Warm", async () => {
    Codec.warmup(4096);
    client = new Connection(Codec.encode(AUTH));
    await client.connect(); // connect to default ip/port which is discord:443
    await client.handshake();
    client.recv_loop();
});

await bench("Built-in", async () => {
    await fetch("https://discord.com/api/v10/users/@me", {
        method: "GET",
        headers: { "Authorization": AUTH, "User-Agent": "APP (Mon2tr, 1.0)" }
    });
});

await bench("Built-in (2)", async () => {
    await fetch("https://discord.com/api/v10/users/@me", {
        method: "GET",
        headers: { "Authorization": AUTH, "User-Agent": "APP (Mon2tr, 1.0)" }
    });
});

var stream: Stream = null!;
await bench("Mon2tr", async () => {
    stream = client.create_stream();
    const identifier = await stream.send(Codec.encode("GET"), Codec.encode("/api/v10/gateway/bot"), new Uint8Array());
    await client.recv(identifier);
});

await bench("Mon2tr - No client.create_stream()", async () => {
    const identifier = await stream.send(Codec.encode("GET"), Codec.encode("/api/v10/gateway/bot"), new Uint8Array());
    await client.recv(identifier);
});

await bench("Mon2tr - No client.recv()", async () => {
    await stream.send(Codec.encode("GET"), Codec.encode("/api/v10/gateway/bot"), new Uint8Array());
});

// const snowflakes = ["@me", "1305125300816052255", "788869971073040454", "852970774067544165", "1125490330679115847", "738824089128665118"]
// const promises = [];
// for (const _ of new Array(30).fill(0)) {
//     const snowflake = snowflakes[Math.floor(Math.random() * snowflakes.length)];

//     const stream = client.create_stream();
//     const identifier = await stream.send(Codec.encode("GET"), Codec.encode(`/api/v10/users/${snowflake}`), new Uint8Array());

//     promises.push(client.recv(identifier));
// }

// const start = Bun.nanoseconds();
// await Promise.all(promises).then((responses) => {
//     responses.forEach((response) => {
//         console.log("\x1b[1m[ HEADERS ]\x1b[0m");
//         for (const header of response?.headers ?? []) {
//             console.log("%s: %s", header.name, header.value);
//         }

//         console.log("\n\x1b[1m[ BODY (%d) ]\x1b[0m", response?.body?.byteLength ?? 0);
//         console.log(Codec.decode(response?.body ?? new Uint8Array()));
//     });
// });

// const end = Bun.nanoseconds();
// console.log("\n\x1b[1m[ COMPLETED IN %d ns ]\x1b[0m", end - start);

process.exit(0);