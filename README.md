# Mon2tr

Mon2tr is an HTTP/2 client designed specifically for interacting with the Discord API.  
It is **not intended to be a general-purpose HTTP client**, nor will it aim to support HTTP/3/QUIC (no “Mon3tr”).  

## Motivation

This project is my attempt at redemption after the failure of **Kodrst**.  
Kodrst underperformed compared to [undici], despite being based on HTTP/1.1. I struggled to optimize it and eventually realized that a focused HTTP/2 implementation could deliver better performance for the Discord use case.  

Unlike Kodrst, Mon2tr avoids the heavy overhead of constant locking and unnecessary abstractions. 
It relies on `MaybeUninit` instead of `Option` to prevent repeated checks and keep hot paths efficient.
For response handling, it does not use channels (like Tokio’s `mpsc`) but instead implements a permissive spin loop combined with a flag system. Within that loop, it uses `tokio::task::yield_now()` to yield back to the runtime, ensuring other tasks can progress without stalling the executor.  

This design sacrifices some generality in favor of **predictable performance and reduced overhead**, tuned for Discord’s specific HTTP/2 patterns.  

---

[undici]: https://github.com/nodejs/undici
