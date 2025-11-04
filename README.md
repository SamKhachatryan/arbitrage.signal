# arbitrage.signal  
**Rapid WebSocket price signal provider for specific Exchange–Ticker combinations.**

## 🚀 Overview  
**arbitrage.signal** is a high-performance tool designed to deliver ultra-low-latency price signals from cryptocurrency exchanges for specific tickers (e.g., `BTC/USDT`, `ETH/USD`).  
It serves as a foundation for **arbitrage systems**, **real-time analytics**, and **alerting mechanisms** that require streaming updates instead of periodic polling.

### ✨ Key Features  
- 🔌 Connects via WebSocket to multiple exchanges  
- 🎯 Filters and streams only the tickers you care about  
- ⚡ Ultra-low latency Rust backend  
- 📈 Prometheus metrics and monitoring support  
- 🧩 Extensible and modular design — add new exchanges or outputs easily  
- 🌐 Optional JS front-end consumer for visualization or testing  

---

## 🧱 Architecture  
1. **Rust Core** — Manages WebSocket connections, processes messages, filters by exchange/ticker, and emits normalized data.  
2. **Consumer Front-End (JS)** — Connects to the backend via WebSocket or HTTP and displays or logs incoming signals.  
3. **Monitoring Layer** — Integrated Prometheus metrics for message rates, latency, and system health.  
4. **Configuration Layer** — Specify exchanges, tickers, and output modes in a config file.

---

## 📦 Getting Started  

### Prerequisites  
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)  
- [Node.js](https://nodejs.org/) (for running the JS consumer)  
- (Optional) [Prometheus](https://prometheus.io/) for monitoring  

---

### 🔧 Build & Run  

```bash
# Clone the repo
git clone https://github.com/SamKhachatryan/arbitrage.signal.git
cd arbitrage.signal

# Build the Rust backend
cargo build --release

# Run the backend
./target/release/arbitrage.signal --config config.toml
