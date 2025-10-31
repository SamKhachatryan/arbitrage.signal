const wsUri = "ws://localhost:4010";

const websocket = new WebSocket(wsUri);

const diffMap = {};

let prev_package_count = 0;
let package_count = 0;

websocket.onopen = (event) => {
    console.log("WebSocket connection established:", event);
    setInterval(() => {
        console.log('Package got within 10 second:', package_count - prev_package_count);
        prev_package_count = package_count;
    }, 10000);
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
    "op-usdt": 0.8,       // Layer 2 token, frequent spread divergence
};

const getReliability = (pairExchange) => {
    if (Date.now() - pairExchange.last_update_ts < 120 && pairExchange.latency < 100) return 'high';
    if (Date.now() - pairExchange.last_update_ts < 220 && pairExchange.latency < 200) return 'medium';

    return 'low';
}

const riskCoef = 2;

websocket.onmessage = (event) => {
    package_count++;
    const parsed = JSON.parse(event.data);

    Object.entries(parsed).forEach(([pairName, pair]) => {
        const allExchangesMap = Object.entries(pair);

        allExchangesMap.forEach(([exchangeName, pairExchange]) => {
            allExchangesMap.forEach(([otherExchangeName, otherPairExchange]) => {
                if (otherExchangeName !== exchangeName) {
                    const highest = Math.max(pairExchange.price, otherPairExchange.price);
                    const lowest = Math.min(pairExchange.price, otherPairExchange.price);
                    const diffPercent = ((highest - lowest) / lowest) * 100.0;

                    const acceptableThreshold = (arbitrageThresholds[pairName] || 0.5) / riskCoef;

                    if (diffPercent >= acceptableThreshold) {
                        const firstReliability = getReliability(pairExchange);
                        const secondReliability = getReliability(pairExchange);

                        if (firstReliability !== 'low' && secondReliability !== 'low') {
                            console.log('Arbitrage opportunity', pairName, `${exchangeName} (${pairExchange.price}) (${firstReliability})`, '-', `${otherExchangeName} (${otherPairExchange.price}) (${secondReliability})`);
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