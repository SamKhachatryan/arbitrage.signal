const wsUri = "ws://localhost:4010";

const websocket = new WebSocket(wsUri);
websocket.binaryType = "arraybuffer"; // important

const diffMap = {};

let prev_package_count = 0;
let package_count = 0;

websocket.onopen = (event) => {
    console.log("WebSocket connection established:", event);
    // setInterval(() => {
    //     console.log('Package got within 10 second:', package_count - prev_package_count);
    //     prev_package_count = package_count;
    // }, 10000);
};

const arbitrageThresholds = {
  "btc-usdt": 0.5,     // High liquidity, tight spreads, fees matter
  "eth-usdt": 0.6,     // Slightly more volatile than BTC
  "sol-usdt": 0.7,     // Mid-cap, exchange-specific variance
  "doge-usdt": 0.8,    // Meme-driven volatility, slippage risk
  "xrp-usdt": 0.7,     // Regulatory-driven price swings
  "ton-usdt": 0.9,     // Newer token, wider spreads
  "ada-usdt": 0.6,     // High volume, moderate spread
  "link-usdt": 0.7,    // DeFi token, exchange-specific gaps
  "arb-usdt": 0.8,     // Governance token, volatile across CEXs
  "op-usdt": 0.8,      // Layer 2 token, frequent spread divergence
  "ltc-usdt": 0.6,     // Legacy coin, decent liquidity
  "bch-usdt": 0.7,     // Forked coin, volatile on low volume
  "uni-usdt": 0.8,     // DeFi token, spread varies by exchange
  "avax-usdt": 0.8,    // Layer 1 token, moderate liquidity
  "apt-usdt": 0.9,     // Newer L1, often wide spreads
  "near-usdt": 0.8,    // L1 with regional exchange variance
  "matic-usdt": 0.7,   // High volume, but fee-sensitive
  "pepe-usdt": 1.2,    // Meme coin, extreme volatility
  "floki-usdt": 1.3,   // Meme coin, low depth, high slippage
  "sui-usdt": 0.9      // Newer token, frequent arbitrage gaps
};

const reliabilityEnum = {
    low: 1,
    medium: 2,
    high: 3,
};

const reliabilityViewEnum = {
    [reliabilityEnum.low]: 'low',
    [reliabilityEnum.medium]: 'medium',
    [reliabilityEnum.high]: 'high',
};

const getReliability = (pairExchange) => {
    if (Date.now() - pairExchange.last_update_ts < 120 && pairExchange.latency < 100) return reliabilityEnum.high;
    if (Date.now() - pairExchange.last_update_ts < 220 && pairExchange.latency < 200) return reliabilityEnum.medium;

    return reliabilityEnum.low;
}

const riskCoef = 6;

const toPairExchange = (binary_arr) => ({
    price: binary_arr[0],
    latency: binary_arr[1],
    last_update_ts: binary_arr[2],
})

websocket.onmessage = (event) => {
    package_count++;
    const bytes = new Uint8Array(event.data);
    const parsed = msgpack.decode(bytes);

    Object.entries(parsed).forEach(([pairName, pair]) => {
        const allExchangesMap = Object.entries(pair);

        allExchangesMap.forEach(([exchangeName, _pairExchange]) => {
            const pairExchange = toPairExchange(_pairExchange);
            allExchangesMap.forEach(([otherExchangeName, _otherPairExchange]) => {
                const otherPairExchange = toPairExchange(_otherPairExchange);
                if (otherExchangeName !== exchangeName) {
                    const highest = Math.max(pairExchange.price, otherPairExchange.price);
                    const lowest = Math.min(pairExchange.price, otherPairExchange.price);
                    const diffPercent = ((highest - lowest) / lowest) * 100.0;
                    const acceptableThreshold = (arbitrageThresholds[pairName] || 0.5) / riskCoef;

                    if (diffPercent >= acceptableThreshold) {
                        const firstReliability = getReliability(pairExchange);
                        const secondReliability = getReliability(pairExchange);

                        const isHighReliability = firstReliability > reliabilityEnum.medium && secondReliability > reliabilityEnum.medium;
                        if (isHighReliability) {
                            console.log(`Arbitrage opportunity (${pairName})`, `${exchangeName} (${pairExchange.price})`, '-', `${otherExchangeName} (${otherPairExchange.price})`, 'Diff percent', '-', Math.round(diffPercent * 100) / 100, '%');
                        }
                    }
                }
            });
            // const otherExchangesMod = otherExchanges.reduce((acc, [, price]) => acc + price * 10000, 0) / otherExchanges.length;

            // diffMap[pairName] ??= {
            //     diff: 0,
            //     exchangeName: exchangeName,
            // };

            // if (Math.abs(price * 10000 - otherExchangesMod) > diffMap[pairName].diff) {
            // diffMap[pairName].diff = Math.abs(price * 10000 - otherExchangesMod);
            // diffMap[pairName].exchangeName = exchangeName
            // }
        });
    });

    draw_diff_map();
};

const draw_diff_map = () => {
    const elements = document.getElementsByClassName('mod-diff-pair');
    Array.from(elements).forEach(item => document.body.removeChild(item));

    Object.entries(diffMap).forEach(([pairName, diff]) => {
        const p = document.createElement('p');
        p.className = 'mod-diff-pair';
        p.textContent = `(${diff.exchangeName}) ${pairName} - ${diff.diff / 10000}`;
        document.body.appendChild(p);
    });
}

websocket.onerror = (error) => {
    console.error("WebSocket error:", error);
};

websocket.onclose = (event) => {
    console.log("WebSocket connection closed:", event);
};

function sendMessage(message) {
    if (websocket.readyState === WebSocket.OPEN) {
        websocket.send(message);
    } else {
        console.warn("WebSocket is not open. Cannot send message.");
    }
}